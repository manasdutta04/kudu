//! X11 queries: the active window, who owns the PRIMARY selection, the keyboard mapping, and
//! screen geometry for the overlay. Used in X11 sessions and, under Wayland, for XWayland apps.

use std::time::{Duration, Instant};

use parking_lot::Mutex;
use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, Window};
use x11rb::rust_connection::RustConnection;

x11rb::atom_manager! {
    pub Atoms: AtomsCookie {
        _NET_ACTIVE_WINDOW,
        _NET_WM_NAME,
        _NET_WM_PID,
        _NET_WORKAREA,
        _NET_CURRENT_DESKTOP,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_NOTIFICATION,
        _NET_WM_STATE,
        _NET_WM_STATE_ABOVE,
        _NET_WM_STATE_SKIP_TASKBAR,
        _NET_WM_STATE_SKIP_PAGER,
        UTF8_STRING,
    }
}

/// A rectangle in X screen pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct XRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl XRect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }

    pub fn intersection(&self, other: &XRect) -> Option<XRect> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        (right > left && bottom > top).then(|| XRect { x: left, y: top, width: right - left, height: bottom - top })
    }
}

/// The focused X window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActiveWindow {
    /// WM_CLASS class ("firefox", "Gnome-terminal").
    pub class: String,
    /// WM_CLASS instance ("Navigator", "gnome-terminal-server").
    pub instance: String,
    pub title: String,
    pub pid: Option<u32>,
}

struct Conn {
    conn: RustConnection,
    root: Window,
    atoms: Atoms,
    resource_mask: u32,
}

/// (keysym, X key code) pairs.
type KeyMap = Vec<(u32, u8)>;

/// A lazily opened, cached connection for quick queries from any thread.
pub struct X11 {
    enabled: bool,
    conn: Mutex<Option<Conn>>,
    retry_after: Mutex<Option<Instant>>,
    /// Keysym → key code for the first layout, with when it was read.
    keycodes: Mutex<Option<(Instant, KeyMap)>>,
}

impl X11 {
    pub fn new(enabled: bool) -> Self {
        Self { enabled, conn: Mutex::new(None), retry_after: Mutex::new(None), keycodes: Mutex::new(None) }
    }

    fn with<R>(&self, f: impl FnOnce(&Conn) -> Result<R, ReplyError>) -> Option<R> {
        if !self.enabled {
            return None;
        }
        let mut guard = self.conn.lock();
        if guard.is_none() {
            if self.retry_after.lock().is_some_and(|t| Instant::now() < t) {
                return None;
            }
            match connect() {
                Ok(conn) => *guard = Some(conn),
                Err(e) => {
                    log::debug!("Can't connect to the X server: {e}");
                    *self.retry_after.lock() = Some(Instant::now() + Duration::from_secs(5));
                    return None;
                }
            }
        }
        let conn = guard.as_ref()?;
        match f(conn) {
            Ok(value) => Some(value),
            Err(ReplyError::ConnectionError(e)) => {
                log::debug!("Lost the X connection: {e}");
                *guard = None;
                None
            }
            Err(ReplyError::X11Error(_)) => None, // e.g. the window just closed
        }
    }

    /// The active window (`_NET_ACTIVE_WINDOW`), if the window manager publishes one.
    pub fn active_window(&self) -> Option<ActiveWindow> {
        self.with(|c| {
            let Some(window) = active_window_id(c)? else { return Ok(None) };
            let class = c.conn.get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)?.reply()?;
            let mut parts = class.value.split(|&b| b == 0).map(|s| String::from_utf8_lossy(s).into_owned());
            let instance = parts.next().unwrap_or_default();
            let class = parts.next().unwrap_or_default();
            let mut title = c.conn.get_property(false, window, c.atoms._NET_WM_NAME, c.atoms.UTF8_STRING, 0, 1024)?.reply()?.value;
            if title.is_empty() {
                title = c.conn.get_property(false, window, AtomEnum::WM_NAME, AtomEnum::ANY, 0, 1024)?.reply()?.value;
            }
            let pid = c
                .conn
                .get_property(false, window, c.atoms._NET_WM_PID, AtomEnum::CARDINAL, 0, 1)?
                .reply()?
                .value32()
                .and_then(|mut v| v.next());
            Ok(Some(ActiveWindow { class, instance, title: String::from_utf8_lossy(&title).into_owned(), pid }))
        })
        .flatten()
    }

    /// Whether the PRIMARY selection belongs to the active window's app, so it holds what's
    /// selected there right now (and not a stale selection from another app).
    pub fn primary_owned_by_active_app(&self) -> bool {
        self.with(|c| {
            let owner = c.conn.get_selection_owner(AtomEnum::PRIMARY.into())?.reply()?.owner;
            let Some(active) = active_window_id(c)? else { return Ok(false) };
            Ok(same_client(owner, active, c.resource_mask))
        })
        .unwrap_or(false)
    }

    /// The key code (X numbering, evdev + 8) that types `keysym` without modifiers in the
    /// first layout, e.g. where `v` is on a Dvorak keyboard.
    pub fn keycode_for(&self, keysym: u32) -> Option<u8> {
        {
            let cache = self.keycodes.lock();
            if let Some((at, map)) = cache.as_ref() {
                if at.elapsed() < Duration::from_secs(10) {
                    return map.iter().find(|(sym, _)| *sym == keysym).map(|(_, code)| *code);
                }
            }
        }
        let map = self.with(|c| {
            let setup = c.conn.setup();
            let (min, max) = (setup.min_keycode, setup.max_keycode);
            let reply = c.conn.get_keyboard_mapping(min, max - min + 1)?.reply()?;
            let per = reply.keysyms_per_keycode as usize;
            let mut map = Vec::new();
            if per > 0 {
                for (index, syms) in reply.keysyms.chunks(per).enumerate() {
                    if let Some(&sym) = syms.first() {
                        if sym != 0 && !map.iter().any(|(s, _)| *s == sym) {
                            map.push((sym, min + index as u8));
                        }
                    }
                }
            }
            Ok(map)
        })?;
        let code = map.iter().find(|(sym, _)| *sym == keysym).map(|(_, code)| *code);
        *self.keycodes.lock() = Some((Instant::now(), map));
        code
    }
}

fn connect() -> Result<Conn, Box<dyn std::error::Error>> {
    let (conn, screen) = x11rb::connect(None)?;
    let root = conn.setup().roots.get(screen).ok_or("no X screen")?.root;
    let atoms = Atoms::new(&conn)?.reply()?;
    let resource_mask = conn.setup().resource_id_mask;
    Ok(Conn { conn, root, atoms, resource_mask })
}

fn active_window_id(c: &Conn) -> Result<Option<Window>, ReplyError> {
    let reply = c.conn.get_property(false, c.root, c.atoms._NET_ACTIVE_WINDOW, AtomEnum::WINDOW, 0, 1)?.reply()?;
    Ok(reply.value32().and_then(|mut v| v.next()).filter(|&w| w != 0))
}

/// Windows created by the same X client share the resource-id base (the bits outside the
/// server's resource-id mask).
pub fn same_client(a: Window, b: Window, resource_mask: u32) -> bool {
    a != 0 && b != 0 && (a & !resource_mask) == (b & !resource_mask)
}

/// The display scale from the `Xft.dpi` resource (what GNOME, KDE and most X11 desktops set
/// for HiDPI), as a factor of 96 dpi.
pub fn scale_from_resources(resources: &str) -> Option<f32> {
    resources.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        if key.trim() != "Xft.dpi" {
            return None;
        }
        let dpi: f32 = value.trim().parse().ok()?;
        (dpi > 0.0).then(|| (dpi / 96.0).clamp(1.0, 4.0))
    })
}

/// Where the overlay window goes: centred at the bottom of the monitor, inside the work area
/// (above panels and docks), `gap` pixels from its bottom.
pub fn overlay_position(monitor: XRect, workarea: Option<XRect>, width: i32, height: i32, gap: i32) -> (i32, i32) {
    let area = workarea
        .and_then(|w| w.intersection(&monitor))
        .filter(|a| a.height >= height + gap && a.width >= width)
        .unwrap_or(monitor);
    let x = monitor.x + (monitor.width - width) / 2;
    let y = area.y + area.height - height - gap;
    (x, y)
}

/// The monitor under the pointer (or the primary, or the first).
pub fn monitor_at(monitors: &[(XRect, bool)], pointer: Option<(i32, i32)>) -> Option<XRect> {
    pointer
        .and_then(|(x, y)| monitors.iter().find(|(m, _)| m.contains(x, y)))
        .or_else(|| monitors.iter().find(|(_, primary)| *primary))
        .or_else(|| monitors.first())
        .map(|(m, _)| *m)
}

/// Screen geometry for placing the overlay, read from a connection.
pub fn overlay_geometry(conn: &impl Connection, root: Window, atoms: &Atoms, screen: XRect) -> (XRect, Option<XRect>) {
    let pointer = conn
        .query_pointer(root)
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|p| (p.root_x as i32, p.root_y as i32));
    let monitors: Vec<(XRect, bool)> = if conn.extension_information(x11rb::protocol::randr::X11_EXTENSION_NAME).ok().flatten().is_some() {
        conn.randr_get_monitors(root, true)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| {
                r.monitors
                    .iter()
                    .map(|m| (XRect { x: m.x as i32, y: m.y as i32, width: m.width as i32, height: m.height as i32 }, m.primary))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let monitor = monitor_at(&monitors, pointer).unwrap_or(screen);
    let desktop = conn
        .get_property(false, root, atoms._NET_CURRENT_DESKTOP, AtomEnum::CARDINAL, 0, 1)
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|r| r.value32().and_then(|mut v| v.next()))
        .unwrap_or(0) as usize;
    let workarea = conn
        .get_property(false, root, atoms._NET_WORKAREA, AtomEnum::CARDINAL, 0, 256)
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|r| r.value32().map(|v| v.collect::<Vec<u32>>()))
        .and_then(|values| {
            let (rects, _) = values.as_chunks::<4>();
            let chunk = rects.get(desktop).or(rects.first())?;
            Some(XRect { x: chunk[0] as i32, y: chunk[1] as i32, width: chunk[2] as i32, height: chunk[3] as i32 })
        });
    (monitor, workarea)
}

/// The scale factor for the overlay on X11: `Xft.dpi`, else `GDK_SCALE`, else 1.
pub fn overlay_scale(conn: &impl Connection, root: Window) -> f32 {
    let resources = conn
        .get_property(false, root, AtomEnum::RESOURCE_MANAGER, AtomEnum::STRING, 0, 1 << 16)
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|r| String::from_utf8_lossy(&r.value).into_owned());
    resources
        .as_deref()
        .and_then(scale_from_resources)
        .or_else(|| std::env::var("GDK_SCALE").ok()?.parse::<f32>().ok().filter(|s| *s >= 1.0))
        .unwrap_or(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_dpi_resource() {
        assert_eq!(scale_from_resources("Xft.antialias:\t1\nXft.dpi:\t192\n"), Some(2.0));
        assert_eq!(scale_from_resources("Xft.dpi: 144"), Some(1.5));
        assert_eq!(scale_from_resources("Xft.dpi:\t72"), Some(1.0));
        assert_eq!(scale_from_resources("Xcursor.size: 24"), None);
    }

    #[test]
    fn places_the_overlay_above_the_panel_of_the_pointer_monitor() {
        let left = XRect { x: 0, y: 0, width: 1920, height: 1080 };
        let right = XRect { x: 1920, y: 0, width: 2560, height: 1440 };
        let monitors = [(left, true), (right, false)];
        assert_eq!(monitor_at(&monitors, Some((2000, 100))), Some(right));
        assert_eq!(monitor_at(&monitors, Some((-5, -5))), Some(left));
        assert_eq!(monitor_at(&monitors, None), Some(left));
        assert_eq!(monitor_at(&[], None), None);

        // A 40 px panel at the bottom of the left monitor.
        let workarea = XRect { x: 0, y: 0, width: 4480, height: 1040 };
        assert_eq!(overlay_position(left, Some(workarea), 380, 44, 3), (770, 1040 - 44 - 3));
        // On the taller right monitor, the work area (which spans both) still ends at 1040.
        assert_eq!(overlay_position(right, Some(workarea), 380, 44, 3), (1920 + 1090, 1040 - 47));
        // No work area: the monitor's bottom.
        assert_eq!(overlay_position(right, None, 380, 44, 3), (3010, 1440 - 47));
        // A work area that doesn't overlap the monitor is ignored.
        let elsewhere = XRect { x: 5000, y: 0, width: 100, height: 100 };
        assert_eq!(overlay_position(left, Some(elsewhere), 380, 44, 3), (770, 1033));
    }

    #[test]
    fn tells_windows_of_the_same_client() {
        let mask = 0x001f_ffff;
        assert!(same_client(0x0340_0007, 0x0340_1a2b, mask));
        assert!(!same_client(0x0340_0007, 0x0360_0007, mask));
        assert!(!same_client(0, 0x0340_0007, mask));
    }
}
