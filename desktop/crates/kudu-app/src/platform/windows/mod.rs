//! Windows: a low-level keyboard hook for shortcuts, clipboard paste for text, UI Automation
//! for reading text, a layered window for the overlay and a notification-area icon.
//!
//! Threads:
//! - `kudu-keyboard`: the keyboard hook, alone so nothing can delay it;
//! - `kudu-ui`: the overlay window and the tray icon;
//! - `kudu-clipboard`: owns the clipboard while pasting;
//! - `kudu-automation`: UI Automation calls, which can block on unresponsive apps.

mod app;
mod autostart;
mod clipboard;
mod hook;
mod input;
mod keys;
mod meeting;
mod overlay;
mod tray;
mod uia;
mod worker;

use std::sync::Arc;
use std::time::Duration;

use ::windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use anyhow::anyhow;
use crossbeam_channel::Sender;
use kudu_core::shortcut::{Matcher, Notation, Shortcuts};
use parking_lot::Mutex;

use super::{FocusedApp, InsertMethod, Overlay, Platform, PlatformEvent, SetupIssue, TrayState};

/// How long to wait for the user to let go of the shortcut's keys before pasting or copying.
const RELEASE_WAIT: Duration = Duration::from_millis(300);

pub struct WindowsPlatform {
    ui: worker::Worker,
    hook: hook::Hook,
    clipboard: clipboard::Clipboard,
    uia: uia::Uia,
    overlay: overlay::OverlayHandle,
    apps: app::AppNames,
}

impl WindowsPlatform {
    pub fn new(events: Sender<PlatformEvent>) -> anyhow::Result<Self> {
        // Pixel-exact overlay placement on every monitor. Fails harmlessly when the manifest
        // already set it.
        // SAFETY: process-wide setting, made before any window exists.
        if let Err(error) = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } {
            log::debug!("DPI awareness not changed: {error}");
        }

        let (levels_tx, levels_rx) = crossbeam_channel::bounded(1);
        let overlay_events = events.clone();
        let ui = worker::Worker::spawn("kudu-ui", None, move |_| {
            let levels = overlay::OverlayHandle::create(overlay_events)?;
            let _ = levels_tx.send(levels);
            Ok(())
        })?;
        let levels = levels_rx.recv().map_err(|_| anyhow!("the overlay wasn't created"))?;

        Ok(Self {
            overlay: overlay::OverlayHandle::new(ui.clone(), levels),
            ui,
            hook: hook::Hook::spawn(events)?,
            clipboard: clipboard::Clipboard::spawn()?,
            uia: uia::Uia::spawn()?,
            apps: app::AppNames::default(),
        })
    }
}

impl Platform for WindowsPlatform {
    fn notation(&self) -> Notation {
        Notation::Windows
    }

    fn default_shortcuts(&self) -> Shortcuts {
        Shortcuts::default_pc(keys::VK_V)
    }

    fn escape_code(&self) -> u32 {
        keys::VK_ESCAPE
    }

    fn key_types_text(&self, code: u32) -> bool {
        keys::key_types_text(code)
    }

    fn start_hotkeys(&self, matcher: Arc<Mutex<Matcher>>, events: Sender<PlatformEvent>) -> anyhow::Result<()> {
        self.hook.start(matcher, events)
    }

    fn record_shortcut(&self) {
        self.hook.record();
    }

    fn cancel_recording(&self) {
        self.hook.cancel_recording();
    }

    fn insert_text(&self, text: &str) -> anyhow::Result<InsertMethod> {
        if text.is_empty() {
            return Ok(InsertMethod::Pasted);
        }
        input::wait_for_modifiers_released(RELEASE_WAIT);
        match self.clipboard.prepare_paste(text) {
            Ok(()) => {
                if input::send_ctrl_chord(keys::VK_V as u16) {
                    Ok(InsertMethod::Pasted)
                } else {
                    // Typically the focused app runs as administrator, which blocks synthesized
                    // input from Kudu; the text stays on the clipboard to paste by hand.
                    Err(anyhow!("Windows blocked the paste keystroke (is the app running as administrator?)"))
                }
            }
            Err(error) => {
                log::warn!("Clipboard unavailable ({error:#}); typing the text instead");
                if input::type_text(text) {
                    Ok(InsertMethod::Typed)
                } else {
                    Err(anyhow!("Windows blocked typing the text (is the app running as administrator?)"))
                }
            }
        }
    }

    fn copy_selection(&self) -> Option<String> {
        self.clipboard.copy_selection()
    }

    fn focused_app(&self) -> Option<FocusedApp> {
        self.apps.focused_app()
    }

    fn focused_field_text(&self) -> Option<String> {
        self.uia.focused_field_text()
    }

    fn text_before_cursor(&self) -> Option<String> {
        self.uia.text_before_cursor()
    }

    fn screen_text(&self, budget: Duration) -> Vec<String> {
        self.uia.screen_text(budget)
    }

    fn overlay(&self) -> &dyn Overlay {
        &self.overlay
    }

    fn start_tray(&self, events: Sender<PlatformEvent>) -> anyhow::Result<()> {
        self.ui
            .call(Duration::from_secs(5), move || tray::start(events))
            .unwrap_or_else(|| Err(anyhow!("the UI thread didn't answer")))
    }

    fn set_tray_state(&self, state: TrayState, tooltip: &str) {
        let tooltip = tooltip.to_string();
        self.ui.post(move || tray::set_state(state, &tooltip));
    }

    fn launch_at_login(&self) -> bool {
        autostart::enabled()
    }

    fn set_launch_at_login(&self, enabled: bool) -> anyhow::Result<()> {
        autostart::set(enabled)
    }

    fn meeting_apps_using_microphone(&self) -> Option<Vec<kudu_core::meeting_apps::MeetingApp>> {
        meeting::apps_using_microphone()
    }

    fn setup_issues(&self) -> Vec<SetupIssue> {
        let mut issues = Vec::new();
        if self.hook.failed() {
            issues.push(SetupIssue {
                title: "Keyboard shortcuts aren't working".into(),
                detail: "Windows didn't let Kudu listen for its shortcuts. Restart Kudu; if that doesn't help, \
                         check whether security software is blocking keyboard hooks."
                    .into(),
                command: None,
            });
        }
        issues
    }
}
