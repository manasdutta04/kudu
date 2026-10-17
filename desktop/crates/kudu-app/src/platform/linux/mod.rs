//! Kudu on Linux, X11 and Wayland (GNOME, KDE, Hyprland, Sway and other wlroots compositors).
//!
//! - Shortcuts: evdev (`/dev/input`), the one global keyboard view that works everywhere.
//! - Text insertion: clipboard + Ctrl+V from a uinput virtual keyboard; `wtype` / `xdotool`
//!   type the text when uinput isn't allowed.
//! - Focused app: X11 properties, or the compositor's IPC (Hyprland, Sway, KDE via kdotool).
//! - Text around the cursor: AT-SPI, where apps expose it.
//! - Overlay: a layer-shell surface on Wayland, an override-redirect window on X11/XWayland.
//! - Tray: StatusNotifierItem.

mod atspi;
mod clipboard;
mod focus;
mod hotkeys;
mod inject;
mod keys;
mod meeting;
mod overlay;
mod tray;
mod util;
mod x11;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use crossbeam_channel::Sender;
use kudu_core::shortcut::{Matcher, Notation, Shortcuts};
use parking_lot::Mutex;

use self::atspi::Atspi;
use self::clipboard::Clipboard;
use self::focus::Focus;
use self::hotkeys::Keyboards;
use self::inject::VirtualKeyboard;
use self::keys::{KEY_C, KEY_ESC, KEY_LEFTCTRL, KEY_LEFTSHIFT, KEY_V, VIRTUAL_KEY_MAX};
use self::overlay::LinuxOverlay;
use self::tray::Tray;
use self::util::Session;
use self::x11::X11;
use super::{FocusedApp, InsertMethod, Overlay, Platform, PlatformEvent, SetupIssue, TrayState};

/// How long to wait for the user to let go of modifier keys before pasting.
const MODIFIER_RELEASE_WAIT: Duration = Duration::from_millis(300);
/// How long the dictated text stays on the clipboard before the user's is put back. The target
/// app reads the clipboard asynchronously after Ctrl+V; restoring sooner pastes the old text.
const CLIPBOARD_RESTORE_DELAY: Duration = Duration::from_millis(400);
/// How long to wait for an app to answer Ctrl+C.
const COPY_WAIT: Duration = Duration::from_millis(400);
/// How long a focused-app lookup is reused (the app asks, then pastes right after; some
/// lookups run a program).
const FOCUS_CACHE: Duration = Duration::from_millis(500);

const UINPUT_RULE: &str = r#"KERNEL=="uinput", GROUP="input", MODE="0660", OPTIONS+="static_node=uinput""#;

pub struct LinuxPlatform {
    session: Session,
    keyboards: Keyboards,
    virtual_keyboard: VirtualKeyboard,
    clipboard: Arc<Clipboard>,
    restore: Arc<Mutex<Restore>>,
    x11: X11,
    atspi: Atspi,
    overlay: LinuxOverlay,
    tray: Tray,
    /// The compositor offers a clipboard protocol for background apps (Wayland only).
    has_data_control: bool,
    focus_cache: Mutex<Option<(Instant, Option<Focus>)>>,
}

/// The user's clipboard, waiting to be put back after a paste or copy.
#[derive(Default)]
struct Restore {
    /// Bumped by every clipboard use; a scheduled restore only runs if it's still current.
    generation: u64,
    /// `Some(saved)` while a restore is pending (`saved` is `None` if the clipboard held no text).
    pending: Option<Option<String>>,
}

impl LinuxPlatform {
    pub fn new(events: Sender<PlatformEvent>) -> anyhow::Result<Self> {
        let session = Session::detect();
        let globals = if session.wayland { wayland_globals() } else { Vec::new() };
        let has_data_control =
            globals.iter().any(|g| g == "zwlr_data_control_manager_v1" || g == "ext_data_control_manager_v1");
        log::info!(
            "Linux session: wayland={} x11={} desktop={:?} data-control={}",
            session.wayland,
            session.x11,
            session.desktop,
            has_data_control
        );
        Ok(Self {
            keyboards: Keyboards::new(events.clone()),
            virtual_keyboard: VirtualKeyboard::new(),
            clipboard: Arc::new(Clipboard::new(session.wayland)),
            restore: Arc::new(Mutex::new(Restore::default())),
            x11: X11::new(session.x11),
            atspi: Atspi::start(),
            overlay: LinuxOverlay::start(&session, events),
            tray: Tray::new(),
            has_data_control,
            focus_cache: Mutex::new(None),
            session,
        })
    }

    fn focus(&self) -> Option<Focus> {
        if let Some((at, focus)) = self.focus_cache.lock().as_ref() {
            if at.elapsed() < FOCUS_CACHE {
                return focus.clone();
            }
        }
        let focus = focus::focused(&self.session, &self.x11).or_else(|| self.atspi.focused_app());
        *self.focus_cache.lock() = Some((Instant::now(), focus.clone()));
        focus
    }

    /// Whether the focused app is a terminal emulator.
    fn in_terminal(&self) -> bool {
        self.focus().is_some_and(|f| f.is_terminal()) || self.atspi.focused_is_terminal()
    }

    /// The evdev key that types `keysym` in the user's layout (Ctrl+V must press the key that
    /// is V in a Dvorak layout, not the key labelled V). Only X11/XWayland can tell; otherwise
    /// the US position, which is also what toolkits match shortcuts on for non-Latin layouts.
    fn key_for(&self, keysym: u32, fallback: u16) -> u16 {
        self.x11
            .keycode_for(keysym)
            .and_then(|code| (code as u16).checked_sub(8))
            .filter(|&code| code > 0 && code <= VIRTUAL_KEY_MAX)
            .unwrap_or(fallback)
    }

    /// Terminals use Ctrl+Shift+V / Ctrl+Shift+C: Ctrl+V and Ctrl+C go to the shell.
    fn chord_modifiers(terminal: bool) -> &'static [u16] {
        if terminal {
            &[KEY_LEFTCTRL, KEY_LEFTSHIFT]
        } else {
            &[KEY_LEFTCTRL]
        }
    }

    /// Starts a clipboard use: returns its generation and the user's clipboard text to put back
    /// (from a restore still pending, since the clipboard then holds our own text).
    fn begin_clipboard_use(&self) -> (u64, Option<String>) {
        let mut restore = self.restore.lock();
        restore.generation += 1;
        let saved = match restore.pending.take() {
            Some(saved) => saved,
            None => self.clipboard.text(),
        };
        (restore.generation, saved)
    }

    /// Puts the user's clipboard back after `CLIPBOARD_RESTORE_DELAY`, if it still holds `ours`
    /// and nothing else used the clipboard meanwhile.
    fn schedule_restore(&self, generation: u64, saved: Option<String>, ours: String) {
        self.restore.lock().pending = Some(saved);
        let restore = self.restore.clone();
        let clipboard = self.clipboard.clone();
        let spawned = std::thread::Builder::new().name("kudu-clipboard".into()).spawn(move || {
            std::thread::sleep(CLIPBOARD_RESTORE_DELAY);
            let mut restore = restore.lock();
            if restore.generation != generation {
                return;
            }
            let Some(saved) = restore.pending.take() else { return };
            // Held across the restore so a new paste can't save our text as the user's.
            if let Some(saved) = saved {
                if clipboard.text().as_deref() == Some(ours.as_str()) {
                    clipboard.set_text(&saved, false);
                }
            }
        });
        if let Err(e) = spawned {
            log::warn!("Couldn't schedule the clipboard restore: {e}");
        }
    }

    fn paste(&self, text: &str) -> anyhow::Result<()> {
        let terminal = self.in_terminal();
        let (generation, saved) = self.begin_clipboard_use();
        if !self.clipboard.set_text(text, true) {
            // Nothing changed; keep the user's text for the next clipboard use to put back.
            self.restore.lock().pending = Some(saved);
            bail!("couldn't put the text on the clipboard");
        }
        self.keyboards.wait_for_modifiers_released(MODIFIER_RELEASE_WAIT);
        let v = self.key_for('v' as u32, KEY_V);
        let result = self.virtual_keyboard.chord(Self::chord_modifiers(terminal), v);
        self.schedule_restore(generation, saved, text.to_string());
        result.context("couldn't send Ctrl+V")
    }

    /// Types the text with an installed tool, for when uinput isn't available.
    fn type_with_tool(&self, text: &str) -> anyhow::Result<InsertMethod> {
        let timeout = Duration::from_secs(3) + Duration::from_millis(10) * text.chars().count() as u32;
        // wtype uses the virtual-keyboard protocol (wlroots compositors; not GNOME or KDE).
        if self.session.wayland && util::which("wtype").is_some() && util::run("wtype", &["--", text], timeout).is_some() {
            return Ok(InsertMethod::Typed);
        }
        // xdotool reaches X11 apps (all of them in an X11 session, XWayland apps otherwise).
        if self.session.x11
            && util::which("xdotool").is_some()
            && util::run("xdotool", &["type", "--clearmodifiers", "--delay", "4", "--", text], timeout).is_some()
        {
            return Ok(InsertMethod::Typed);
        }
        bail!("Kudu can't type here: allow it to use /dev/uinput (see Setup), or install wtype (Wayland) or xdotool (X11)")
    }
}

impl Platform for LinuxPlatform {
    fn notation(&self) -> Notation {
        Notation::Linux
    }

    fn default_shortcuts(&self) -> Shortcuts {
        Shortcuts::default_pc(KEY_V as u32)
    }

    fn escape_code(&self) -> u32 {
        KEY_ESC as u32
    }

    fn key_types_text(&self, code: u32) -> bool {
        u16::try_from(code).is_ok_and(keys::key_types_text)
    }

    fn start_hotkeys(&self, matcher: Arc<Mutex<Matcher>>, events: Sender<PlatformEvent>) -> anyhow::Result<()> {
        self.keyboards.start(matcher, events)
    }

    fn record_shortcut(&self) {
        self.keyboards.record();
    }

    fn cancel_recording(&self) {
        self.keyboards.cancel_recording();
    }

    fn insert_text(&self, text: &str) -> anyhow::Result<InsertMethod> {
        if text.is_empty() {
            return Ok(InsertMethod::Typed);
        }
        if self.virtual_keyboard.is_available() && self.clipboard.is_available() {
            match self.paste(text) {
                Ok(()) => return Ok(InsertMethod::Pasted),
                Err(e) => log::warn!("Paste failed ({e:#}); typing instead"),
            }
        }
        self.type_with_tool(text)
    }

    fn copy_selection(&self) -> Option<String> {
        // Accessibility first: no key events, no clipboard.
        if let Some(text) = self.atspi.selected_text() {
            return Some(text);
        }
        // On X11, selecting text already put it in PRIMARY; trust it only if the focused app
        // owns it (otherwise it's an old selection from another window).
        if self.session.is_x11_session() && self.x11.primary_owned_by_active_app() {
            if let Some(text) = self.clipboard.primary_text().filter(|t| !t.trim().is_empty()) {
                return Some(text);
            }
        }
        if !self.virtual_keyboard.is_available() {
            return None;
        }
        let terminal = self.in_terminal();
        let (generation, saved) = self.begin_clipboard_use();
        // With text on the clipboard, a copy of the same text would look like no copy at all:
        // empty it first, so any text that shows up is the selection.
        let cleared = saved.is_some() && self.clipboard.clear();
        self.keyboards.wait_for_modifiers_released(MODIFIER_RELEASE_WAIT);
        let c = self.key_for('c' as u32, KEY_C);
        let mut copied = None;
        if self.virtual_keyboard.chord(Self::chord_modifiers(terminal), c).is_ok() {
            let deadline = Instant::now() + COPY_WAIT;
            while Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(25));
                if let Some(text) = self.clipboard.text().filter(|t| !t.is_empty()) {
                    if cleared || saved.is_none() || Some(&text) != saved.as_ref() {
                        copied = Some(text);
                        break;
                    }
                }
            }
        }
        let mut restore = self.restore.lock();
        if restore.generation == generation {
            restore.pending = None;
            if let Some(saved) = &saved {
                self.clipboard.set_text(saved, false);
            }
        }
        copied
    }

    fn focused_app(&self) -> Option<FocusedApp> {
        self.focus().map(|f| f.app)
    }

    fn focused_field_text(&self) -> Option<String> {
        self.atspi.field_text()
    }

    fn text_before_cursor(&self) -> Option<String> {
        self.atspi.text_before_cursor(500)
    }

    fn screen_text(&self, budget: Duration) -> Vec<String> {
        self.atspi.screen_text(budget)
    }

    fn overlay(&self) -> &dyn Overlay {
        &self.overlay
    }

    fn start_tray(&self, events: Sender<PlatformEvent>) -> anyhow::Result<()> {
        self.tray.start(events)
    }

    fn set_tray_state(&self, state: TrayState, tooltip: &str) {
        self.tray.set_state(state, tooltip);
    }

    fn launch_at_login(&self) -> bool {
        autostart_file().and_then(|path| std::fs::read_to_string(path).ok()).is_some_and(|entry| autostart_enabled(&entry))
    }

    fn set_launch_at_login(&self, enabled: bool) -> anyhow::Result<()> {
        let path = autostart_file().context("no home folder")?;
        if !enabled {
            if path.exists() {
                std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
            }
            return Ok(());
        }
        // Inside an AppImage the executable is in a temporary mount; start the AppImage itself.
        let exe = std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .map(Ok)
            .unwrap_or_else(std::env::current_exe)
            .context("finding Kudu's executable")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        std::fs::write(&path, autostart_entry(&exe)).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    fn meeting_apps_using_microphone(&self) -> Option<Vec<kudu_core::meeting_apps::MeetingApp>> {
        meeting::apps_using_microphone()
    }

    fn setup_issues(&self) -> Vec<SetupIssue> {
        let mut issues = Vec::new();
        if !self.keyboards.is_listening() && !hotkeys::any_keyboard_readable() {
            issues.push(SetupIssue {
                title: "Add yourself to the input group".into(),
                detail: "Kudu reads the keyboard directly to notice your shortcut in every app, on X11 and Wayland. \
                         Run this command, then log out and back in."
                    .into(),
                command: Some("sudo usermod -aG input $USER".into()),
            });
        }
        if !self.virtual_keyboard.is_available() {
            let missing = !Path::new("/dev/uinput").exists()
                || self.virtual_keyboard.error() == Some(std::io::ErrorKind::NotFound);
            let mut command = format!(
                "echo '{UINPUT_RULE}' | sudo tee /etc/udev/rules.d/70-kudu-uinput.rules && sudo udevadm control --reload-rules && sudo udevadm trigger"
            );
            if missing {
                command = format!("sudo modprobe uinput && {command}");
            }
            issues.push(SetupIssue {
                title: "Let Kudu type into apps".into(),
                detail: "Kudu pastes your words with a virtual keyboard (/dev/uinput), which needs permission. \
                         Run this command (you also need to be in the input group), then restart Kudu."
                    .into(),
                command: Some(command),
            });
        }
        if self.session.wayland && !self.has_data_control && !self.session.x11 && util::which("wl-copy").is_none() {
            let os_release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
            issues.push(SetupIssue {
                title: "Install wl-clipboard".into(),
                detail: "Your desktop doesn't let background apps use the clipboard directly, so Kudu needs the wl-copy tool to paste."
                    .into(),
                command: Some(install_command(&os_release, "wl-clipboard")),
            });
        }
        issues
    }
}

/// The Wayland globals the compositor offers (to see which protocols it supports).
fn wayland_globals() -> Vec<String> {
    use wayland_client::globals::{registry_queue_init, GlobalListContents};
    use wayland_client::protocol::wl_registry::{self, WlRegistry};
    use wayland_client::{Connection, Dispatch, QueueHandle};

    struct Probe;
    impl Dispatch<WlRegistry, GlobalListContents> for Probe {
        fn event(_: &mut Self, _: &WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
    }

    let Ok(conn) = Connection::connect_to_env() else { return Vec::new() };
    let Ok((globals, _queue)) = registry_queue_init::<Probe>(&conn) else { return Vec::new() };
    globals.contents().with_list(|list| list.iter().map(|g| g.interface.clone()).collect())
}

/// `~/.config/autostart/kudu.desktop` (XDG autostart, honoured by every desktop).
fn autostart_file() -> Option<PathBuf> {
    let dirs = directories::BaseDirs::new()?;
    Some(dirs.config_dir().join("autostart").join("kudu.desktop"))
}

fn autostart_enabled(entry: &str) -> bool {
    !entry.lines().map(str::trim).any(|line| line == "Hidden=true" || line == "X-GNOME-Autostart-enabled=false")
}

fn autostart_entry(exe: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Kudu\n\
         Comment=Hold a shortcut, speak, release: your words are typed\n\
         Exec={} --background\n\
         Icon=kudu\n\
         Terminal=false\n\
         NoDisplay=true\n\
         X-GNOME-Autostart-enabled=true\n",
        exec_quote(&exe.to_string_lossy())
    )
}

/// Quotes a path for a desktop entry's `Exec` key: double quotes, `"` `` ` `` `$` `\` escaped
/// with a backslash, `%` doubled, then backslashes escaped again because the value itself is a
/// desktop-entry string.
fn exec_quote(path: &str) -> String {
    let mut quoted = String::from("\"");
    for c in path.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                quoted.push('\\');
                quoted.push(c);
            }
            '%' => quoted.push_str("%%"),
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted.replace('\\', "\\\\")
}

/// The command that installs a package on this distribution.
fn install_command(os_release: &str, package: &str) -> String {
    let field = |key: &str| {
        os_release
            .lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
            .map(|v| v.trim_matches('"').to_ascii_lowercase())
            .unwrap_or_default()
    };
    let ids = format!("{} {}", field("ID"), field("ID_LIKE"));
    let has = |name: &str| ids.split_whitespace().any(|id| id == name);
    if has("debian") || has("ubuntu") {
        format!("sudo apt install {package}")
    } else if has("fedora") || has("rhel") || has("centos") {
        format!("sudo dnf install {package}")
    } else if has("arch") {
        format!("sudo pacman -S {package}")
    } else if has("opensuse") || has("suse") || ids.contains("opensuse") {
        format!("sudo zypper install {package}")
    } else if has("nixos") {
        format!("nix-env -iA nixos.{package}")
    } else {
        format!("Install the {package} package with your package manager")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_an_autostart_entry() {
        let entry = autostart_entry(Path::new("/opt/Kudu App/kudu"));
        assert!(entry.contains("Exec=\"/opt/Kudu App/kudu\" --background\n"));
        assert!(autostart_enabled(&entry));
        assert!(!autostart_enabled("[Desktop Entry]\nHidden=true\n"));
        assert!(!autostart_enabled("[Desktop Entry]\nX-GNOME-Autostart-enabled=false\n"));
    }

    #[test]
    fn quotes_exec_paths() {
        assert_eq!(exec_quote("/usr/bin/kudu"), "\"/usr/bin/kudu\"");
        assert_eq!(exec_quote("/a$b"), "\"/a\\\\$b\"");
        assert_eq!(exec_quote("/a\\b"), "\"/a\\\\\\\\b\"");
        assert_eq!(exec_quote("/50%"), "\"/50%%\"");
    }

    #[test]
    fn picks_the_package_manager() {
        assert_eq!(install_command("ID=ubuntu\nID_LIKE=debian\n", "wl-clipboard"), "sudo apt install wl-clipboard");
        assert_eq!(install_command("ID=fedora\n", "wl-clipboard"), "sudo dnf install wl-clipboard");
        assert_eq!(install_command("ID=endeavouros\nID_LIKE=arch\n", "wl-clipboard"), "sudo pacman -S wl-clipboard");
        assert_eq!(install_command("ID=\"opensuse-tumbleweed\"\nID_LIKE=\"opensuse suse\"\n", "x"), "sudo zypper install x");
        assert!(install_command("", "x").contains("package manager"));
    }

    #[test]
    fn chooses_terminal_shortcuts() {
        assert_eq!(LinuxPlatform::chord_modifiers(true), &[KEY_LEFTCTRL, KEY_LEFTSHIFT]);
        assert_eq!(LinuxPlatform::chord_modifiers(false), &[KEY_LEFTCTRL]);
    }
}
