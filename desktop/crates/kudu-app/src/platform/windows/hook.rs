//! The global keyboard listener: a `WH_KEYBOARD_LL` hook on its own thread.
//!
//! Every key press on the system waits for this hook to return, and Windows silently removes
//! hooks that take longer than `LowLevelHooksTimeout`. So the hook runs on a dedicated,
//! high-priority thread that does nothing else (the overlay and tray can't delay it), and the
//! callback only updates a little state, asks the matcher, and sends events on a channel.
//!
//! Two safety nets keep it alive and in sync:
//! - a raw-input listener counts key presses independently of the hook; if keys arrive but the
//!   hook hasn't run for a while, Windows dropped it, and it is installed again;
//! - when the input desktop changes (lock screen, UAC prompt, Ctrl + Alt + Del), the hook can't
//!   see the key releases that happen there, so the held keys are forgotten.

use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ::windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::System::SystemInformation::GetTickCount64;
use ::windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL};
use ::windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use ::windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use ::windows::Win32::UI::Input::{RegisterRawInputDevices, RAWINPUTDEVICE, RIDEV_INPUTSINK};
use ::windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, EVENT_SYSTEM_DESKTOPSWITCH, HC_ACTION, HHOOK,
    KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED, LLKHF_UP, WH_KEYBOARD_LL, WINEVENT_OUTOFCONTEXT, WM_INPUT,
};
use crossbeam_channel::Sender;
use kudu_core::shortcut::{HotkeyEvent, Key, Matcher, Modifier, Notation, Recorded, Recorder, Shortcut};
use parking_lot::Mutex;

use super::input::{self, KUDU_INPUT_TAG};
use super::keys::{self, MENU_KEY_BITS, MODIFIER_KEYS, VK_ESCAPE};
use super::worker::Worker;
use crate::platform::PlatformEvent;

/// How long a key's auto-repeat can pause: a press of a key we think is down, after longer
/// than this, is a new press (its release was missed).
const REPEAT_GAP_MS: u32 = 1500;
/// After this long without a hook call while raw input reports keys, the hook is reinstalled.
const HOOK_SILENCE_MS: u64 = 1000;

pub struct Hook {
    worker: Worker,
    failed: Arc<AtomicBool>,
}

impl Hook {
    /// Starts the hook thread. The hook itself is installed by [`Hook::start`] or
    /// [`Hook::record`].
    pub fn spawn(events: Sender<PlatformEvent>) -> anyhow::Result<Self> {
        let worker = Worker::spawn("kudu-keyboard", Some(handle_message), move |hwnd| {
            // SAFETY: raising our own thread's priority.
            if let Err(error) = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL) } {
                log::warn!("Couldn't raise the keyboard thread's priority: {error}");
            }
            STATE.with(|state| *state.borrow_mut() = Some(State::new(events)));
            watch_raw_keyboard(hwnd);
            // SAFETY: an out-of-context WinEvent hook calls back on this thread, which pumps
            // messages.
            let desktop_hook = unsafe {
                SetWinEventHook(
                    EVENT_SYSTEM_DESKTOPSWITCH,
                    EVENT_SYSTEM_DESKTOPSWITCH,
                    None,
                    Some(desktop_switched),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                )
            };
            if desktop_hook.is_invalid() {
                log::warn!("Couldn't watch for desktop switches");
            }
            Ok(())
        })?;
        Ok(Self { worker, failed: Arc::new(AtomicBool::new(false)) })
    }

    /// Feeds key events to `matcher` from now on.
    pub fn start(&self, matcher: Arc<Mutex<Matcher>>, events: Sender<PlatformEvent>) -> anyhow::Result<()> {
        let result = self
            .worker
            .call(Duration::from_secs(2), move || {
                with_state(|state| {
                    state.matcher = Some(matcher);
                    state.events = events;
                    state.install()
                })
                .unwrap_or_else(|| Err(anyhow::anyhow!("the keyboard hook state is busy")))
            })
            .unwrap_or_else(|| Err(anyhow::anyhow!("the keyboard thread didn't answer")));
        self.failed.store(result.is_err(), Ordering::Relaxed);
        result
    }

    pub fn record(&self) {
        let failed = self.failed.clone();
        self.worker.post(move || {
            with_state(|state| {
                state.recorder = Some(Recorder::new());
                if let Err(error) = state.install() {
                    log::error!("Couldn't install the keyboard hook to record a shortcut: {error:#}");
                    failed.store(true, Ordering::Relaxed);
                }
            });
        });
    }

    pub fn cancel_recording(&self) {
        self.worker.post(|| {
            with_state(|state| state.recorder = None);
        });
    }

    /// Whether installing the hook failed (shortcuts can't work).
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// When the hook last ran (`GetTickCount64`), read by the raw-input watchdog.
static LAST_HOOK_CALL: AtomicU64 = AtomicU64::new(0);

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|cell| {
        let mut borrowed = cell.try_borrow_mut().ok()?;
        borrowed.as_mut().map(f)
    })
}

/// A set of 256 virtual-key codes.
#[derive(Default)]
struct KeySet([u64; 4]);

impl KeySet {
    fn contains(&self, vk: u32) -> bool {
        self.0[(vk as usize >> 6) & 3] & (1 << (vk & 63)) != 0
    }
    fn insert(&mut self, vk: u32) {
        self.0[(vk as usize >> 6) & 3] |= 1 << (vk & 63);
    }
    /// Removes `vk`, returning whether it was there.
    fn remove(&mut self, vk: u32) -> bool {
        let was = self.contains(vk);
        self.0[(vk as usize >> 6) & 3] &= !(1 << (vk & 63));
        was
    }
}

struct State {
    hook: Option<HHOOK>,
    matcher: Option<Arc<Mutex<Matcher>>>,
    events: Sender<PlatformEvent>,
    recorder: Option<Recorder>,
    /// Physical modifiers held, one bit per entry of `MODIFIER_KEYS`.
    held: u8,
    /// Modifiers whose press was hidden from the system: while recording, or because the press
    /// started one of Kudu's shortcuts (so Alt alone doesn't reach an app's menu bar, or Win
    /// the Start menu). Their releases are hidden too, unless they were replayed.
    swallowed_modifiers: u8,
    /// Win/Alt keys pressed with no other key since: releasing one now would open the Start
    /// menu or the menu bar.
    unmasked: u8,
    /// Regular keys down, for detecting auto-repeat.
    down: KeySet,
    /// Regular keys whose press was hidden, so their repeats and release are hidden too.
    swallowed: KeySet,
    last_event: [u32; 256],
    /// Raw-input key events seen while the hook stayed silent.
    missed: u32,
}

impl State {
    fn new(events: Sender<PlatformEvent>) -> Self {
        Self {
            hook: None,
            matcher: None,
            events,
            recorder: None,
            held: 0,
            swallowed_modifiers: 0,
            unmasked: 0,
            down: KeySet::default(),
            swallowed: KeySet::default(),
            last_event: [0; 256],
            missed: 0,
        }
    }

    fn install(&mut self) -> anyhow::Result<()> {
        if self.hook.is_some() {
            return Ok(());
        }
        // SAFETY: installs `keyboard_proc` for this thread, which pumps messages.
        let hook =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(GetModuleHandleW(None)?.into()), 0)? };
        // SAFETY: plain query.
        LAST_HOOK_CALL.store(unsafe { GetTickCount64() }, Ordering::Relaxed);
        self.hook = Some(hook);
        log::info!("Keyboard hook installed");
        Ok(())
    }

    fn reinstall(&mut self) {
        if let Some(hook) = self.hook.take() {
            // SAFETY: `hook` came from SetWindowsHookExW on this thread.
            let _ = unsafe { UnhookWindowsHookEx(hook) };
        }
        if let Err(error) = self.install() {
            log::error!("Couldn't reinstall the keyboard hook: {error:#}");
        }
    }

    fn send(&self, event: PlatformEvent) {
        let _ = self.events.send(event);
    }

    fn held_modifiers(&self) -> ([Modifier; 8], usize) {
        let mut list = [MODIFIER_KEYS[0].1; 8];
        let mut count = 0;
        for (bit, (_, modifier)) in MODIFIER_KEYS.iter().enumerate() {
            if self.held & (1 << bit) != 0 {
                list[count] = *modifier;
                count += 1;
            }
        }
        (list, count)
    }

    /// Forgets modifiers that Windows says are up: their release happened where the hook
    /// couldn't see it. Hidden presses are skipped (Windows never saw them go down), and so are
    /// keys Kudu released synthetically (Windows thinks they're up; the user still holds them).
    fn prune_held(&mut self, except_bit: usize) {
        let forced_up = input::FORCED_UP.load(Ordering::Relaxed);
        for (bit, (vk, _)) in MODIFIER_KEYS.iter().enumerate() {
            let mask = 1u8 << bit;
            if bit == except_bit || self.held & mask == 0 || (self.swallowed_modifiers | forced_up) & mask != 0 {
                continue;
            }
            // SAFETY: plain query. During the hook it reflects the events before this one.
            if unsafe { GetAsyncKeyState(*vk as i32) } >= 0 {
                self.held &= !mask;
            }
        }
    }

    /// Returns whether to hide the event from the rest of the system.
    fn on_key(&mut self, info: &KBDLLHOOKSTRUCT) -> bool {
        let vk = info.vkCode;
        if vk > 255 {
            return false;
        }
        let up = info.flags.0 & LLKHF_UP.0 != 0;
        let extended = info.flags.0 & LLKHF_EXTENDED.0 != 0;
        // AltGr is sent as a fake Left Ctrl (scan code 0x21D) followed by Right Alt. The fake
        // Ctrl must not count as a held Ctrl, or Right Alt shortcuts would never match.
        if vk == 0xA2 && info.scanCode & 0x200 != 0 {
            return false;
        }
        let now = Instant::now();
        match keys::modifier_bit(vk, info.scanCode, extended) {
            Some(bit) => self.on_modifier(bit, up, now),
            None => self.on_regular_key(vk, info.scanCode, extended, up, info.time, now),
        }
    }

    fn on_modifier(&mut self, bit: usize, up: bool, now: Instant) -> bool {
        let mask = 1u8 << bit;
        let before = self.held;
        // A physical event for this key: Windows' state for it is right again.
        input::FORCED_UP.fetch_and(!mask, Ordering::Relaxed);
        if up {
            self.held &= !mask;
            self.unmasked &= !mask;
        } else {
            if self.held & mask == 0 && MENU_KEY_BITS & mask != 0 {
                self.unmasked |= mask;
            }
            self.held |= mask;
        }
        self.prune_held(bit);
        let changed = self.held != before;

        if self.recorder.is_some() {
            // Hide modifiers while recording (so recording Win doesn't open the Start menu),
            // but only releases whose press was hidden too.
            let swallow = if up {
                let was = self.swallowed_modifiers & mask != 0;
                self.swallowed_modifiers &= !mask;
                was
            } else {
                self.swallowed_modifiers |= mask;
                true
            };
            if changed {
                let (list, count) = self.held_modifiers();
                let result = self.recorder.as_mut().map(|r| r.modifiers_changed(&list[..count]));
                if let Some(result) = result {
                    self.recorded(result);
                }
            }
            return swallow;
        }

        // Modifiers reach the system, unless their press starts one of Kudu's shortcuts: then
        // the shortcut belongs to Kudu alone, and the press (and later its release) is hidden.
        // A release is hidden exactly when its press was.
        let mut swallow = up && self.swallowed_modifiers & mask != 0;
        if up {
            self.swallowed_modifiers &= !mask;
        }
        if changed {
            let (list, count) = self.held_modifiers();
            if let Some((events, holding)) = self.match_with(|m| m.modifiers_changed(&list[..count], now)) {
                let starts = events.iter().any(|e| matches!(e, HotkeyEvent::Pressed | HotkeyEvent::CommandPressed));
                if !up && starts && holding {
                    swallow = true;
                    self.swallowed_modifiers |= mask;
                    // The system never sees this press, so there's no menu to stop on release.
                    self.unmasked &= !mask;
                } else if !up && events.contains(&HotkeyEvent::Cancelled) && self.swallowed_modifiers != 0 {
                    // Another modifier joined: it's the user's own shortcut (Alt + Shift…).
                    // Give the system the hidden presses first, then this one.
                    input::replay_hidden(self.swallowed_modifiers, input::Then::Modifier(bit));
                    self.swallowed_modifiers = 0;
                    swallow = true;
                }
                self.after_match(&events, holding);
            }
        }
        swallow
    }

    fn on_regular_key(&mut self, vk: u32, scan: u32, extended: bool, up: bool, time: u32, now: Instant) -> bool {
        let repeat = !up && self.down.contains(vk) && time.wrapping_sub(self.last_event[vk as usize]) < REPEAT_GAP_MS;
        self.last_event[vk as usize] = time;
        if up {
            self.down.remove(vk);
        } else {
            self.down.insert(vk);
        }

        if self.recorder.is_some() {
            if up {
                return self.swallowed.remove(vk);
            }
            self.swallowed.insert(vk);
            if !repeat {
                let key = Key { code: vk, label: keys::key_label(vk, scan, extended) };
                let result = self.recorder.as_mut().map(|r| r.key_down(key, VK_ESCAPE));
                if let Some(result) = result {
                    self.recorded(result);
                }
            }
            return true;
        }

        let mut swallow = false;
        if let Some((outcome_swallow, events, holding)) = self.match_key(|m| {
            let outcome = if up { m.key_up(vk, now) } else { m.key_down(vk, repeat, now) };
            (outcome.swallow, outcome.events, m.is_holding())
        }) {
            swallow = outcome_swallow;
            self.after_match(&events, holding);
        }
        if up {
            swallow |= self.swallowed.remove(vk);
        } else if repeat {
            swallow |= self.swallowed.contains(vk);
        } else if swallow {
            self.swallowed.insert(vk);
        } else {
            self.swallowed.remove(vk);
        }
        if !swallow {
            // The system saw a key while Win/Alt were down: their release won't open a menu.
            self.unmasked = 0;
            if !up && self.swallowed_modifiers != 0 {
                // A key pressed while a hidden shortcut key is held (Alt + Tab, Win + E…): the
                // user's own shortcut, and Kudu has let go of its hold. Give the system the
                // hidden presses, then this key; its release passes as usual.
                input::replay_hidden(self.swallowed_modifiers, input::Then::Key { vk, scan, extended });
                self.swallowed_modifiers = 0;
                return true;
            }
        }
        swallow
    }

    /// Runs `f` on the matcher (if any), returning its events and whether a shortcut is held.
    fn match_with(
        &self,
        f: impl FnOnce(&mut Matcher) -> kudu_core::shortcut::Outcome,
    ) -> Option<(Vec<HotkeyEvent>, bool)> {
        self.match_key(|m| {
            let outcome = f(m);
            (outcome.swallow, outcome.events, m.is_holding())
        })
        .map(|(_, events, holding)| (events, holding))
    }

    fn match_key<R>(&self, f: impl FnOnce(&mut Matcher) -> R) -> Option<R> {
        let matcher = self.matcher.as_ref()?;
        // Never block the hook: the app only holds this lock for a moment.
        let Some(mut guard) = matcher.try_lock_for(Duration::from_millis(20)) else {
            log::warn!("The shortcut matcher was busy; a key event went unmatched");
            return None;
        };
        Some(f(&mut guard))
    }

    fn after_match(&mut self, events: &[HotkeyEvent], holding: bool) {
        for event in events {
            self.send(PlatformEvent::Hotkey(*event));
        }
        let used = holding
            || events
                .iter()
                .any(|e| matches!(e, HotkeyEvent::Pressed | HotkeyEvent::CommandPressed | HotkeyEvent::PasteLast | HotkeyEvent::Meeting));
        if used && self.unmasked != 0 {
            // A Kudu shortcut includes Win or Alt: press the mask key now, while they're still
            // down, so their release doesn't open the Start menu or the menu bar. (Injected
            // from inside the hook, it reaches the system after the current event, and so
            // before the release.)
            input::send_mask_key();
            self.unmasked = 0;
        }
    }

    fn recorded(&mut self, result: Recorded) {
        match result {
            Recorded::Pending => {
                let preview = self.recorder.as_ref().map(|r| r.preview()).unwrap_or_default();
                let caps = Shortcut::new(preview, None).keycaps(Notation::Windows);
                self.send(PlatformEvent::RecordingPreview(caps));
            }
            done => {
                self.recorder = None;
                self.send(PlatformEvent::Recorded(done));
            }
        }
    }

    /// The input desktop changed: releases there are invisible to the hook, so start over.
    fn reset(&mut self) {
        let now = Instant::now();
        self.held = 0;
        self.swallowed_modifiers = 0;
        self.unmasked = 0;
        self.down = KeySet::default();
        self.swallowed = KeySet::default();
        if let Some((events, _)) = self.match_with(|m| m.modifiers_changed(&[], now)) {
            for event in events {
                self.send(PlatformEvent::Hotkey(event));
            }
        }
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: plain query.
        LAST_HOOK_CALL.store(unsafe { GetTickCount64() }, Ordering::Relaxed);
        // SAFETY: for HC_ACTION, `lparam` points to a KBDLLHOOKSTRUCT valid during this call.
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        // Kudu's own synthesized keys (the paste, the mask key) pass untouched.
        let ours = info.flags.0 & LLKHF_INJECTED.0 != 0 && info.dwExtraInfo == KUDU_INPUT_TAG;
        if !ours {
            let swallow = catch_unwind(AssertUnwindSafe(|| with_state(|state| state.on_key(info)).unwrap_or(false)))
                .unwrap_or_else(|_| {
                    log::error!("The keyboard hook panicked");
                    false
                });
            if swallow {
                return LRESULT(1);
            }
        }
    }
    // SAFETY: passing the event on to the next hook unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn desktop_switched(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    log::debug!("Input desktop switched; resetting held keys");
    let _ = catch_unwind(|| with_state(State::reset));
}

/// Registers for raw keyboard input on the hook thread's window (see the module docs).
fn watch_raw_keyboard(hwnd: HWND) {
    let device = RAWINPUTDEVICE { usUsagePage: 0x01, usUsage: 0x06, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd };
    // SAFETY: `device` is a valid registration for a window of this thread.
    if let Err(error) = unsafe { RegisterRawInputDevices(&[device], std::mem::size_of::<RAWINPUTDEVICE>() as u32) } {
        log::warn!("Couldn't watch raw keyboard input; a dropped hook won't be noticed: {error}");
    }
}

fn handle_message(_hwnd: HWND, msg: u32, _wparam: WPARAM, _lparam: LPARAM) -> Option<LRESULT> {
    if msg == WM_INPUT {
        // The hook runs before Windows produces the raw input for a key, so a key that shows
        // up here without a recent hook call means the hook is gone.
        // SAFETY: plain query.
        let silent_for = unsafe { GetTickCount64() }.saturating_sub(LAST_HOOK_CALL.load(Ordering::Relaxed));
        with_state(|state| {
            if state.hook.is_none() {
                return;
            }
            if silent_for > HOOK_SILENCE_MS {
                state.missed += 1;
                if state.missed >= 3 {
                    log::warn!("The keyboard hook stopped receiving keys; reinstalling it");
                    state.missed = 0;
                    state.reinstall();
                }
            } else {
                state.missed = 0;
            }
        });
    }
    // WM_INPUT must still reach DefWindowProc, which frees the raw input data.
    None
}
