//! Text around the cursor and on screen through AT-SPI, the Linux accessibility bus.
//!
//! AT-SPI has no "what has focus?" call, so a background thread listens for focus changes and
//! remembers the last focused object. Every query checks that object is still focused and its
//! window still active, so an app that doesn't expose accessibility (many Qt apps unless
//! accessibility is switched on, Electron and Chromium apps without a screen reader) isn't
//! mistaken for the previous one. Password fields and terminals are never read.
//!
//! Every D-Bus call has a short timeout, so a hung app can't stall dictation.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Message;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use super::focus::{nice_name, Focus};
use super::util;
use crate::platform::FocusedApp;

const CALL_TIMEOUT: Duration = Duration::from_millis(200);
const RECONNECT_DELAY: Duration = Duration::from_secs(20);

const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
const TEXT: &str = "org.a11y.atspi.Text";
const NULL_PATH: &str = "/org/a11y/atspi/null";

// AtspiRole values.
const ROLE_ALERT: u32 = 2;
const ROLE_DIALOG: u32 = 16;
const ROLE_FILE_CHOOSER: u32 = 19;
const ROLE_FRAME: u32 = 23;
const ROLE_LABEL: u32 = 29;
const ROLE_LIST_ITEM: u32 = 32;
const ROLE_PASSWORD_TEXT: u32 = 40;
const ROLE_TABLE_CELL: u32 = 56;
const ROLE_TERMINAL: u32 = 60;
const ROLE_WINDOW: u32 = 69;
const ROLE_PARAGRAPH: u32 = 73;
const ROLE_APPLICATION: u32 = 75;
const ROLE_HEADING: u32 = 83;
const ROLE_LINK: u32 = 88;
const ROLE_STATIC: u32 = 116;

// AtspiStateType bits.
const STATE_ACTIVE: u32 = 1;
const STATE_EDITABLE: u32 = 7;
const STATE_FOCUSED: u32 = 12;

/// An accessible object: the app's bus name and the object's path.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Obj {
    bus: String,
    path: String,
}

impl Obj {
    fn from_pair((bus, path): (String, OwnedObjectPath)) -> Option<Self> {
        let path = path.as_str().to_string();
        (!bus.is_empty() && path != NULL_PATH).then_some(Self { bus, path })
    }
}

pub struct Atspi {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    conn: Mutex<Option<Connection>>,
    /// The last object that reported gaining focus, and its window once looked up.
    focus: Mutex<Option<(Obj, Option<Obj>)>>,
}

impl Atspi {
    /// Connects to the accessibility bus in the background and starts following focus.
    pub fn start() -> Self {
        let inner = Arc::new(Inner::default());
        let worker = inner.clone();
        let spawned = std::thread::Builder::new().name("kudu-atspi".into()).spawn(move || loop {
            match connect() {
                Ok(conn) => {
                    *worker.conn.lock() = Some(conn.clone());
                    if let Err(e) = worker.follow_focus(&conn) {
                        log::info!("AT-SPI focus listener stopped: {e}");
                    }
                    *worker.conn.lock() = None;
                }
                Err(e) => log::debug!("No accessibility bus: {e}"),
            }
            std::thread::sleep(RECONNECT_DELAY);
        });
        if let Err(e) = spawned {
            log::warn!("Couldn't start the AT-SPI thread: {e}");
        }
        Self { inner }
    }

    /// Up to `limit` characters before the caret in the focused text field.
    pub fn text_before_cursor(&self, limit: i32) -> Option<String> {
        let (conn, obj) = self.inner.focused_field()?;
        let caret = get_i32(&conn, &obj, TEXT, "CaretOffset")?;
        if caret <= 0 {
            return (caret == 0).then(String::new);
        }
        get_text(&conn, &obj, (caret - limit).max(0), caret)
    }

    /// The whole text of the focused text field (up to 100 000 characters).
    pub fn field_text(&self) -> Option<String> {
        let (conn, obj) = self.inner.focused_field()?;
        let count = get_i32(&conn, &obj, TEXT, "CharacterCount")?;
        if !(0..=100_000).contains(&count) {
            return None;
        }
        get_text(&conn, &obj, 0, count)
    }

    /// Whether the focused object is a terminal (VTE terminals report this role).
    pub fn focused_is_terminal(&self) -> bool {
        self.inner.focused_in_active_window().is_some_and(|(conn, obj, _)| role(&conn, &obj) == Some(ROLE_TERMINAL))
    }

    /// The text selected in the focused object, without touching the clipboard.
    pub fn selected_text(&self) -> Option<String> {
        let (conn, obj, _) = self.inner.focused_in_active_window()?;
        if matches!(role(&conn, &obj)?, ROLE_PASSWORD_TEXT) {
            return None;
        }
        let count: i32 = call(&conn, &obj, TEXT, "GetNSelections", &())?.body().deserialize().ok()?;
        if count <= 0 {
            return None;
        }
        let (start, end): (i32, i32) = call(&conn, &obj, TEXT, "GetSelection", &(0i32,))?.body().deserialize().ok()?;
        if end <= start {
            return None;
        }
        get_text(&conn, &obj, start, end).filter(|t| !t.trim().is_empty())
    }

    /// The focused app according to AT-SPI (for desktops that don't tell otherwise).
    pub fn focused_app(&self) -> Option<Focus> {
        let (conn, obj, window) = self.inner.focused_in_active_window()?;
        let app = call(&conn, &obj, ACCESSIBLE, "GetApplication", &())
            .and_then(|m| m.body().deserialize::<(String, OwnedObjectPath)>().ok())
            .and_then(Obj::from_pair);
        let app_name = app.as_ref().and_then(|a| get_string(&conn, a, ACCESSIBLE, "Name")).unwrap_or_default();
        let pid = conn
            .call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "GetConnectionUnixProcessID",
                &(obj.bus.as_str(),),
            )
            .ok()
            .and_then(|m| m.body().deserialize::<u32>().ok());
        let process = pid.and_then(util::process_name);
        let id = if app_name.is_empty() { process.clone()? } else { app_name };
        let title = get_string(&conn, &window, ACCESSIBLE, "Name").unwrap_or_default();
        Some(Focus { app: FocusedApp { name: nice_name(&id), id, title }, process })
    }

    /// Names and short texts visible in the focused window, gathered breadth-first until
    /// `budget` runs out.
    pub fn screen_text(&self, budget: Duration) -> Vec<String> {
        let deadline = Instant::now() + budget;
        let Some((conn, _, window)) = self.inner.focused_in_active_window() else { return Vec::new() };
        let mut texts = Vec::new();
        let mut seen_texts = HashSet::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::from([window]);
        while let Some(obj) = queue.pop_front() {
            if Instant::now() >= deadline || visited.len() >= 4000 || texts.len() >= 400 {
                break;
            }
            if !visited.insert(obj.clone()) {
                continue;
            }
            let Some(role) = role(&conn, &obj) else {
                // A call failed or timed out: this app is slow, stop here.
                break;
            };
            if role == ROLE_PASSWORD_TEXT {
                continue;
            }
            let mut text = get_string(&conn, &obj, ACCESSIBLE, "Name").unwrap_or_default();
            if text.trim().is_empty() && is_text_role(role) {
                text = get_text(&conn, &obj, 0, 300).unwrap_or_default();
            }
            let text = text.trim();
            if !text.is_empty() && seen_texts.insert(text.to_string()) {
                texts.push(text.chars().take(300).collect());
            }
            if role == ROLE_TERMINAL {
                continue;
            }
            if let Some(children) = call(&conn, &obj, ACCESSIBLE, "GetChildren", &())
                .and_then(|m| m.body().deserialize::<Vec<(String, OwnedObjectPath)>>().ok())
            {
                queue.extend(children.into_iter().filter_map(Obj::from_pair));
            }
        }
        texts
    }
}

impl Inner {
    fn connection(&self) -> Option<Connection> {
        self.conn.lock().clone()
    }

    /// Listens for focus changes until the connection breaks.
    fn follow_focus(&self, conn: &Connection) -> zbus::Result<()> {
        register_event(conn, "object:state-changed:focused");
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.a11y.atspi.Event.Object")?
            .member("StateChanged")?
            .arg(0, "focused")?
            .build();
        let messages = MessageIterator::for_match_rule(rule, conn, Some(64))?;
        log::info!("Following focus through AT-SPI");
        for message in messages {
            let message = message?;
            let header = message.header();
            let (Some(sender), Some(path)) = (header.sender(), header.path()) else { continue };
            let Some(gained) = focus_gained(&message) else { continue };
            let obj = Obj { bus: sender.to_string(), path: path.to_string() };
            let mut focus = self.focus.lock();
            if gained {
                *focus = Some((obj, None));
            } else if focus.as_ref().is_some_and(|(f, _)| *f == obj) {
                *focus = None;
            }
        }
        Ok(())
    }

    /// The focused object, if its window is the active one, with that window.
    fn focused_in_active_window(&self) -> Option<(Connection, Obj, Obj)> {
        let conn = self.connection()?;
        let (obj, cached_window) = self.focus.lock().clone()?;
        if states(&conn, &obj)? & bit(STATE_FOCUSED) == 0 {
            return None;
        }
        let window = match cached_window {
            Some(window) => window,
            None => {
                let window = window_of(&conn, &obj)?;
                if let Some((current, cached)) = self.focus.lock().as_mut() {
                    if *current == obj {
                        *cached = Some(window.clone());
                    }
                }
                window
            }
        };
        if states(&conn, &window)? & bit(STATE_ACTIVE) == 0 {
            return None; // focus moved to an app that doesn't report it
        }
        Some((conn, obj, window))
    }

    /// The focused object if it's an editable text field we may read.
    fn focused_field(&self) -> Option<(Connection, Obj)> {
        let (conn, obj, _) = self.focused_in_active_window()?;
        let role = role(&conn, &obj)?;
        if role == ROLE_PASSWORD_TEXT || role == ROLE_TERMINAL {
            return None;
        }
        if states(&conn, &obj)? & bit(STATE_EDITABLE) == 0 {
            return None;
        }
        Some((conn, obj))
    }
}

fn connect() -> zbus::Result<Connection> {
    let session = Connection::session()?;
    let address: String = session
        .call_method(Some("org.a11y.Bus"), "/org/a11y/bus", Some("org.a11y.Bus"), "GetAddress", &())?
        .body()
        .deserialize()?;
    zbus::blocking::connection::Builder::address(address.as_str())?.method_timeout(CALL_TIMEOUT).build()
}

/// Asks apps to emit an event (toolkits only send events someone registered for).
fn register_event(conn: &Connection, event: &str) {
    let (dest, path, iface) = ("org.a11y.atspi.Registry", "/org/a11y/atspi/registry", "org.a11y.atspi.Registry");
    let registered = conn.call_method(Some(dest), path, Some(iface), "RegisterEvent", &(event, Vec::<String>::new(), ""));
    if registered.is_err() {
        // at-spi2-core before 2.46 takes just the event name.
        let _ = conn.call_method(Some(dest), path, Some(iface), "RegisterEvent", &(event,));
    }
}

/// From a `StateChanged` signal: whether the object gained (true) or lost (false) focus.
fn focus_gained(message: &Message) -> Option<bool> {
    let body = message.body();
    // The last argument changed type across at-spi versions: a dict now, a (so) before.
    let (kind, detail) = body
        .deserialize::<(String, i32, i32, OwnedValue, std::collections::HashMap<String, OwnedValue>)>()
        .map(|(kind, detail, _, _, _)| (kind, detail))
        .or_else(|_| {
            body.deserialize::<(String, i32, i32, OwnedValue, (String, OwnedObjectPath))>()
                .map(|(kind, detail, _, _, _)| (kind, detail))
        })
        .ok()?;
    (kind == "focused").then_some(detail != 0)
}

fn bit(state: u32) -> u64 {
    1u64 << state
}

fn call<B>(conn: &Connection, obj: &Obj, iface: &str, method: &str, body: &B) -> Option<Message>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    conn.call_method(Some(obj.bus.as_str()), obj.path.as_str(), Some(iface), method, body).ok()
}

fn property(conn: &Connection, obj: &Obj, iface: &str, name: &str) -> Option<Value<'static>> {
    let message = call(conn, obj, "org.freedesktop.DBus.Properties", "Get", &(iface, name))?;
    let value: OwnedValue = message.body().deserialize().ok()?;
    Some(Value::from(value))
}

fn get_i32(conn: &Connection, obj: &Obj, iface: &str, name: &str) -> Option<i32> {
    property(conn, obj, iface, name)?.downcast::<i32>().ok()
}

fn get_string(conn: &Connection, obj: &Obj, iface: &str, name: &str) -> Option<String> {
    property(conn, obj, iface, name)?.downcast::<String>().ok()
}

fn get_text(conn: &Connection, obj: &Obj, start: i32, end: i32) -> Option<String> {
    call(conn, obj, TEXT, "GetText", &(start, end))?.body().deserialize::<String>().ok()
}

fn role(conn: &Connection, obj: &Obj) -> Option<u32> {
    call(conn, obj, ACCESSIBLE, "GetRole", &())?.body().deserialize::<u32>().ok()
}

/// The object's states as a bit set (AT-SPI sends two 32-bit words).
fn states(conn: &Connection, obj: &Obj) -> Option<u64> {
    let words: Vec<u32> = call(conn, obj, ACCESSIBLE, "GetState", &())?.body().deserialize().ok()?;
    Some(state_bits(&words))
}

fn state_bits(words: &[u32]) -> u64 {
    words.iter().take(2).enumerate().fold(0u64, |bits, (i, &w)| bits | ((w as u64) << (32 * i)))
}

fn parent(conn: &Connection, obj: &Obj) -> Option<Obj> {
    let value = property(conn, obj, ACCESSIBLE, "Parent")?;
    value.downcast::<(String, OwnedObjectPath)>().ok().and_then(Obj::from_pair)
}

/// The window containing an object: the nearest frame/window/dialog ancestor, or the
/// top-level object under the application.
fn window_of(conn: &Connection, obj: &Obj) -> Option<Obj> {
    let mut current = obj.clone();
    for _ in 0..64 {
        let current_role = role(conn, &current)?;
        if matches!(current_role, ROLE_FRAME | ROLE_WINDOW | ROLE_DIALOG | ROLE_ALERT | ROLE_FILE_CHOOSER) {
            return Some(current);
        }
        let parent = parent(conn, &current)?;
        if role(conn, &parent)? == ROLE_APPLICATION {
            return Some(current);
        }
        current = parent;
    }
    None
}

fn is_text_role(role: u32) -> bool {
    matches!(role, ROLE_PARAGRAPH | ROLE_HEADING | ROLE_LABEL | ROLE_STATIC | ROLE_LIST_ITEM | ROLE_TABLE_CELL | ROLE_LINK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combines_state_words() {
        assert_eq!(state_bits(&[bit(STATE_FOCUSED) as u32 | bit(STATE_EDITABLE) as u32, 0]) & bit(STATE_FOCUSED), bit(STATE_FOCUSED));
        assert_eq!(state_bits(&[0, 1]), 1u64 << 32);
        assert_eq!(state_bits(&[]), 0);
    }

    #[test]
    fn ignores_null_objects() {
        let null = OwnedObjectPath::try_from(NULL_PATH).unwrap();
        assert_eq!(Obj::from_pair((":1.5".into(), null)), None);
        let path = OwnedObjectPath::try_from("/org/a11y/atspi/accessible/12").unwrap();
        assert!(Obj::from_pair((":1.5".into(), path.clone())).is_some());
        assert_eq!(Obj::from_pair((String::new(), path)), None);
    }
}
