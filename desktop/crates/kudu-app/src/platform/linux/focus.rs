//! Which app has keyboard focus.
//!
//! X11 publishes the active window on the root window. Wayland deliberately doesn't tell apps
//! about other apps' windows, so each compositor needs its own route: Hyprland and Sway answer
//! over their IPC, KDE through `kdotool` when installed. XWayland apps are still visible
//! through X11, and AT-SPI (when apps expose it) covers the rest.

use std::time::Duration;

use serde_json::Value;

use super::util::{self, Desktop, Session};
use super::x11::X11;
use crate::platform::FocusedApp;

const IPC_TIMEOUT: Duration = Duration::from_millis(250);

/// The focused app, plus the process name for terminal detection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Focus {
    pub app: FocusedApp,
    pub process: Option<String>,
}

impl Focus {
    pub fn is_terminal(&self) -> bool {
        is_terminal(&self.app.id) || self.process.as_deref().is_some_and(is_terminal)
    }
}

pub fn focused(session: &Session, x11: &X11) -> Option<Focus> {
    if session.wayland {
        let compositor = match session.desktop {
            Desktop::Hyprland => hyprland(),
            Desktop::Sway => sway(),
            Desktop::Kde => kde(),
            _ => None,
        };
        if compositor.is_some() {
            return compositor;
        }
    }
    // The X11 session, or an XWayland app (compositors set `_NET_ACTIVE_WINDOW` to None when
    // a Wayland window has focus).
    x11.active_window().map(|w| {
        let process = w.pid.and_then(util::process_name);
        let id = if w.class.is_empty() { process.clone().unwrap_or_default() } else { w.class.clone() };
        Focus { app: FocusedApp { name: nice_name(&id), id, title: w.title }, process }
    })
}

fn hyprland() -> Option<Focus> {
    let json: Value = serde_json::from_str(&util::run("hyprctl", &["activewindow", "-j"], IPC_TIMEOUT)?).ok()?;
    let class = json.get("class").and_then(Value::as_str).filter(|c| !c.is_empty())?;
    let title = json.get("title").and_then(Value::as_str).unwrap_or_default();
    let pid = json.get("pid").and_then(Value::as_u64).map(|p| p as u32);
    Some(Focus {
        app: FocusedApp { name: nice_name(class), id: class.to_string(), title: title.to_string() },
        process: pid.and_then(util::process_name),
    })
}

fn sway() -> Option<Focus> {
    let json: Value = serde_json::from_str(&util::run("swaymsg", &["-t", "get_tree", "-r"], IPC_TIMEOUT)?).ok()?;
    sway_focused(&json)
}

/// The focused window in a Sway tree: `app_id` for Wayland apps, the X11 class otherwise.
fn sway_focused(tree: &Value) -> Option<Focus> {
    let mut stack = vec![tree];
    while let Some(node) = stack.pop() {
        let is_window = node.get("pid").is_some() && node.get("type").and_then(Value::as_str) != Some("workspace");
        if is_window && node.get("focused").and_then(Value::as_bool) == Some(true) {
            let id = node
                .get("app_id")
                .and_then(Value::as_str)
                .or_else(|| node.pointer("/window_properties/class").and_then(Value::as_str))
                .unwrap_or_default();
            let title = node.get("name").and_then(Value::as_str).unwrap_or_default();
            let pid = node.get("pid").and_then(Value::as_u64).map(|p| p as u32);
            let process = pid.and_then(util::process_name);
            let id = if id.is_empty() { process.clone().unwrap_or_default() } else { id.to_string() };
            return Some(Focus { app: FocusedApp { name: nice_name(&id), id, title: title.to_string() }, process });
        }
        for key in ["nodes", "floating_nodes"] {
            if let Some(children) = node.get(key).and_then(Value::as_array) {
                stack.extend(children.iter());
            }
        }
    }
    None
}

fn kde() -> Option<Focus> {
    util::which("kdotool")?;
    // One call: kdotool runs a KWin script per invocation, which is the slow part.
    let output = util::run(
        "kdotool",
        &["getactivewindow", "getwindowclassname", "getwindowname", "getwindowpid"],
        Duration::from_millis(400),
    )?;
    let mut lines = output.lines();
    let class = lines.next()?.trim().to_string();
    if class.is_empty() {
        return None;
    }
    let title = lines.next().unwrap_or_default().trim().to_string();
    let pid = lines.next().and_then(|p| p.trim().parse::<u32>().ok());
    Some(Focus { app: FocusedApp { name: nice_name(&class), id: class, title }, process: pid.and_then(util::process_name) })
}

/// Terminal emulators, by app id, WM_CLASS or process name (lowercase; reverse-DNS ids are
/// matched on their last part too). Terminals paste with Ctrl + Shift + V and copy with
/// Ctrl + Shift + C, because Ctrl + V / Ctrl + C mean something to the shell.
const TERMINALS: &[&str] = &[
    "gnome-terminal",
    "gnome-terminal-server",
    "org.gnome.terminal",
    "ptyxis",
    "org.gnome.ptyxis",
    "kgx",
    "org.gnome.console",
    "konsole",
    "org.kde.konsole",
    "yakuake",
    "kitty",
    "alacritty",
    "foot",
    "footclient",
    "wezterm",
    "wezterm-gui",
    "org.wezfurlong.wezterm",
    "xterm",
    "uxterm",
    "urxvt",
    "rxvt",
    "tilix",
    "com.gexperts.tilix",
    "terminator",
    "xfce4-terminal",
    "terminology",
    "blackbox",
    "com.raggesilver.blackbox",
    "ghostty",
    "com.mitchellh.ghostty",
    "mate-terminal",
    "lxterminal",
    "qterminal",
    "st",
    "st-256color",
    "sakura",
    "guake",
    "tilda",
    "cool-retro-term",
    "deepin-terminal",
    "io.elementary.terminal",
    "rio",
    "contour",
    "warp",
    "dev.warp.warp",
];

pub fn is_terminal(id: &str) -> bool {
    let id = id.trim().to_ascii_lowercase();
    if id.is_empty() {
        return false;
    }
    let last = id.rsplit('.').next().unwrap_or(&id);
    TERMINALS.contains(&id.as_str()) || TERMINALS.contains(&last) || id.ends_with("-terminal")
}

/// Display names for app ids that don't read well on their own.
const NAMES: &[(&str, &str)] = &[
    ("code", "Visual Studio Code"),
    ("code-oss", "Code - OSS"),
    ("code-url-handler", "Visual Studio Code"),
    ("vscodium", "VSCodium"),
    ("google-chrome", "Google Chrome"),
    ("chromium-browser", "Chromium"),
    ("brave-browser", "Brave"),
    ("microsoft-edge", "Microsoft Edge"),
    ("gnome-terminal-server", "Terminal"),
    ("gnome-terminal", "Terminal"),
    ("kgx", "Console"),
    ("org.gnome.console", "Console"),
    ("org.gnome.ptyxis", "Ptyxis"),
    ("org.mozilla.firefox", "Firefox"),
    ("navigator", "Firefox"),
    ("libreoffice-writer", "LibreOffice Writer"),
    ("libreoffice-calc", "LibreOffice Calc"),
    ("libreoffice-impress", "LibreOffice Impress"),
    ("soffice", "LibreOffice"),
    ("jetbrains-idea", "IntelliJ IDEA"),
    ("jetbrains-pycharm", "PyCharm"),
    ("org.telegram.desktop", "Telegram"),
    ("telegramdesktop", "Telegram"),
    ("xfce4-terminal", "Xfce Terminal"),
    ("wezterm-gui", "WezTerm"),
    ("org.wezfurlong.wezterm", "WezTerm"),
];

/// A readable app name from a WM_CLASS or app id: "org.gnome.TextEditor" → "Text Editor",
/// "google-chrome" → "Google Chrome", "firefox" → "Firefox".
pub fn nice_name(id: &str) -> String {
    let lower = id.trim().to_ascii_lowercase();
    if let Some((_, name)) = NAMES.iter().find(|(key, _)| *key == lower) {
        return name.to_string();
    }
    let base = if id.contains('.') { id.rsplit('.').next().unwrap_or(id) } else { id };
    let mut name = String::new();
    let mut previous: Option<char> = None;
    for c in base.chars() {
        if c == '-' || c == '_' || c == ' ' {
            if !name.ends_with(' ') && !name.is_empty() {
                name.push(' ');
            }
            previous = Some(' ');
            continue;
        }
        // Split CamelCase ("TextEditor" → "Text Editor").
        if c.is_uppercase() && previous.is_some_and(|p| p.is_lowercase()) {
            name.push(' ');
        }
        if previous.is_none() || previous == Some(' ') {
            name.extend(c.to_uppercase());
        } else {
            name.push(c);
        }
        previous = Some(c);
    }
    let name = name.trim().to_string();
    if name.is_empty() {
        id.to_string()
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_terminals() {
        for id in [
            "gnome-terminal-server",
            "Gnome-terminal",
            "org.gnome.Ptyxis",
            "org.kde.konsole",
            "kitty",
            "Alacritty",
            "foot",
            "org.wezfurlong.wezterm",
            "XTerm",
            "URxvt",
            "com.gexperts.Tilix",
            "Terminator",
            "Xfce4-terminal",
            "com.raggesilver.BlackBox",
            "com.mitchellh.ghostty",
            "kgx",
            "mate-terminal",
        ] {
            assert!(is_terminal(id), "{id} is a terminal");
        }
        for id in ["firefox", "code", "org.gnome.TextEditor", "Slack", "", "terminal-notes.app"] {
            assert!(!is_terminal(id), "{id} isn't a terminal");
        }
        let focus = Focus { app: FocusedApp { id: "Wrapper".into(), ..Default::default() }, process: Some("kitty".into()) };
        assert!(focus.is_terminal());
    }

    #[test]
    fn makes_nice_names() {
        assert_eq!(nice_name("firefox"), "Firefox");
        assert_eq!(nice_name("org.mozilla.firefox"), "Firefox");
        assert_eq!(nice_name("org.gnome.TextEditor"), "Text Editor");
        assert_eq!(nice_name("google-chrome"), "Google Chrome");
        assert_eq!(nice_name("Code"), "Visual Studio Code");
        assert_eq!(nice_name("Slack"), "Slack");
        assert_eq!(nice_name("obsidian"), "Obsidian");
        assert_eq!(nice_name("org.kde.dolphin"), "Dolphin");
        assert_eq!(nice_name("gnome-terminal-server"), "Terminal");
    }

    #[test]
    fn finds_the_focused_sway_window() {
        let tree: Value = serde_json::json!({
            "type": "root", "focused": false, "nodes": [{
                "type": "output", "focused": false, "nodes": [{
                    "type": "workspace", "focused": false, "nodes": [
                        {"type": "con", "focused": false, "pid": 10, "app_id": "foot", "name": "~"},
                        {"type": "con", "focused": true, "pid": 11, "app_id": null, "name": "Inbox",
                         "window_properties": {"class": "thunderbird"}}
                    ], "floating_nodes": []
                }]
            }]
        });
        let focus = sway_focused(&tree).expect("focused window");
        assert_eq!(focus.app.id, "thunderbird");
        assert_eq!(focus.app.name, "Thunderbird");
        assert_eq!(focus.app.title, "Inbox");
    }
}
