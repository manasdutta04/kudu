//! Which meeting apps are using the microphone, from Windows' own record of microphone use
//! (the same list behind the microphone icon in the taskbar and Settings → Privacy).

use kudu_core::meeting_apps::{app_for_program, MeetingApp};
use windows_registry::{Key, CURRENT_USER};

const CONSENT: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

/// `None` when Windows doesn't keep the record (older builds, or a policy turned it off).
pub fn apps_using_microphone() -> Option<Vec<MeetingApp>> {
    let root = CURRENT_USER.open(CONSENT).ok()?;
    let mut apps = Vec::new();
    // Store apps (new Teams, WhatsApp) by package name, desktop apps by path ("C:#…#Zoom.exe").
    for name in root.keys().ok()? {
        if name == "NonPackaged" {
            continue;
        }
        if root.open(&name).is_ok_and(|key| in_use(&key)) {
            apps.extend(app_for_program(&name));
        }
    }
    if let Ok(desktop) = root.open("NonPackaged") {
        for name in desktop.keys().into_iter().flatten() {
            if desktop.open(&name).is_ok_and(|key| in_use(&key)) {
                apps.extend(app_for_program(&name));
            }
        }
    }
    Some(apps)
}

/// An app is using the microphone now when it started and hasn't stopped.
fn in_use(key: &Key) -> bool {
    key.get_u64("LastUsedTimeStop").is_ok_and(|stop| stop == 0) && key.get_u64("LastUsedTimeStart").is_ok_and(|start| start > 0)
}
