//! The global keyboard listener, reading keyboards through evdev.
//!
//! Why evdev: Wayland gives no app a global view of the keyboard (by design), and the X11 ways
//! (XRecord, XInput2 raw events) don't exist on Wayland. Reading `/dev/input/event*` works the
//! same on X11 and on every Wayland compositor; it needs the user to be in the `input` group.
//!
//! Linux can't hide single events from other apps without grabbing the whole device (and
//! re-injecting everything else), so the matcher's `swallow` is ignored here. The default
//! shortcuts are modifier-only, so nothing visible leaks; a key shortcut like Alt + Space still
//! reaches the focused app.

use std::collections::{BTreeMap, HashSet};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use evdev::{Device, EventSummary, KeyCode};
use kudu_core::shortcut::{Key, Matcher, Modifier, Notation, Recorded, Recorder, Shortcut};
use parking_lot::Mutex;

use super::keys::{self, KEY_A, KEY_ESC, KEY_LEFTCTRL};
use crate::platform::PlatformEvent;

/// Name of Kudu's own virtual keyboard, whose events must not be read back as shortcuts.
pub const VIRTUAL_KEYBOARD_NAME: &str = "Kudu virtual keyboard";

/// How often to look for newly plugged-in keyboards.
const RESCAN_INTERVAL: Duration = Duration::from_secs(2);

pub struct Keyboards {
    shared: Arc<Shared>,
}

struct Shared {
    state: Mutex<State>,
    started: AtomicBool,
    next_device: AtomicU64,
}

#[derive(Default)]
struct State {
    events: Option<Sender<PlatformEvent>>,
    matcher: Option<Arc<Mutex<Matcher>>>,
    recorder: Option<Recorder>,
    /// Modifier keys held, per (device, key code), so the same key on two keyboards and a
    /// keyboard unplugged mid-press are handled right.
    held: BTreeMap<(u64, u16), Modifier>,
    /// Device nodes being read.
    reading: HashSet<PathBuf>,
    /// Device nodes that aren't keyboards (or are ours), by path and inode so a reused node
    /// number for a new device is looked at again.
    ignored: HashSet<(PathBuf, u64)>,
    /// Keyboards found so far (for setup checks).
    keyboards: usize,
}

impl State {
    fn held_set(&self) -> Vec<Modifier> {
        let mut held: Vec<Modifier> = self.held.values().copied().collect();
        held.sort();
        held.dedup();
        held
    }
}

impl Keyboards {
    pub fn new(events: Sender<PlatformEvent>) -> Self {
        let state = State { events: Some(events), ..Default::default() };
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(state),
                started: AtomicBool::new(false),
                next_device: AtomicU64::new(1),
            }),
        }
    }

    /// Starts reading every keyboard, now and as they're plugged in.
    pub fn start(&self, matcher: Arc<Mutex<Matcher>>, events: Sender<PlatformEvent>) -> anyhow::Result<()> {
        {
            let mut state = self.shared.state.lock();
            state.matcher = Some(matcher);
            state.events = Some(events);
        }
        if self.shared.started.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let found = self.shared.scan();
        if found == 0 {
            log::warn!("No readable keyboard in /dev/input; is the user in the input group?");
        } else {
            log::info!("Listening to {found} keyboard(s)");
        }
        let shared = self.shared.clone();
        std::thread::Builder::new().name("kudu-keyboards".into()).spawn(move || loop {
            std::thread::sleep(RESCAN_INTERVAL);
            shared.scan();
        })?;
        Ok(())
    }

    pub fn record(&self) {
        let mut state = self.shared.state.lock();
        state.recorder = Some(Recorder::new());
    }

    pub fn cancel_recording(&self) {
        self.shared.state.lock().recorder = None;
    }

    /// Whether any physical modifier key is held right now.
    pub fn modifiers_held(&self) -> bool {
        !self.shared.state.lock().held.is_empty()
    }

    /// Waits (up to `max`) for the user to let go of every modifier key, so a synthesized
    /// Ctrl + V isn't combined with a still-held Alt or Super.
    pub fn wait_for_modifiers_released(&self, max: Duration) {
        let deadline = Instant::now() + max;
        while self.modifiers_held() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn is_listening(&self) -> bool {
        self.shared.state.lock().keyboards > 0
    }
}

/// Whether a device is a real keyboard (it has letters and Ctrl; power buttons, mice and
/// headsets with a few keys don't).
fn is_keyboard(device: &Device) -> bool {
    device.name() != Some(VIRTUAL_KEYBOARD_NAME)
        && device
            .supported_keys()
            .is_some_and(|keys| keys.contains(KeyCode::new(KEY_A)) && keys.contains(KeyCode::new(KEY_LEFTCTRL)))
}

/// The `/dev/input/event*` nodes.
pub fn event_nodes() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir("/dev/input") else { return Vec::new() };
    let mut nodes: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("event")))
        .collect();
    nodes.sort();
    nodes
}

/// Whether at least one keyboard can be opened (for setup checks, without starting anything).
pub fn any_keyboard_readable() -> bool {
    event_nodes().iter().any(|path| Device::open(path).is_ok_and(|d| is_keyboard(&d)))
}

impl Shared {
    /// Opens keyboards not read yet. Returns how many are being read in total.
    fn scan(self: &Arc<Self>) -> usize {
        for path in event_nodes() {
            let inode = std::fs::metadata(&path).map(|m| m.ino()).unwrap_or(0);
            {
                let state = self.state.lock();
                if state.reading.contains(&path) || state.ignored.contains(&(path.clone(), inode)) {
                    continue;
                }
            }
            let device = match Device::open(&path) {
                Ok(device) => device,
                // Permission denied (not in the input group yet) or a node that just vanished:
                // try again next scan.
                Err(_) => continue,
            };
            if !is_keyboard(&device) {
                self.state.lock().ignored.insert((path, inode));
                continue;
            }
            let id = self.next_device.fetch_add(1, Ordering::Relaxed);
            log::info!("Reading keyboard {:?} ({})", device.name().unwrap_or("unnamed"), path.display());
            {
                let mut state = self.state.lock();
                state.reading.insert(path.clone());
                state.keyboards += 1;
            }
            let shared = self.clone();
            let spawned = std::thread::Builder::new()
                .name("kudu-keyboard".into())
                .spawn(move || shared.read(device, id, path.clone()));
            if let Err(e) = spawned {
                log::error!("Couldn't start a keyboard thread: {e}");
            }
        }
        self.state.lock().reading.len()
    }

    fn read(&self, mut device: Device, id: u64, path: PathBuf) {
        loop {
            match device.fetch_events() {
                Ok(events) => {
                    for event in events {
                        if let EventSummary::Key(_, code, value) = event.destructure() {
                            self.key(id, code.code(), value);
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    // ENODEV when unplugged.
                    log::info!("Stopped reading {}: {e}", path.display());
                    break;
                }
            }
        }
        self.device_gone(id, &path);
    }

    fn device_gone(&self, id: u64, path: &Path) {
        let mut state = self.state.lock();
        state.reading.remove(path);
        state.keyboards = state.keyboards.saturating_sub(1);
        let before = state.held_set();
        state.held.retain(|(device, _), _| *device != id);
        let after = state.held_set();
        if before != after {
            Self::modifiers_changed(&mut state, &after);
        }
    }

    /// One key event: `value` is 1 for down, 0 for up, 2 for auto-repeat.
    fn key(&self, device: u64, code: u16, value: i32) {
        let mut state = self.state.lock();
        if let Some(modifier) = keys::modifier_for(code) {
            let before = state.held_set();
            match value {
                0 => {
                    state.held.remove(&(device, code));
                }
                1 => {
                    state.held.insert((device, code), modifier);
                }
                _ => return, // a held modifier repeating changes nothing
            }
            let after = state.held_set();
            if before != after {
                Self::modifiers_changed(&mut state, &after);
            }
            return;
        }

        let now = Instant::now();
        if state.recorder.is_some() {
            if value == 1 {
                let key = Key { code: code as u32, label: keys::key_label(code) };
                let result = state.recorder.as_mut().map(|r| r.key_down(key, KEY_ESC as u32));
                if let Some(result) = result {
                    if result != Recorded::Pending {
                        state.recorder = None;
                    }
                    send(&state, PlatformEvent::Recorded(result));
                }
            }
            return;
        }
        let Some(matcher) = state.matcher.clone() else { return };
        let outcome = match value {
            0 => matcher.lock().key_up(code as u32, now),
            _ => matcher.lock().key_down(code as u32, value == 2, now),
        };
        for event in outcome.events {
            send(&state, PlatformEvent::Hotkey(event));
        }
    }

    fn modifiers_changed(state: &mut State, held: &[Modifier]) {
        let now = Instant::now();
        // The matcher always follows the modifiers, so it's in step when recording ends; its
        // events are dropped while recording (holding the old shortcut mustn't dictate).
        let outcome = state.matcher.as_ref().map(|m| m.lock().modifiers_changed(held, now));
        if let Some(recorder) = state.recorder.as_mut() {
            let result = recorder.modifiers_changed(held);
            let preview = Shortcut::new(recorder.preview(), None).keycaps(Notation::Linux);
            if result == Recorded::Pending {
                send(state, PlatformEvent::RecordingPreview(preview));
            } else {
                state.recorder = None;
                send(state, PlatformEvent::Recorded(result));
            }
            return;
        }
        if let Some(outcome) = outcome {
            for event in outcome.events {
                send(state, PlatformEvent::Hotkey(event));
            }
        }
    }
}

fn send(state: &State, event: PlatformEvent) {
    if let Some(events) = &state.events {
        if events.send(event).is_err() {
            log::debug!("The app stopped listening to platform events");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kudu_core::shortcut::{HotkeyEvent, ModifierKind, Shortcuts};

    fn keyboards() -> (Keyboards, crossbeam_channel::Receiver<PlatformEvent>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        let keyboards = Keyboards::new(tx);
        keyboards.shared.state.lock().matcher =
            Some(Arc::new(Mutex::new(Matcher::new(Shortcuts::default_pc(keys::KEY_V as u32), KEY_ESC as u32))));
        (keyboards, rx)
    }

    fn hotkeys(rx: &crossbeam_channel::Receiver<PlatformEvent>) -> Vec<HotkeyEvent> {
        rx.try_iter()
            .filter_map(|e| match e {
                PlatformEvent::Hotkey(h) => Some(h),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn tracks_modifiers_across_keyboards() {
        let (k, rx) = keyboards();
        let s = &k.shared;
        s.key(1, KEY_LEFTCTRL, 1);
        s.key(2, keys::KEY_LEFTMETA, 1);
        assert_eq!(hotkeys(&rx), vec![HotkeyEvent::Pressed]);
        s.key(2, keys::KEY_LEFTMETA, 2); // repeat
        assert!(hotkeys(&rx).is_empty());
        // Unplugging the second keyboard releases its keys.
        s.device_gone(2, Path::new("/dev/input/event9"));
        assert_eq!(hotkeys(&rx), vec![HotkeyEvent::Released]);
        assert!(k.modifiers_held());
        s.key(1, KEY_LEFTCTRL, 0);
        assert!(!k.modifiers_held());
    }

    #[test]
    fn records_a_shortcut() {
        let (k, rx) = keyboards();
        k.record();
        let s = &k.shared;
        s.key(1, keys::KEY_RIGHTALT, 1);
        s.key(1, keys::KEY_RIGHTALT, 0);
        let events: Vec<PlatformEvent> = rx.try_iter().collect();
        assert!(matches!(&events[0], PlatformEvent::RecordingPreview(caps) if caps == &vec!["Right Alt".to_string()]));
        match &events[1] {
            PlatformEvent::Recorded(Recorded::Done(shortcut)) => {
                assert_eq!(shortcut.modifiers, vec![Modifier::new(ModifierKind::Alt, kudu_core::shortcut::Side::Right)]);
            }
            other => panic!("unexpected {other:?}"),
        }
        // Recording is over: holding the dictation keys dictates again.
        s.key(1, KEY_LEFTCTRL, 1);
        s.key(1, keys::KEY_LEFTMETA, 1);
        assert_eq!(hotkeys(&rx), vec![HotkeyEvent::Pressed]);
    }

    #[test]
    fn records_a_key_with_modifiers_and_escape_cancels() {
        let (k, rx) = keyboards();
        k.record();
        k.shared.key(1, keys::KEY_LEFTALT, 1);
        k.shared.key(1, keys::KEY_SPACE, 1);
        let last = rx.try_iter().last();
        assert!(matches!(last, Some(PlatformEvent::Recorded(Recorded::Done(s))) if s.key.as_ref().is_some_and(|k| k.label == "Space")));
        k.shared.key(1, keys::KEY_LEFTALT, 0);
        k.record();
        k.shared.key(1, KEY_ESC, 1);
        assert!(matches!(rx.try_iter().last(), Some(PlatformEvent::Recorded(Recorded::Cancelled))));
    }
}
