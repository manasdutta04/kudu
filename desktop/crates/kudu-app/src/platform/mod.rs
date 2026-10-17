//! Everything that differs between Windows and Linux, behind one trait.
//!
//! Kudu runs as a light background process (this trait, the speech engine, the overlay pill
//! and the tray icon). The settings window is a separate process started on demand, so the
//! background process never carries a GUI toolkit.

use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Sender;
use kudu_core::shortcut::{HotkeyEvent, Matcher, Notation, Recorded, Shortcuts};
use parking_lot::Mutex;

use crate::overlay::OverlayState;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(windows)]
pub mod windows;

/// Messages from the platform's threads (keyboard hook, tray, overlay) to the app.
#[derive(Clone, Debug)]
pub enum PlatformEvent {
    Hotkey(HotkeyEvent),
    /// Result of `record_shortcut`.
    Recorded(Recorded),
    /// While recording: the keys held so far, as keycaps.
    RecordingPreview(Vec<String>),
    /// The user clicked Undo on the "saved to dictionary" notice.
    UndoLearned,
    Tray(TrayAction),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayAction {
    OpenWindow,
    PasteLast,
    /// Start or stop meeting notes.
    Meeting,
    Quit,
}

/// What the tray icon shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayState {
    Idle,
    Recording,
    Working,
    /// Setup isn't finished (model missing, permissions).
    NeedsSetup,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FocusedApp {
    /// Human-readable name: "Slack", "Visual Studio Code", "Firefox".
    pub name: String,
    /// Stable identifier: the executable name on Windows ("slack.exe"), the WM_CLASS or app id
    /// on Linux ("Slack", "org.mozilla.firefox").
    pub id: String,
    /// The focused window's title.
    pub title: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertMethod {
    /// Typed as Unicode key events (short text, no clipboard involved).
    Typed,
    /// Put on the clipboard and pasted, then the clipboard was restored.
    Pasted,
}

/// A problem the user has to fix before Kudu can work, shown in the window's setup card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupIssue {
    pub title: String,
    pub detail: String,
    /// A command to copy, e.g. `sudo usermod -aG input $USER`.
    pub command: Option<String>,
}

/// Platform services. Implementations are cheap to call from any thread.
pub trait Platform: Send + Sync + 'static {
    /// How shortcuts are written on this platform.
    fn notation(&self) -> Notation;

    /// Shortcuts for a first launch.
    fn default_shortcuts(&self) -> Shortcuts;

    /// Key code of Escape (cancels a dictation).
    fn escape_code(&self) -> u32;

    /// Whether a key types text (letters, digits, punctuation, Space), so holding it alone
    /// can't be a dictation shortcut. F-keys, Pause, Insert… don't.
    fn key_types_text(&self, code: u32) -> bool;

    /// Starts the global keyboard listener. Key events go through `matcher`; whatever it
    /// recognizes is sent as `PlatformEvent::Hotkey`, and the events it says to swallow are
    /// hidden from other apps where the platform allows it. The app updates the matcher's
    /// shortcuts and `is_dictating` through the shared lock.
    fn start_hotkeys(&self, matcher: Arc<Mutex<Matcher>>, events: Sender<PlatformEvent>) -> anyhow::Result<()>;

    /// The next key combination is recorded instead of matched (and hidden from other apps).
    /// Progress is sent as `RecordingPreview`, the result as `Recorded`.
    fn record_shortcut(&self);

    fn cancel_recording(&self);

    /// Inserts text at the cursor of the focused app.
    fn insert_text(&self, text: &str) -> anyhow::Result<InsertMethod>;

    /// The selected text in the focused app (via a copy, restoring the clipboard), if any.
    fn copy_selection(&self) -> Option<String>;

    /// The app that will receive the text.
    fn focused_app(&self) -> Option<FocusedApp>;

    /// The full text of the focused text field, where the platform's accessibility API exposes
    /// it. Used to learn from corrections and to fit spacing and capitals to the text before
    /// the cursor. `None` when unsupported.
    fn focused_field_text(&self) -> Option<String>;

    /// The text just before the cursor in the focused field (a few hundred characters), when
    /// available.
    fn text_before_cursor(&self) -> Option<String>;

    /// Visible text of the focused window (names and terms to spell right), within `budget`.
    fn screen_text(&self, budget: Duration) -> Vec<String>;

    /// The overlay pill.
    fn overlay(&self) -> &dyn Overlay;

    /// Starts the tray icon. Menu clicks are sent as `PlatformEvent::Tray`.
    fn start_tray(&self, events: Sender<PlatformEvent>) -> anyhow::Result<()>;

    fn set_tray_state(&self, state: TrayState, tooltip: &str);

    /// Whether Kudu starts when the user logs in.
    fn launch_at_login(&self) -> bool;

    fn set_launch_at_login(&self, enabled: bool) -> anyhow::Result<()>;

    /// Problems that stop Kudu from working (permissions, missing packages).
    fn setup_issues(&self) -> Vec<SetupIssue>;

    /// The meeting apps using the microphone right now (Kudu excluded), or `None` when this
    /// system can't tell.
    fn meeting_apps_using_microphone(&self) -> Option<Vec<kudu_core::meeting_apps::MeetingApp>>;
}

/// The floating pill near the bottom of the screen. It never takes focus.
pub trait Overlay: Send + Sync {
    /// Shows the pill in a state (or switches state if it's already showing).
    fn show(&self, state: OverlayState);

    /// Audio loudness history while recording, 0…1, newest first (5 values).
    fn set_levels(&self, levels: [f32; 5]);

    fn hide(&self);
}

/// Creates the platform for this OS.
pub fn current(events: Sender<PlatformEvent>) -> anyhow::Result<Arc<dyn Platform>> {
    #[cfg(windows)]
    {
        Ok(Arc::new(windows::WindowsPlatform::new(events)?))
    }
    #[cfg(target_os = "linux")]
    {
        Ok(Arc::new(linux::LinuxPlatform::new(events)?))
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = events;
        anyhow::bail!("This build of Kudu is for Windows and Linux; on a Mac, use the native app in macos/.")
    }
}

/// How long the overlay waits between frames while animating.
pub const OVERLAY_FRAME: Duration = Duration::from_millis(16);
