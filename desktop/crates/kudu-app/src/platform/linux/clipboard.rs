//! The clipboard, for pasting dictated text and copying the selection.
//!
//! On X11 the clipboard has no storage: the app that copied serves the text when another app
//! pastes. So the `arboard::Clipboard` lives as long as Kudu (its worker thread is the owner);
//! dropping it right after setting the text would leave nothing to paste.
//!
//! On Wayland, arboard uses the data-control protocol (KDE, wlroots compositors). GNOME
//! doesn't offer it; there arboard falls back to XWayland's clipboard, which Mutter bridges to
//! Wayland apps, and as a last resort we run `wl-copy` / `wl-paste`.

use std::time::Duration;

use arboard::{GetExtLinux, LinuxClipboardKind, SetExtLinux};
use parking_lot::Mutex;

use super::util;

const TOOL_TIMEOUT: Duration = Duration::from_millis(500);

pub struct Clipboard {
    inner: Mutex<Option<arboard::Clipboard>>,
    wayland: bool,
}

impl Clipboard {
    pub fn new(wayland: bool) -> Self {
        Self { inner: Mutex::new(open()), wayland }
    }

    fn with<R>(&self, f: impl FnOnce(&mut arboard::Clipboard) -> Result<R, arboard::Error>) -> Option<R> {
        let mut guard = self.inner.lock();
        if guard.is_none() {
            *guard = open();
        }
        let clipboard = guard.as_mut()?;
        match f(clipboard) {
            Ok(value) => Some(value),
            Err(arboard::Error::ContentNotAvailable) => None,
            Err(e) => {
                log::debug!("Clipboard error: {e}");
                None
            }
        }
    }

    fn wl_tools(&self) -> bool {
        self.wayland && util::which("wl-copy").is_some()
    }

    /// The clipboard's text, if it holds text.
    pub fn text(&self) -> Option<String> {
        if let Some(text) = self.with(|c| c.get().text()) {
            return Some(text);
        }
        if self.wl_tools() {
            return util::run("wl-paste", &["--no-newline", "--type", "text"], TOOL_TIMEOUT);
        }
        None
    }

    /// Puts text on the clipboard. `transient`: ask clipboard managers not to keep it in their
    /// history (it's only there for a moment, to paste).
    pub fn set_text(&self, text: &str, transient: bool) -> bool {
        let set = self.with(|c| {
            let set = c.set();
            let set = if transient { set.exclude_from_history() } else { set };
            set.text(text.to_string())
        });
        if set.is_some() {
            return true;
        }
        if self.wl_tools() {
            let mut args = vec!["--type", "text/plain;charset=utf-8"];
            if transient {
                args.push("--sensitive");
            }
            if util::run_with_input("wl-copy", &args, text, TOOL_TIMEOUT) {
                return true;
            }
            // Older wl-copy without --sensitive.
            return util::run_with_input("wl-copy", &["--type", "text/plain;charset=utf-8"], text, TOOL_TIMEOUT);
        }
        false
    }

    /// Empties the clipboard (so a following copy can be told apart from old contents).
    pub fn clear(&self) -> bool {
        if self.with(|c| c.clear()).is_some() {
            return true;
        }
        self.wl_tools() && util::run_with_input("wl-copy", &["--clear"], "", TOOL_TIMEOUT)
    }

    /// The PRIMARY selection (the text selected last, on X11).
    pub fn primary_text(&self) -> Option<String> {
        self.with(|c| c.get().clipboard(LinuxClipboardKind::Primary).text())
    }

    /// Whether a working clipboard was found.
    pub fn is_available(&self) -> bool {
        self.inner.lock().is_some() || self.wl_tools()
    }
}

fn open() -> Option<arboard::Clipboard> {
    static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    match arboard::Clipboard::new() {
        Ok(clipboard) => Some(clipboard),
        Err(e) => {
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                log::warn!("No clipboard access through arboard: {e}");
            }
            None
        }
    }
}
