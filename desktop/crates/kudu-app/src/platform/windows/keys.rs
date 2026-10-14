//! Virtual-key codes: modifiers, labels for recorded shortcuts, and which keys type text.

use ::windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyNameTextW, GetKeyboardLayout, MapVirtualKeyExW, MAPVK_VK_TO_CHAR, VK_CONTROL, VK_LCONTROL, VK_LMENU,
    VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
};
use ::windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
use kudu_core::shortcut::{Modifier, ModifierKind, Side};

pub const VK_ESCAPE: u32 = 0x1B;
pub const VK_V: u32 = 0x56;
pub const VK_C: u32 = 0x43;

/// The eight physical modifier keys, indexed by their bit in a held-modifiers mask.
pub const MODIFIER_KEYS: [(u32, Modifier); 8] = [
    (VK_LCONTROL.0 as u32, Modifier::new(ModifierKind::Control, Side::Left)),
    (VK_RCONTROL.0 as u32, Modifier::new(ModifierKind::Control, Side::Right)),
    (VK_LMENU.0 as u32, Modifier::new(ModifierKind::Alt, Side::Left)),
    (VK_RMENU.0 as u32, Modifier::new(ModifierKind::Alt, Side::Right)),
    (VK_LSHIFT.0 as u32, Modifier::new(ModifierKind::Shift, Side::Left)),
    (VK_RSHIFT.0 as u32, Modifier::new(ModifierKind::Shift, Side::Right)),
    (VK_LWIN.0 as u32, Modifier::new(ModifierKind::Super, Side::Left)),
    (VK_RWIN.0 as u32, Modifier::new(ModifierKind::Super, Side::Right)),
];

/// Bits of the Win and Alt keys, whose lone release opens the Start menu / the menu bar.
pub const MENU_KEY_BITS: u8 = 0b1100_1100;

/// The modifier bit for a key event, or `None` for a regular key. The low-level hook reports
/// sided codes (VK_LCONTROL…); the generic ones (VK_CONTROL…) only come from other programs'
/// synthesized input and are resolved with the extended flag / scan code.
pub fn modifier_bit(vk: u32, scan: u32, extended: bool) -> Option<usize> {
    let vk16 = vk as u16;
    let bit = match vk16 {
        v if v == VK_LCONTROL.0 => 0,
        v if v == VK_RCONTROL.0 => 1,
        v if v == VK_LMENU.0 => 2,
        v if v == VK_RMENU.0 => 3,
        v if v == VK_LSHIFT.0 => 4,
        v if v == VK_RSHIFT.0 => 5,
        v if v == VK_LWIN.0 => 6,
        v if v == VK_RWIN.0 => 7,
        v if v == VK_CONTROL.0 => usize::from(extended),
        v if v == VK_MENU.0 => 2 + usize::from(extended),
        v if v == VK_SHIFT.0 => 4 + usize::from(scan & 0xFF == 0x36),
        _ => return None,
    };
    Some(bit)
}

/// Whether a key types text, or is needed while typing (Enter, Tab, Backspace, arrows…), so
/// holding it alone can't be a dictation shortcut. F-keys, Pause, Insert, Scroll Lock, Caps Lock
/// and media keys don't.
pub fn key_types_text(vk: u32) -> bool {
    matches!(
        vk,
        0x08 | 0x09 | 0x0D        // Backspace, Tab, Enter
        | 0x1B                    // Escape
        | 0x20                    // Space
        | 0x21..=0x28             // Page Up/Down, End, Home, arrows
        | 0x2E                    // Delete
        | 0x30..=0x39             // 0–9
        | 0x41..=0x5A             // A–Z
        | 0x60..=0x6F             // numpad digits and operators
        | 0xBA..=0xC0             // ; = , - . / `
        | 0xDB..=0xDF             // [ \ ] ' and OEM_8
        | 0xE2 // the extra \ key on ISO keyboards
    )
}

/// What a key is called in a recorded shortcut: "Space", "F9", "V", "Num 5", "/".
pub fn key_label(vk: u32, scan: u32, extended: bool) -> String {
    let named = match vk {
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0C => "Clear",
        0x0D => "Enter",
        0x13 => "Pause",
        0x14 => "Caps Lock",
        0x1B => "Esc",
        0x20 => "Space",
        0x21 => "Page Up",
        0x22 => "Page Down",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "Left",
        0x26 => "Up",
        0x27 => "Right",
        0x28 => "Down",
        0x2C => "Print Screen",
        0x2D => "Insert",
        0x2E => "Delete",
        0x5D => "Menu",
        0x6A => "Num *",
        0x6B => "Num +",
        0x6D => "Num -",
        0x6E => "Num .",
        0x6F => "Num /",
        0x90 => "Num Lock",
        0x91 => "Scroll Lock",
        0xAD => "Mute",
        0xAE => "Volume Down",
        0xAF => "Volume Up",
        0xB0 => "Next Track",
        0xB1 => "Previous Track",
        0xB2 => "Stop",
        0xB3 => "Play/Pause",
        _ => "",
    };
    if !named.is_empty() {
        return named.to_string();
    }
    match vk {
        0x30..=0x39 | 0x41..=0x5A => return char::from(vk as u8).to_string(),
        0x60..=0x69 => return format!("Num {}", vk - 0x60),
        0x70..=0x87 => return format!("F{}", vk - 0x6F),
        _ => {}
    }
    // Punctuation depends on the layout: ask what the key types in the foreground app's layout.
    // SAFETY: plain queries.
    let typed = unsafe {
        let layout = GetKeyboardLayout(GetWindowThreadProcessId(GetForegroundWindow(), None));
        MapVirtualKeyExW(vk, MAPVK_VK_TO_CHAR, Some(layout))
    };
    // The high bit marks a dead key (´ ` ^ …); the character is still the right label.
    if let Some(c) = char::from_u32(typed & 0x7FFF_FFFF).filter(|c| !c.is_control() && *c != '\0') {
        return c.to_uppercase().collect();
    }
    let mut buffer = [0u16; 64];
    let lparam = ((scan & 0xFF) << 16) | (u32::from(extended) << 24);
    // SAFETY: `buffer` is a valid, writable slice.
    let len = unsafe { GetKeyNameTextW(lparam as i32, &mut buffer) };
    if len > 0 {
        return String::from_utf16_lossy(&buffer[..len as usize]);
    }
    format!("Key {vk}")
}
