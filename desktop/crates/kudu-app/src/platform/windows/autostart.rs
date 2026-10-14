//! Starting Kudu when the user logs in: a value under the current user's `Run` key.

use anyhow::Context;
use windows_registry::CURRENT_USER;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Where Task Manager's Startup page records entries the user turned off (first byte odd).
const APPROVED_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const VALUE: &str = "Kudu";

fn command() -> anyhow::Result<String> {
    let exe = std::env::current_exe().context("finding Kudu's executable")?;
    Ok(format!("\"{}\" --background", exe.display()))
}

/// On when the entry exists, points at this executable, and wasn't turned off in Task Manager.
pub fn enabled() -> bool {
    let Ok(registered) = CURRENT_USER.open(RUN_KEY).and_then(|key| key.get_string(VALUE)) else {
        return false;
    };
    let Ok(expected) = command() else { return false };
    if !registered.eq_ignore_ascii_case(&expected) {
        return false;
    }
    let disabled = CURRENT_USER
        .open(APPROVED_KEY)
        .and_then(|key| key.get_value(VALUE))
        .is_ok_and(|value| value.first().is_some_and(|flags| flags & 1 == 1));
    !disabled
}

pub fn set(enabled: bool) -> anyhow::Result<()> {
    let key = CURRENT_USER.create(RUN_KEY).context("opening the Run key")?;
    if enabled {
        key.set_string(VALUE, command()?).context("adding Kudu to the Run key")?;
    } else if key.get_string(VALUE).is_ok() {
        key.remove_value(VALUE).context("removing Kudu from the Run key")?;
    }
    // Drop a Task Manager "disabled" mark, so turning it on here really turns it on.
    if let Ok(approved) = CURRENT_USER.create(APPROVED_KEY) {
        if approved.get_value(VALUE).is_ok() {
            let _ = approved.remove_value(VALUE);
        }
    }
    Ok(())
}
