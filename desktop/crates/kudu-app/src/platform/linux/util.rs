//! Small helpers: the desktop session, external commands with timeouts.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Which compositor or desktop Kudu runs under, from the session's environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    /// A Wayland session (`WAYLAND_DISPLAY` set or `XDG_SESSION_TYPE=wayland`).
    pub wayland: bool,
    /// An X server is reachable: the X11 session itself, or XWayland under Wayland.
    pub x11: bool,
    pub desktop: Desktop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Desktop {
    Gnome,
    Kde,
    Hyprland,
    Sway,
    Other,
}

impl Session {
    pub fn detect() -> Self {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
        Self::from_env(var)
    }

    pub fn from_env(var: impl Fn(&str) -> Option<String>) -> Self {
        let session_type = var("XDG_SESSION_TYPE").unwrap_or_default().to_ascii_lowercase();
        let wayland = var("WAYLAND_DISPLAY").is_some() || session_type == "wayland";
        let x11 = var("DISPLAY").is_some();
        let current = var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_ascii_lowercase();
        let desktop = if var("HYPRLAND_INSTANCE_SIGNATURE").is_some() || current.contains("hyprland") {
            Desktop::Hyprland
        } else if var("SWAYSOCK").is_some() || current.split(':').any(|d| d == "sway") {
            Desktop::Sway
        } else if current.contains("kde") || var("KDE_FULL_SESSION").is_some() {
            Desktop::Kde
        } else if current.contains("gnome") || current.contains("unity") || current.contains("ubuntu") {
            Desktop::Gnome
        } else {
            Desktop::Other
        };
        Self { wayland, x11, desktop }
    }

    /// Whether the X server is the session's own (not XWayland).
    pub fn is_x11_session(&self) -> bool {
        self.x11 && !self.wayland
    }
}

/// The full path of a program on `PATH`, if installed.
pub fn which(program: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join(program)).find(|candidate| {
        candidate
            .metadata()
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    })
}

/// Runs a program and returns its standard output, or `None` if it fails, isn't installed or
/// takes longer than `timeout` (then it's killed).
pub fn run(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    // Read on another thread: a large output (a window tree) would otherwise fill the pipe
    // and block the program forever.
    let (tx, rx) = crossbeam_channel::bounded(1);
    std::thread::spawn(move || {
        let mut output = Vec::new();
        let _ = stdout.read_to_end(&mut output);
        let _ = tx.send(output);
    });
    let started = Instant::now();
    let output = match rx.recv_timeout(timeout) {
        Ok(output) => output,
        Err(_) => {
            log::debug!("{program} took longer than {timeout:?}; stopped it");
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    };
    let remaining = timeout.saturating_sub(started.elapsed()).max(Duration::from_millis(20));
    if !wait_for(&mut child, remaining)?.success() {
        return None;
    }
    String::from_utf8(output).ok()
}

/// Runs a program with `input` on its standard input; true if it exits successfully within
/// `timeout`. Its output is discarded (`wl-copy` forks a server that keeps any pipe open).
pub fn run_with_input(program: &str, args: &[&str], input: &str, timeout: Duration) -> bool {
    let Ok(mut child) = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(input.as_bytes()).is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return false;
        }
    }
    wait_for(&mut child, timeout).is_some_and(|status| status.success())
}

/// Waits for a child to exit, killing it after `timeout`.
fn wait_for(child: &mut Child, timeout: Duration) -> Option<std::process::ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(2)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// The short process name of a pid (`/proc/<pid>/comm`).
pub fn process_name(pid: u32) -> Option<String> {
    let name = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn session(vars: &[(&str, &str)]) -> Session {
        let map: HashMap<String, String> = vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        Session::from_env(|name| map.get(name).cloned())
    }

    #[test]
    fn detects_the_desktop() {
        let gnome = session(&[("WAYLAND_DISPLAY", "wayland-0"), ("DISPLAY", ":0"), ("XDG_CURRENT_DESKTOP", "ubuntu:GNOME")]);
        assert_eq!(gnome, Session { wayland: true, x11: true, desktop: Desktop::Gnome });
        assert!(!gnome.is_x11_session());

        let hypr = session(&[("WAYLAND_DISPLAY", "wayland-1"), ("HYPRLAND_INSTANCE_SIGNATURE", "abc")]);
        assert_eq!(hypr.desktop, Desktop::Hyprland);
        assert!(!hypr.x11);

        assert_eq!(session(&[("SWAYSOCK", "/run/sway.sock")]).desktop, Desktop::Sway);
        assert_eq!(session(&[("XDG_CURRENT_DESKTOP", "KDE")]).desktop, Desktop::Kde);

        let x = session(&[("DISPLAY", ":0"), ("XDG_SESSION_TYPE", "x11"), ("XDG_CURRENT_DESKTOP", "XFCE")]);
        assert!(x.is_x11_session());
        assert_eq!(x.desktop, Desktop::Other);
    }

    #[test]
    fn runs_commands_with_a_timeout() {
        assert_eq!(run("sh", &["-c", "printf hi"], Duration::from_secs(2)).as_deref(), Some("hi"));
        assert_eq!(run("sh", &["-c", "exit 3"], Duration::from_secs(2)), None);
        let started = Instant::now();
        assert_eq!(run("sh", &["-c", "sleep 5"], Duration::from_millis(100)), None);
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(run_with_input("sh", &["-c", "cat >/dev/null"], "text", Duration::from_secs(2)));
        assert_eq!(run("kudu-no-such-program", &[], Duration::from_secs(1)), None);
        assert!(which("sh").is_some());
        assert!(which("kudu-no-such-program").is_none());
    }
}
