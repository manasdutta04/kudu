//! Which kind of app receives the text, from what the platform layer can see of the focused
//! window: the Windows executable name (`slack.exe`) or the Linux app id / `WM_CLASS`
//! (`org.telegram.desktop`, `jetbrains-idea`), the app's display name and the window title.
//!
//! Mirrors the macOS app's bundle-identifier mapping. Browsers are classified by the site in
//! their window title ("Inbox – Gmail" is email, "WhatsApp" is messages, "GitHub" stays general).

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Broad class of the app receiving the text; drives formatting and style decisions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppCategory {
    #[default]
    General,
    /// Editors, IDEs, terminals: no prose formatting, no trailing period.
    Code,
    /// WhatsApp, Telegram, Signal.
    PersonalMessage,
    /// Slack, Teams, Discord.
    WorkMessage,
    /// Outlook, Thunderbird, webmail.
    Email,
}

impl AppCategory {
    pub const ALL: [AppCategory; 5] =
        [Self::General, Self::Code, Self::PersonalMessage, Self::WorkMessage, Self::Email];

    pub fn is_message(self) -> bool {
        matches!(self, Self::PersonalMessage | Self::WorkMessage)
    }

    /// The stored name, as in the macOS app (`"workMessage"`); used as keys in usage totals.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Code => "code",
            Self::PersonalMessage => "personalMessage",
            Self::WorkMessage => "workMessage",
            Self::Email => "email",
        }
    }
}

impl fmt::Display for AppCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AppCategory {
    type Err = ();

    fn from_str(text: &str) -> Result<Self, ()> {
        Self::ALL.into_iter().find(|category| category.as_str() == text).ok_or(())
    }
}

/// Code editors and IDEs (terminals are listed separately and count as code too).
#[rustfmt::skip]
const EDITORS: &[&str] = &[
    // Windows executables (without ".exe")
    "code", "code - insiders", "code-insiders", "cursor", "windsurf", "zed", "sublime_text", "devenv", "idea", "idea64",
    "pycharm", "pycharm64", "webstorm", "webstorm64", "clion", "clion64", "goland", "goland64", "rider", "rider64",
    "rustrover", "rustrover64", "phpstorm", "phpstorm64", "rubymine", "rubymine64", "datagrip", "datagrip64", "studio",
    "studio64", "fleet", "lapce", "neovide",
    // Linux app ids and WM_CLASS
    "code-oss", "codium", "vscodium", "com.visualstudio.code", "com.visualstudio.code.oss", "com.vscodium.codium",
    "dev.zed.zed", "zed-editor", "sublime-text", "subl", "com.sublimetext.three", "dev.lapce.lapce",
    "org.gnome.builder", "gnome-builder", "android-studio",
];

/// Terminal emulators. Text is pasted into them with Ctrl+Shift+V on Linux.
#[rustfmt::skip]
const TERMINALS: &[&str] = &[
    // Windows
    "windowsterminal", "wt", "cmd", "powershell", "pwsh", "powershell_ise", "conhost", "openconsole", "wezterm-gui",
    "wezterm", "alacritty", "mintty", "hyper", "tabby", "warp", "ghostty", "kitty",
    // Linux
    "gnome-terminal", "gnome-terminal-server", "org.gnome.terminal", "org.gnome.ptyxis", "ptyxis", "org.gnome.console",
    "kgx", "konsole", "org.kde.konsole", "yakuake", "org.kde.yakuake", "foot", "footclient", "org.wezfurlong.wezterm",
    "xterm", "uxterm", "tilix", "com.gexperts.tilix", "terminator", "xfce4-terminal", "lxterminal", "qterminal",
    "mate-terminal", "urxvt", "rxvt", "rxvt-unicode", "st", "st-256color", "com.mitchellh.ghostty", "dev.warp.warp",
    "terminology", "guake", "deepin-terminal", "com.raggesilver.blackbox", "blackbox", "cool-retro-term", "sakura",
    "contour", "io.elementary.terminal", "org.codeberg.dnkl.foot",
];

#[rustfmt::skip]
const WORK_MESSAGING: &[&str] = &[
    "slack", "com.slack.slack", "discord", "discordcanary", "discordptb", "com.discordapp.discord", "vesktop",
    "dev.vencord.vesktop", "ms-teams", "teams", "msteams", "teams-for-linux",
    "com.github.ismaelmartinez.teams_for_linux", "zoom", "us.zoom.zoom", "mattermost", "com.mattermost.desktop",
    "rocket.chat", "chat.rocket.rocketchat",
];

#[rustfmt::skip]
const PERSONAL_MESSAGING: &[&str] = &[
    "whatsapp", "whatsapp.root", "whatsapp-for-linux", "com.github.eneshecan.whatsappforlinux", "zapzap",
    "com.rtosta.zapzap", "telegram", "telegramdesktop", "telegram-desktop", "org.telegram.desktop", "signal",
    "signal-desktop", "org.signal.signal", "wechat", "weixin", "messenger", "caprine", "com.sindresorhus.caprine",
    "viber",
];

#[rustfmt::skip]
const EMAIL: &[&str] = &[
    "outlook", "olk", "hxoutlook", "mailclient", "thunderbird", "org.mozilla.thunderbird",
    "net.thunderbird.thunderbird", "betterbird", "eu.betterbird.betterbird", "mailspring",
    "com.getmailspring.mailspring", "evolution", "org.gnome.evolution", "geary", "org.gnome.geary", "kmail", "kmail2",
    "org.kde.kmail", "org.kde.kmail2", "claws-mail", "sylpheed",
];

/// Display names that differ from the executable or app id ("Microsoft Teams", Windows Store
/// apps hosted by `ApplicationFrameHost.exe`).
const NAMES: &[(&str, AppCategory)] = &[
    ("visual studio code", AppCategory::Code),
    ("windows terminal", AppCategory::Code),
    ("windows powershell", AppCategory::Code),
    ("command prompt", AppCategory::Code),
    ("terminal", AppCategory::Code),
    ("microsoft teams", AppCategory::WorkMessage),
    ("microsoft outlook", AppCategory::Email),
    ("mail", AppCategory::Email),
];

#[rustfmt::skip]
const BROWSERS: &[&str] = &[
    "chrome", "google-chrome", "google-chrome-stable", "google-chrome-beta", "google-chrome-unstable", "chromium",
    "chromium-browser", "org.chromium.chromium", "msedge", "microsoft-edge", "microsoft-edge-stable",
    "com.microsoft.edge", "firefox", "firefox-esr", "org.mozilla.firefox", "navigator", "brave", "brave-browser",
    "com.brave.browser", "opera", "vivaldi", "vivaldi-stable", "com.vivaldi.vivaldi", "arc", "zen", "zen-browser",
    "app.zen_browser.zen", "librewolf", "io.gitlab.librewolf-community", "waterfox", "floorp", "one.ablaze.floorp",
    "epiphany", "org.gnome.epiphany", "falkon", "org.kde.falkon", "thorium", "iexplore",
];

/// Sites recognized in a browser's window title, matched against whole title segments
/// ("Inbox (3) - me@gmail.com - Gmail" has the segment "gmail").
const SITES: &[(&str, AppCategory)] = &[
    ("gmail", AppCategory::Email),
    ("outlook", AppCategory::Email),
    ("outlook.com", AppCategory::Email),
    ("proton mail", AppCategory::Email),
    ("yahoo mail", AppCategory::Email),
    ("fastmail", AppCategory::Email),
    ("zoho mail", AppCategory::Email),
    ("icloud mail", AppCategory::Email),
    ("slack", AppCategory::WorkMessage),
    ("discord", AppCategory::WorkMessage),
    ("microsoft teams", AppCategory::WorkMessage),
    ("teams", AppCategory::WorkMessage),
    ("google chat", AppCategory::WorkMessage),
    ("mattermost", AppCategory::WorkMessage),
    ("whatsapp", AppCategory::PersonalMessage),
    ("whatsapp web", AppCategory::PersonalMessage),
    ("telegram", AppCategory::PersonalMessage),
    ("telegram web", AppCategory::PersonalMessage),
    ("messenger", AppCategory::PersonalMessage),
    ("google messages", AppCategory::PersonalMessage),
    ("google messages for web", AppCategory::PersonalMessage),
    ("messages for web", AppCategory::PersonalMessage),
];

/// Classifies the focused app. `app_id` is the Windows executable name or path (`Slack.exe`,
/// `C:\…\Code.exe`) or the Linux app id / `WM_CLASS`; `app_name` its display name; and
/// `window_title` the focused window's title, used for browsers. Case-insensitive; any of
/// them may be empty.
pub fn category_for(app_id: &str, app_name: &str, window_title: &str) -> AppCategory {
    let id = normalize(app_id);
    let name = normalize(app_name);
    if let Some(category) = category_of_app(&id).or_else(|| category_of_app(&name)) {
        return category;
    }
    if let Some(&(_, category)) = NAMES.iter().find(|(known, _)| *known == name) {
        return category;
    }
    let is_browser = |text: &str| BROWSERS.contains(&text) || is_web_app(text);
    if is_browser(&id) || is_browser(&name) || (id.is_empty() && name.is_empty()) {
        return category_from_title(window_title).unwrap_or_default();
    }
    AppCategory::General
}

/// Whether the app is a terminal emulator (Linux terminals paste with Ctrl+Shift+V).
pub fn is_terminal(app_id: &str) -> bool {
    let id = normalize(app_id);
    TERMINALS.contains(&id.as_str()) || id.strip_suffix(".desktop").is_some_and(|id| TERMINALS.contains(&id))
}

fn category_of_app(id: &str) -> Option<AppCategory> {
    if id.is_empty() {
        return None;
    }
    // A desktop-entry name ("org.gnome.Ptyxis.desktop"), unless the id itself ends that way
    // ("org.telegram.desktop").
    if let Some(entry) = id.strip_suffix(".desktop") {
        if let Some(category) = category_of_known_app(entry) {
            return Some(category);
        }
    }
    category_of_known_app(id)
}

fn category_of_known_app(id: &str) -> Option<AppCategory> {
    if EDITORS.contains(&id)
        || TERMINALS.contains(&id)
        || id.starts_with("jetbrains-")
        || id.starts_with("com.jetbrains.")
    {
        Some(AppCategory::Code)
    } else if WORK_MESSAGING.contains(&id) {
        Some(AppCategory::WorkMessage)
    } else if PERSONAL_MESSAGING.contains(&id) {
        Some(AppCategory::PersonalMessage)
    } else if EMAIL.contains(&id) {
        Some(AppCategory::Email)
    } else {
        None
    }
}

/// Installed web apps run by a browser ("chrome-<id>-Default", "crx_<id>", "msedge-_<id>").
fn is_web_app(id: &str) -> bool {
    ["chrome-", "crx_", "msedge-", "brave-", "chromium-"].iter().any(|prefix| id.starts_with(prefix))
}

/// Lowercased file name without directory and ".exe".
fn normalize(id: &str) -> String {
    let base = id.trim().rsplit(['/', '\\']).next().unwrap_or_default();
    let lower = base.to_lowercase();
    lower.strip_suffix(".exe").unwrap_or(&lower).trim().to_owned()
}

fn category_from_title(title: &str) -> Option<AppCategory> {
    let lower = title.to_lowercase().replace(" - ", "|");
    lower.split(['|', '—', '–', '·', '•']).find_map(|segment| {
        let segment = clean_segment(segment);
        SITES.iter().find(|(site, _)| *site == segment).map(|&(_, category)| category)
    })
}

/// "(3) WhatsApp" → "whatsapp", "● Slack" → "slack", "Outlook (PWA)" → "outlook".
fn clean_segment(segment: &str) -> &str {
    let mut segment = segment.trim().trim_start_matches(['●', '*']).trim();
    if segment.starts_with('(') {
        if let Some(close) = segment.find(')') {
            segment = segment[close + 1..].trim();
        }
    }
    if let Some(open) = segment.rfind(" (") {
        if segment.ends_with(')') {
            segment = segment[..open].trim();
        }
    }
    segment
}

#[cfg(test)]
mod tests {
    use super::*;
    use AppCategory::*;

    #[test]
    fn windows_executables() {
        let cases = [
            ("slack.exe", WorkMessage),
            ("Discord.exe", WorkMessage),
            ("ms-teams.exe", WorkMessage),
            ("Teams.exe", WorkMessage),
            ("OUTLOOK.EXE", Email),
            ("olk.exe", Email),
            ("thunderbird.exe", Email),
            ("WhatsApp.exe", PersonalMessage),
            ("Telegram.exe", PersonalMessage),
            ("Signal.exe", PersonalMessage),
            ("Code.exe", Code),
            ("Cursor.exe", Code),
            ("idea64.exe", Code),
            ("WindowsTerminal.exe", Code),
            ("cmd.exe", Code),
            ("powershell.exe", Code),
            ("pwsh.exe", Code),
            ("wezterm-gui.exe", Code),
            (r"C:\Program Files\Microsoft VS Code\Code.exe", Code),
            ("notepad.exe", General),
            ("WINWORD.EXE", General),
        ];
        for (id, expected) in cases {
            assert_eq!(category_for(id, "", ""), expected, "{id}");
        }
    }

    #[test]
    fn linux_app_ids() {
        let cases = [
            ("Slack", WorkMessage),
            ("discord", WorkMessage),
            ("org.telegram.desktop", PersonalMessage),
            ("signal", PersonalMessage),
            ("thunderbird", Email),
            ("org.mozilla.Thunderbird", Email),
            ("code", Code),
            ("Code", Code),
            ("cursor", Code),
            ("jetbrains-idea", Code),
            ("jetbrains-pycharm-ce", Code),
            ("gnome-terminal-server", Code),
            ("org.gnome.Ptyxis", Code),
            ("konsole", Code),
            ("kitty", Code),
            ("Alacritty", Code),
            ("foot", Code),
            ("org.wezfurlong.wezterm", Code),
            ("xterm", Code),
            ("tilix", Code),
            ("org.gnome.Ptyxis.desktop", Code),
            ("org.gnome.Nautilus", General),
            ("libreoffice-writer", General),
        ];
        for (id, expected) in cases {
            assert_eq!(category_for(id, "", ""), expected, "{id}");
        }
    }

    #[test]
    fn display_names_help_when_the_executable_is_a_host() {
        assert_eq!(category_for("ApplicationFrameHost.exe", "WhatsApp", "WhatsApp"), PersonalMessage);
        assert_eq!(category_for("ApplicationFrameHost.exe", "Mail", "Inbox"), Email);
        assert_eq!(category_for("", "Microsoft Teams", ""), WorkMessage);
    }

    #[test]
    fn browsers_by_window_title() {
        let cases = [
            ("chrome.exe", "Inbox (3) - me@gmail.com - Gmail - Google Chrome", Email),
            ("msedge.exe", "Mail - Manas Dutta - Outlook - Microsoft\u{200b} Edge", Email),
            ("firefox", "Inbox – Fastmail — Mozilla Firefox", Email),
            ("google-chrome", "general (Channel) - Acme - Slack - Google Chrome", WorkMessage),
            ("firefox.exe", "#general | Acme - Discord — Mozilla Firefox", WorkMessage),
            ("brave-browser", "(3) WhatsApp - Brave", PersonalMessage),
            ("chrome.exe", "Chat | Microsoft Teams - Google Chrome", WorkMessage),
            ("Navigator", "Telegram Web — Mozilla Firefox", PersonalMessage),
            ("chrome.exe", "manasdutta04/kudu: Voice dictation - GitHub - Google Chrome", General),
            ("chrome.exe", "Market outlook for 2027 - Google Search - Google Chrome", General),
            ("crx_abcdef", "Gmail", Email),
        ];
        for (id, title, expected) in cases {
            assert_eq!(category_for(id, "", title), expected, "{title}");
        }
        // A title alone doesn't change a known non-browser app.
        assert_eq!(category_for("notepad.exe", "", "Slack notes.txt - Notepad"), General);
    }

    #[test]
    fn terminals() {
        for id in ["WindowsTerminal.exe", "pwsh.exe", "gnome-terminal-server", "org.gnome.Ptyxis", "kitty", "Alacritty"]
        {
            assert!(is_terminal(id), "{id}");
        }
        for id in ["Code.exe", "code", "slack", "firefox"] {
            assert!(!is_terminal(id), "{id}");
        }
    }

    #[test]
    fn stored_names_round_trip() {
        for category in AppCategory::ALL {
            assert_eq!(category.as_str().parse(), Ok(category));
            assert_eq!(serde_json::to_string(&category).unwrap(), format!("\"{}\"", category.as_str()));
        }
        assert!(WorkMessage.is_message() && PersonalMessage.is_message() && !Email.is_message());
    }
}
