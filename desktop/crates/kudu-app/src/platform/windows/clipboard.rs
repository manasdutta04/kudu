//! Pasting through the clipboard, and copying the selection, without losing what the user had
//! on the clipboard.
//!
//! All clipboard work runs on one thread that owns a window: the clipboard's owner must be a
//! window (with no owner, `SetClipboardData` may fail), and that window must keep answering
//! messages, or other apps stall when they take over the clipboard.
//!
//! The text is offered with *delayed rendering*: Windows asks for the data (`WM_RENDERFORMAT`)
//! the moment an app reads it. That tells when the paste actually happened, so the user's
//! clipboard is restored right after the target app read the text, not on a guess, and slow apps
//! don't end up pasting the restored content instead.

use std::cell::RefCell;
use std::thread;
use std::time::{Duration, Instant};

use ::windows::core::PCWSTR;
use ::windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
use ::windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardOwner,
    GetClipboardSequenceNumber, GetOpenClipboardWindow, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use ::windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use ::windows::Win32::UI::WindowsAndMessaging::{
    GetWindowThreadProcessId, KillTimer, MsgWaitForMultipleObjectsEx, PeekMessageW, SetTimer, MSG, MWMO_NONE,
    PM_NOREMOVE, PM_QS_SENDMESSAGE, QS_SENDMESSAGE, WM_DESTROYCLIPBOARD, WM_RENDERALLFORMATS, WM_RENDERFORMAT,
    WM_TIMER,
};
use anyhow::{anyhow, bail, Context};

use super::app;
use super::worker::Worker;

const CF_TEXT: u32 = 1;
const CF_OEMTEXT: u32 = 7;
const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;
const CF_LOCALE: u32 = 16;
const CF_DIBV5: u32 = 17;

/// Registered formats worth keeping. All are plain memory blocks, so copying their bytes is
/// a faithful copy; handle-based formats (bitmaps, metafiles, palettes, private GDI data) are
/// skipped because their bytes alone aren't the data. Apps' private formats are skipped too:
/// reading them can force the source app to render large, slow representations.
const KEPT_FORMATS: &[&str] = &[
    "HTML Format",
    "Rich Text Format",
    "PNG",
    "image/png",
    "Preferred DropEffect",
    "Shell IDList Array",
    "FileNameW",
    "FileName",
];

/// Markers that keep a dictation out of clipboard history (Win + V), the cloud clipboard and
/// clipboard managers. Restored content carries them too: it is already in the history.
const PRIVACY_FORMATS: &[&str] =
    &["ExcludeClipboardContentFromMonitorProcessing", "CanIncludeInClipboardHistory", "CanUploadToCloudClipboard"];

/// Restore no sooner than this after the paste…
const MIN_RESTORE: Duration = Duration::from_millis(400);
/// …and this long after the target app read the text…
const AFTER_READ: Duration = Duration::from_millis(150);
/// …or this long after the paste if the target app never read it.
const MAX_RESTORE: Duration = Duration::from_millis(2500);
const RESTORE_TIMER: usize = 1;
/// Larger single formats aren't kept (a huge bitmap), to bound memory and time.
const MAX_FORMAT_BYTES: usize = 64 << 20;

/// What the user had on the clipboard: (format, bytes) in the order the source app offered
/// them, which is the order of its preference.
#[derive(Default)]
pub struct Saved(Vec<(u32, Vec<u8>)>);

/// A paste waiting for its restore.
struct Pending {
    generation: u64,
    saved: Saved,
    /// UTF-16, CRLF line ends, NUL-terminated.
    text: Vec<u16>,
    started: Instant,
    /// When the target app read the text.
    read_at: Option<Instant>,
    /// Processes of the paste target (an app hosted in a frame has two).
    target: [u32; 2],
    /// Another app took over the clipboard: don't restore over its content.
    lost: bool,
}

struct State {
    hwnd: HWND,
    kept: Vec<u32>,
    privacy: Vec<u32>,
    vscode: u32,
    pending: Option<Pending>,
    generation: u64,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|cell| cell.try_borrow_mut().ok()?.as_mut().map(f))
}

pub struct Clipboard {
    worker: Worker,
}

const CALL_TIMEOUT: Duration = Duration::from_millis(1500);

impl Clipboard {
    pub fn spawn() -> anyhow::Result<Self> {
        let worker = Worker::spawn("kudu-clipboard", Some(handle_message), |hwnd| {
            let register = |name: &str| {
                let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
                // SAFETY: `wide` is NUL-terminated and outlives the call.
                unsafe { RegisterClipboardFormatW(PCWSTR(wide.as_ptr())) }
            };
            let state = State {
                hwnd,
                kept: KEPT_FORMATS.iter().map(|name| register(name)).filter(|&id| id != 0).collect(),
                privacy: PRIVACY_FORMATS.iter().map(|name| register(name)).filter(|&id| id != 0).collect(),
                vscode: register("vscode-editor-data"),
                pending: None,
                generation: 0,
            };
            STATE.with(|cell| *cell.borrow_mut() = Some(state));
            Ok(())
        })?;
        Ok(Self { worker })
    }

    /// Puts `text` on the clipboard (the user's content is kept to be restored after the
    /// paste). Ctrl + V must be sent right after this returns.
    pub fn prepare_paste(&self, text: &str) -> anyhow::Result<()> {
        let text = to_clipboard_text(text);
        let target = app::foreground_processes();
        self.worker
            .call(CALL_TIMEOUT, move || prepare_paste(text, target))
            .unwrap_or_else(|| Err(anyhow!("the clipboard thread didn't answer")))
    }

    /// Copies the selection of the focused app with Ctrl + C and restores the clipboard.
    pub fn copy_selection(&self) -> Option<String> {
        let (saved, ours, sequence) = match self.worker.call(CALL_TIMEOUT, prepare_copy)? {
            Ok(prepared) => prepared,
            Err(error) => {
                log::warn!("Couldn't save the clipboard before copying: {error:#}");
                return None;
            }
        };
        super::input::wait_for_modifiers_released(Duration::from_millis(300));
        let mut changed = false;
        if super::input::send_ctrl_chord(super::keys::VK_C as u16) {
            let deadline = Instant::now() + Duration::from_millis(400);
            while Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
                // SAFETY: plain query.
                if unsafe { GetClipboardSequenceNumber() } != sequence {
                    changed = true;
                    break;
                }
            }
        }
        if changed {
            // The app may still be adding formats; let it finish.
            thread::sleep(Duration::from_millis(15));
        }
        if !changed && !ours {
            return None; // Nothing was copied and the clipboard is untouched.
        }
        self.worker.call(CALL_TIMEOUT, move || finish_copy(saved, changed)).flatten()
    }
}

/// Text in the clipboard's format: UTF-16 with Windows line ends, NUL-terminated.
fn to_clipboard_text(text: &str) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len() + 1);
    let mut previous = '\0';
    for c in text.chars() {
        if c == '\n' && previous != '\r' {
            out.push(b'\r' as u16);
        }
        let mut units = [0u16; 2];
        out.extend_from_slice(c.encode_utf16(&mut units));
        previous = c;
    }
    out.push(0);
    out
}

/// An open clipboard, closed on drop.
struct Open;

impl Open {
    /// Opens the clipboard, retrying for a moment: another app may have it open.
    fn new(hwnd: HWND) -> anyhow::Result<Self> {
        for attempt in 0..12u64 {
            // SAFETY: plain call; the clipboard is closed again by `Drop`.
            if unsafe { OpenClipboard(Some(hwnd)) }.is_ok() {
                return Ok(Open);
            }
            wait_answering_sent_messages(Duration::from_millis(5 + attempt * 3));
        }
        bail!("the clipboard is held by another app")
    }
}

/// Waits while still answering messages other apps send to our window. The app holding the
/// clipboard open may be the one pasting, waiting for us to render the text (WM_RENDERFORMAT);
/// a plain sleep would keep both sides waiting until the retries run out.
fn wait_answering_sent_messages(duration: Duration) {
    let deadline = Instant::now() + duration;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return;
        }
        // SAFETY: waiting with no handles; PeekMessage with PM_QS_SENDMESSAGE only delivers
        // sent messages to our window procedures and leaves posted ones queued.
        unsafe {
            MsgWaitForMultipleObjectsEx(None, remaining.as_millis() as u32, QS_SENDMESSAGE, MWMO_NONE);
            let mut msg = MSG::default();
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE | PM_QS_SENDMESSAGE);
        }
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: we opened the clipboard on this thread.
        let _ = unsafe { CloseClipboard() };
    }
}

/// Copies a memory-block clipboard handle's bytes.
fn read_global(handle: HANDLE) -> Option<Vec<u8>> {
    let memory = HGLOBAL(handle.0);
    // SAFETY: `memory` is a clipboard handle of a memory-block format, valid while the clipboard
    // is open; it's only read between GlobalLock and GlobalUnlock, within GlobalSize bytes.
    unsafe {
        let size = GlobalSize(memory);
        if size == 0 || size > MAX_FORMAT_BYTES {
            return None;
        }
        let pointer = GlobalLock(memory) as *const u8;
        if pointer.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(pointer, size).to_vec();
        let _ = GlobalUnlock(memory);
        Some(bytes)
    }
}

/// Puts bytes on the open clipboard. The system owns the memory once this succeeds.
fn set_data(format: u32, bytes: &[u8]) -> anyhow::Result<()> {
    // SAFETY: a fresh block of `len` bytes is filled within bounds, then handed to the clipboard
    // or freed if that fails.
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1))?;
        let pointer = GlobalLock(memory) as *mut u8;
        if pointer.is_null() {
            let _ = GlobalFree(Some(memory));
            bail!("GlobalLock failed");
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer, bytes.len());
        let _ = GlobalUnlock(memory);
        if let Err(error) = SetClipboardData(format, Some(HANDLE(memory.0))) {
            let _ = GlobalFree(Some(memory));
            return Err(error).with_context(|| format!("SetClipboardData({format})"));
        }
    }
    Ok(())
}

fn utf16_bytes(units: &[u16]) -> Vec<u8> {
    units.iter().flat_map(|unit| unit.to_le_bytes()).collect()
}

/// Reads the formats worth keeping from the open clipboard.
fn read_saved(state_kept: &[u32]) -> Saved {
    let mut formats = Vec::new();
    let mut format = 0;
    loop {
        // SAFETY: plain enumeration of the open clipboard.
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            break;
        }
        formats.push(format);
    }
    let has = |f: u32| formats.contains(&f);
    let mut saved = Vec::new();
    for &format in &formats {
        let keep = match format {
            CF_UNICODETEXT | CF_HDROP | CF_LOCALE => true,
            // Windows makes these from CF_UNICODETEXT (with CF_LOCALE) again by itself.
            CF_TEXT | CF_OEMTEXT => !has(CF_UNICODETEXT),
            // One bitmap is enough: Windows converts between the two.
            CF_DIBV5 => true,
            CF_DIB => !has(CF_DIBV5),
            _ => state_kept.contains(&format),
        };
        if !keep {
            continue;
        }
        // SAFETY: the clipboard is open; the handle is only used while it stays open.
        if let Ok(handle) = unsafe { GetClipboardData(format) } {
            if let Some(bytes) = read_global(handle) {
                saved.push((format, bytes));
            }
        }
    }
    Saved(saved)
}

/// Replaces the open clipboard's content with `saved` (plus the privacy markers).
fn write_saved(saved: &Saved, privacy: &[u32]) {
    // SAFETY: the clipboard is open on this thread.
    if let Err(error) = unsafe { EmptyClipboard() } {
        log::warn!("Couldn't empty the clipboard to restore it: {error}");
        return;
    }
    for (format, bytes) in &saved.0 {
        if let Err(error) = set_data(*format, bytes) {
            log::debug!("Couldn't restore clipboard format {format}: {error:#}");
        }
    }
    if !saved.0.is_empty() {
        set_privacy_markers(privacy);
    }
}

fn set_privacy_markers(privacy: &[u32]) {
    for &format in privacy {
        let _ = set_data(format, &0u32.to_le_bytes());
    }
}

/// Whether we still own the clipboard with the content we put there.
fn still_ours(hwnd: HWND) -> bool {
    // SAFETY: plain query.
    unsafe { GetClipboardOwner() }.is_ok_and(|owner| owner == hwnd)
}

/// Takes the pending paste if the clipboard still holds its text: its saved content is then the
/// user's real clipboard. Call with the clipboard open, so the paste can't be read (and need
/// rendering) after it's taken.
fn take_pending(state: &mut State) -> Option<Pending> {
    let pending = state.pending.take()?;
    // SAFETY: stopping our own timer.
    let _ = unsafe { KillTimer(Some(state.hwnd), RESTORE_TIMER) };
    (!pending.lost && still_ours(state.hwnd)).then_some(pending)
}

fn set_pending(state: &mut State, pending: Pending) {
    state.pending = Some(pending);
    // SAFETY: a timer on our own window.
    unsafe { SetTimer(Some(state.hwnd), RESTORE_TIMER, 25, None) };
}

fn prepare_paste(text: Vec<u16>, target: [u32; 2]) -> anyhow::Result<()> {
    let (hwnd, kept, privacy) = with_state(|s| (s.hwnd, s.kept.clone(), s.privacy.clone()))
        .ok_or_else(|| anyhow!("clipboard state unavailable"))?;
    let open = Open::new(hwnd)?;
    let previous = with_state(take_pending).flatten();
    let saved = match previous {
        Some(previous) => previous.saved,
        None => read_saved(&kept),
    };
    // SAFETY: the clipboard is open on this thread.
    let offered = unsafe { EmptyClipboard() }
        .context("EmptyClipboard")
        // A null handle offers the format and renders it on request (WM_RENDERFORMAT).
        // SAFETY: as above.
        .and_then(|()| unsafe { SetClipboardData(CF_UNICODETEXT, None) }.context("SetClipboardData"));
    let ok = offered.is_ok();
    if ok {
        set_privacy_markers(&privacy);
    }
    // Registered while the clipboard is still open, so a read right after closing finds the
    // text. On failure the user's content is still restored later.
    with_state(|s| {
        s.generation += 1;
        let pending = Pending {
            generation: s.generation,
            saved,
            text: if ok { text } else { vec![0] },
            started: Instant::now(),
            read_at: None,
            target,
            lost: false,
        };
        set_pending(s, pending);
    });
    drop(open);
    offered.map(|_| ())
}

/// Saves the clipboard before a copy. Returns the content to restore, whether the clipboard
/// currently holds one of our pastes, and the clipboard's sequence number.
fn prepare_copy() -> anyhow::Result<(Saved, bool, u32)> {
    let (hwnd, kept) =
        with_state(|s| (s.hwnd, s.kept.clone())).ok_or_else(|| anyhow!("clipboard state unavailable"))?;
    let open = Open::new(hwnd)?;
    let (saved, ours) = match with_state(take_pending).flatten() {
        Some(pending) => (pending.saved, true),
        None => (read_saved(&kept), false),
    };
    drop(open);
    // SAFETY: plain query.
    Ok((saved, ours, unsafe { GetClipboardSequenceNumber() }))
}

/// Reads the copied text (if `changed`) and restores `saved`.
fn finish_copy(saved: Saved, changed: bool) -> Option<String> {
    let (hwnd, privacy, vscode) = with_state(|s| (s.hwnd, s.privacy.clone(), s.vscode))?;
    let open = match Open::new(hwnd) {
        Ok(open) => open,
        Err(error) => {
            log::warn!("Couldn't read the copied selection: {error:#}");
            return None;
        }
    };
    let text = if changed { read_text(vscode) } else { None };
    write_saved(&saved, &privacy);
    drop(open);
    text.filter(|t| !t.is_empty())
}

fn read_text(vscode: u32) -> Option<String> {
    // Code editors copy the whole line when nothing is selected; that isn't a selection.
    if vscode != 0 {
        // SAFETY: the clipboard is open.
        if let Ok(handle) = unsafe { GetClipboardData(vscode) } {
            if let Some(bytes) = read_global(handle) {
                let json = String::from_utf8_lossy(&bytes);
                if json.contains("\"isFromEmptySelection\":true") {
                    return None;
                }
            }
        }
    }
    // SAFETY: the clipboard is open.
    let handle = unsafe { GetClipboardData(CF_UNICODETEXT) }.ok()?;
    let bytes = read_global(handle)?;
    let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|&pair| u16::from_le_bytes(pair)).collect();
    let end = units.iter().position(|&unit| unit == 0).unwrap_or(units.len());
    Some(String::from_utf16_lossy(&units[..end]).replace("\r\n", "\n"))
}

/// Puts the pending text on the clipboard, which the requesting app has open.
fn render(state: &mut State) {
    let Some(pending) = state.pending.as_mut() else { return };
    if let Err(error) = set_data(CF_UNICODETEXT, &utf16_bytes(&pending.text)) {
        log::warn!("Couldn't render the pasted text: {error:#}");
        return;
    }
    // Only a read by the paste target counts; a clipboard monitor reading it first doesn't
    // mean the paste happened. An app that opened the clipboard without a window can't be
    // told apart, and is taken to be the target.
    // SAFETY: plain queries.
    let reader = unsafe { GetOpenClipboardWindow() }.ok().filter(|hwnd| !hwnd.is_invalid());
    let by_target = match reader {
        None => true,
        Some(hwnd) => {
            let mut pid = 0;
            // SAFETY: plain query with a valid out pointer.
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
            pid != 0 && pending.target.contains(&pid)
        }
    };
    if by_target {
        pending.read_at = Some(Instant::now());
    } else {
        log::debug!("The pasted text was read by another app first");
    }
}

/// Whether the pending paste is due to be restored (its generation), dropping it if another
/// app took over the clipboard meanwhile.
fn restore_due(state: &mut State) -> Option<u64> {
    let stop = |state: &mut State| {
        // SAFETY: stopping our own timer.
        let _ = unsafe { KillTimer(Some(state.hwnd), RESTORE_TIMER) };
    };
    let Some(pending) = state.pending.as_ref() else {
        stop(state);
        return None;
    };
    if pending.lost {
        state.pending = None; // The user (or an app) put something else on the clipboard since.
        stop(state);
        return None;
    }
    let due = match pending.read_at {
        Some(read) => (pending.started + MIN_RESTORE).max(read + AFTER_READ),
        None => pending.started + MAX_RESTORE,
    };
    (Instant::now() >= due).then_some(pending.generation)
}

/// Puts the user's content back. The pending paste stays registered until the clipboard is
/// open, so a read that's in progress can still be rendered; if the clipboard can't be opened
/// now, the timer tries again.
fn restore_pending(hwnd: HWND, generation: u64) {
    let open = match Open::new(hwnd) {
        Ok(open) => open,
        Err(error) => {
            let give_up = with_state(|s| {
                let expired = s.pending.as_ref().is_some_and(|p| p.started.elapsed() > MAX_RESTORE * 3);
                if expired {
                    s.pending = None;
                }
                expired
            });
            if give_up == Some(true) {
                log::warn!("Couldn't restore the clipboard: {error:#}");
            }
            return;
        }
    };
    let restore = with_state(|s| {
        if s.pending.as_ref().is_none_or(|p| p.generation != generation) {
            return None;
        }
        let pending = take_pending(s)?;
        Some((pending.saved, s.privacy.clone()))
    })
    .flatten();
    if let Some((saved, privacy)) = restore {
        log::debug!("Restoring the clipboard after paste {generation}");
        write_saved(&saved, &privacy);
    }
    drop(open);
}

fn handle_message(hwnd: HWND, msg: u32, wparam: WPARAM, _lparam: LPARAM) -> Option<LRESULT> {
    match msg {
        WM_RENDERFORMAT => {
            if wparam.0 as u32 == CF_UNICODETEXT {
                with_state(render);
            }
            Some(LRESULT(0))
        }
        WM_RENDERALLFORMATS => {
            // Kudu is closing while a paste is pending: put the text there for real.
            if let Ok(open) = Open::new(hwnd) {
                if still_ours(hwnd) {
                    with_state(render);
                }
                drop(open);
            }
            Some(LRESULT(0))
        }
        WM_DESTROYCLIPBOARD => {
            // Someone emptied the clipboard we own. Kudu only empties it itself after taking the
            // pending paste, so this marks another app's (or the user's) new content.
            with_state(|s| {
                if let Some(pending) = s.pending.as_mut() {
                    pending.lost = true;
                }
            });
            Some(LRESULT(0))
        }
        WM_TIMER if wparam.0 == RESTORE_TIMER => {
            if let Some(generation) = with_state(restore_due).flatten() {
                restore_pending(hwnd, generation);
            }
            Some(LRESULT(0))
        }
        _ => None,
    }
}
