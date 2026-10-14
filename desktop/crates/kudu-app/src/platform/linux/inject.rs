//! Synthesized key presses through a uinput virtual keyboard.
//!
//! Why uinput: it is the one way to send keys that every compositor accepts. X11's XTest
//! doesn't reach Wayland apps, the Wayland virtual-keyboard protocol isn't offered by GNOME or
//! KDE, and the portal's remote-desktop session asks the user every time. A uinput device looks
//! like a keyboard plugged in, so the compositor treats its keys like real ones.

use std::io;
use std::time::{Duration, Instant};

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode};
use parking_lot::Mutex;

use super::hotkeys::VIRTUAL_KEYBOARD_NAME;
use super::keys::VIRTUAL_KEY_MAX;

/// Compositors take a moment to set up a new input device; keys sent before are lost.
const SETTLE_TIME: Duration = Duration::from_millis(400);
/// Pause between key events, so each reaches apps as its own step (some apps miss a press and
/// release delivered together).
const KEY_GAP: Duration = Duration::from_millis(6);

pub struct VirtualKeyboard {
    device: Mutex<Option<VirtualDevice>>,
    created: Instant,
    error: Option<io::ErrorKind>,
}

impl VirtualKeyboard {
    /// Creates the device now so the compositor has picked it up by the first dictation.
    pub fn new() -> Self {
        match create() {
            Ok(device) => {
                log::info!("Created the uinput virtual keyboard");
                Self { device: Mutex::new(Some(device)), created: Instant::now(), error: None }
            }
            Err(e) => {
                log::warn!("Can't create a uinput keyboard ({e}); falling back to typing tools");
                Self { device: Mutex::new(None), created: Instant::now(), error: Some(e.kind()) }
            }
        }
    }

    pub fn is_available(&self) -> bool {
        self.device.lock().is_some()
    }

    /// Why the device couldn't be created, if it couldn't.
    pub fn error(&self) -> Option<io::ErrorKind> {
        self.error
    }

    /// Presses `modifiers` in order, taps `key`, then releases the modifiers in reverse.
    pub fn chord(&self, modifiers: &[u16], key: u16) -> io::Result<()> {
        let since = self.created.elapsed();
        if since < SETTLE_TIME {
            std::thread::sleep(SETTLE_TIME - since);
        }
        let mut guard = self.device.lock();
        let device = guard.as_mut().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no uinput keyboard"))?;
        let mut steps: Vec<(u16, i32)> = modifiers.iter().map(|&m| (m, 1)).collect();
        steps.push((key, 1));
        steps.push((key, 0));
        steps.extend(modifiers.iter().rev().map(|&m| (m, 0)));
        let mut result = Ok(());
        for (code, value) in steps {
            // `emit` adds the SYN_REPORT that ends each step.
            if let Err(e) = device.emit(&[InputEvent::new(EventType::KEY.0, code, value)]) {
                result = Err(e);
                // Keep going so a modifier isn't left pressed.
            }
            std::thread::sleep(KEY_GAP);
        }
        result
    }
}

fn create() -> io::Result<VirtualDevice> {
    // Declare the whole classic key range: the letter a layout puts V on can be any key.
    let mut keys = AttributeSet::<KeyCode>::new();
    for code in 1..=VIRTUAL_KEY_MAX {
        keys.insert(KeyCode::new(code));
    }
    VirtualDevice::builder()?.name(VIRTUAL_KEYBOARD_NAME).with_keys(&keys)?.build()
}
