//! Synthesized keyboard input (`SendInput`).

use std::sync::atomic::{AtomicU8, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use ::windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyboardLayout, MapVirtualKeyExW, SendInput, HKL, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, VIRTUAL_KEY,
    VK_CONTROL, VK_RETURN, VK_TAB,
};
use ::windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use super::keys::{MENU_KEY_BITS, MODIFIER_KEYS};

/// Marks Kudu's own synthesized events (`dwExtraInfo`), so the keyboard hook lets them pass
/// untouched: Kudu's paste must never be matched as a shortcut or swallowed.
pub const KUDU_INPUT_TAG: usize = 0x4B55_4455; // "KUDU"

/// An unassigned virtual key. Windows opens the Start menu when Win is released, and the
/// menu bar when Alt is released, only if no other key was pressed in between; a press of this
/// key in between stops that without any other effect.
pub const VK_MASK: u16 = 0xE8;

/// Modifiers (bits as in `MODIFIER_KEYS`) that Kudu released synthetically while the user still
/// holds them. Windows now reports them as up, but the keyboard hook must keep counting them as
/// held until their physical release.
pub static FORCED_UP: AtomicU8 = AtomicU8::new(0);

/// Right Ctrl, Right Alt and both Win keys are "extended" keys.
const EXTENDED_MODIFIER_BITS: u8 = 0b1100_1010;

fn key(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: KUDU_INPUT_TAG },
        },
    }
}

fn send(inputs: &[INPUT]) -> bool {
    if inputs.is_empty() {
        return true;
    }
    // SAFETY: `inputs` is a valid slice of initialized INPUT structures.
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        log::warn!("SendInput inserted {sent} of {} events", inputs.len());
        return false;
    }
    true
}

/// The keyboard layout of the app that will receive the keys.
fn foreground_layout() -> HKL {
    // SAFETY: plain queries; a null foreground window gives thread 0, i.e. the current layout.
    unsafe {
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        GetKeyboardLayout(thread)
    }
}

fn scan_code(vk: u16, layout: HKL) -> u16 {
    // SAFETY: plain query.
    unsafe { MapVirtualKeyExW(vk as u32, MAPVK_VK_TO_VSC, Some(layout)) as u16 }
}

/// What follows hidden modifier presses in [`replay_hidden`]: the key that showed they belong
/// to one of the user's own shortcuts.
pub enum Then {
    /// Another modifier (bit as in `MODIFIER_KEYS`) went down.
    Modifier(usize),
    /// A regular key went down.
    Key { vk: u32, scan: u32, extended: bool },
}

/// Presses, for the system, modifiers whose presses Kudu hid (they started one of Kudu's
/// shortcuts), followed by the key that turned out to make them the user's own shortcut
/// (Alt + Tab, Alt + Shift…), in the order they were pressed. Their physical releases then
/// reach the system as usual.
pub fn replay_hidden(hidden: u8, then: Then) {
    let layout = foreground_layout();
    let modifier = |bit: usize, flags: KEYBD_EVENT_FLAGS| {
        let vk = MODIFIER_KEYS[bit].0 as u16;
        let flags = if EXTENDED_MODIFIER_BITS & (1 << bit) != 0 { flags | KEYEVENTF_EXTENDEDKEY } else { flags };
        key(vk, scan_code(vk, layout), flags)
    };
    let mut inputs: Vec<INPUT> =
        (0..MODIFIER_KEYS.len()).filter(|bit| hidden & (1 << bit) != 0).map(|bit| modifier(bit, KEYBD_EVENT_FLAGS(0))).collect();
    inputs.push(match then {
        Then::Modifier(bit) => modifier(bit, KEYBD_EVENT_FLAGS(0)),
        Then::Key { vk, scan, extended } => {
            let flags = if extended { KEYEVENTF_EXTENDEDKEY } else { KEYBD_EVENT_FLAGS(0) };
            key(vk as u16, scan as u16, flags)
        }
    });
    send(&inputs);
}

/// Presses and releases the mask key (see [`VK_MASK`]).
pub fn send_mask_key() {
    send(&[key(VK_MASK, 0, KEYBD_EVENT_FLAGS(0)), key(VK_MASK, 0, KEYEVENTF_KEYUP)]);
}

fn is_down(vk: u32) -> bool {
    // SAFETY: plain query. The high bit is set while the key is down.
    unsafe { GetAsyncKeyState(vk as i32) < 0 }
}

/// Bits of the modifiers Windows reports as down.
fn held_modifiers() -> u8 {
    MODIFIER_KEYS.iter().enumerate().filter(|(_, (vk, _))| is_down(*vk)).fold(0, |bits, (bit, _)| bits | 1 << bit)
}

/// Waits until the user has let go of Ctrl, Alt, Shift and Win (they'd combine with the
/// synthesized shortcut: Win + Ctrl + V isn't a paste). Keys still held after `timeout` are
/// released synthetically.
pub fn wait_for_modifiers_released(timeout: Duration) {
    let deadline = Instant::now() + timeout;
    let held = loop {
        let held = held_modifiers();
        if held == 0 {
            return;
        }
        if Instant::now() >= deadline {
            break held;
        }
        thread::sleep(Duration::from_millis(4));
    };
    log::debug!("Modifiers still held after {timeout:?} (bits {held:#010b}); releasing them");
    let layout = foreground_layout();
    let mut inputs = Vec::new();
    if held & MENU_KEY_BITS != 0 {
        // So the synthetic Win/Alt release doesn't open the Start menu or the menu bar.
        inputs.push(key(VK_MASK, 0, KEYBD_EVENT_FLAGS(0)));
        inputs.push(key(VK_MASK, 0, KEYEVENTF_KEYUP));
    }
    for (bit, (vk, _)) in MODIFIER_KEYS.iter().enumerate() {
        let mask = 1u8 << bit;
        if held & mask == 0 {
            continue;
        }
        let mut flags = KEYEVENTF_KEYUP;
        if EXTENDED_MODIFIER_BITS & mask != 0 {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        inputs.push(key(*vk as u16, scan_code(*vk as u16, layout), flags));
    }
    // Before sending: the hook may see the user's next key right after the releases.
    FORCED_UP.fetch_or(held, Ordering::Relaxed);
    send(&inputs);
}

/// Presses Ctrl + `vk` (Ctrl + V to paste, Ctrl + C to copy) as one atomic batch, so the user's
/// own typing can't interleave with it.
pub fn send_ctrl_chord(vk: u16) -> bool {
    let layout = foreground_layout();
    let ctrl = scan_code(VK_CONTROL.0, layout);
    let letter = scan_code(vk, layout);
    send(&[
        key(VK_CONTROL.0, ctrl, KEYBD_EVENT_FLAGS(0)),
        key(vk, letter, KEYBD_EVENT_FLAGS(0)),
        key(vk, letter, KEYEVENTF_KEYUP),
        key(VK_CONTROL.0, ctrl, KEYEVENTF_KEYUP),
    ])
}

/// Types `text` as Unicode key events: each UTF-16 unit (so both halves of a surrogate pair)
/// becomes a `VK_PACKET` press. Line breaks and tabs are sent as the real keys, which apps
/// handle more consistently than the characters.
pub fn type_text(text: &str) -> bool {
    let layout = foreground_layout();
    let mut inputs = Vec::with_capacity(text.len() * 2);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let vk = match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                Some(VK_RETURN)
            }
            '\n' => Some(VK_RETURN),
            '\t' => Some(VK_TAB),
            _ => None,
        };
        if let Some(vk) = vk {
            let scan = scan_code(vk.0, layout);
            inputs.push(key(vk.0, scan, KEYBD_EVENT_FLAGS(0)));
            inputs.push(key(vk.0, scan, KEYEVENTF_KEYUP));
            continue;
        }
        let mut units = [0u16; 2];
        for &unit in c.encode_utf16(&mut units).iter() {
            inputs.push(key(0, unit, KEYEVENTF_UNICODE));
            inputs.push(key(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
        }
    }
    // Large batches are split so a single call stays reasonable; each batch is still atomic.
    inputs.chunks(2000).all(send)
}
