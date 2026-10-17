//! Customizable shortcuts and the state machine that recognizes them.
//!
//! The platform layer (a Windows keyboard hook, Linux evdev) turns raw key events into three
//! calls on [`Matcher`]: the set of modifier keys held changed, a regular key went down, or a
//! regular key went up. The matcher answers with what happened (dictation started, Command mode,
//! paste last…) and whether the event should be hidden from other apps.
//!
//! Shortcuts are either:
//! - **modifier-only**, held like a walkie-talkie button: `Ctrl + Win`, `Right Alt`, `fn`;
//! - **a key with modifiers**: `Alt + Space` (hold), `Ctrl + Alt + V` (press).
//!
//! Dictation and Command mode chord the way people expect: holding the dictation shortcut and
//! adding the extra modifier of the Command shortcut within a moment turns the hold into a
//! Command mode instruction; adding any other modifier means the user is typing a system
//! shortcut, so the hold is cancelled.

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Kinds of modifier keys. `Super` is the Windows key, the Linux Super key and ⌘ on a Mac.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ModifierKind {
    Fn,
    Control,
    Alt,
    Shift,
    Super,
}

/// Which physical key of a pair. Shortcuts usually accept either (`Any`); a single right-hand
/// modifier like `Right Alt` is specific.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Side {
    Any,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Modifier {
    pub kind: ModifierKind,
    pub side: Side,
}

impl Modifier {
    pub const fn new(kind: ModifierKind, side: Side) -> Self {
        Self { kind, side }
    }

    pub const fn any(kind: ModifierKind) -> Self {
        Self { kind, side: Side::Any }
    }

    /// Whether a physically held key satisfies this requirement.
    fn accepts(&self, held: &Modifier) -> bool {
        self.kind == held.kind && (self.side == Side::Any || self.side == held.side)
    }
}

/// A regular (non-modifier) key, by the platform's key code, with a label for display.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Key {
    /// Windows virtual-key code, Linux evdev key code, or macOS virtual key code.
    pub code: u32,
    /// What the key shows: "Space", "V", "F9".
    pub label: String,
}

/// Names for the keys of a platform, so shortcuts read the way that platform writes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notation {
    /// ⌃ ⌥ ⇧ ⌘ fn
    Mac,
    /// Ctrl, Alt, Shift, Win
    Windows,
    /// Ctrl, Alt, Shift, Super
    Linux,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Shortcut {
    /// Sorted, one entry per kind.
    pub modifiers: Vec<Modifier>,
    pub key: Option<Key>,
}

impl Shortcut {
    pub fn new(mut modifiers: Vec<Modifier>, key: Option<Key>) -> Self {
        modifiers.sort();
        modifiers.dedup_by_key(|m| m.kind);
        Self { modifiers, key }
    }

    pub fn modifiers(modifiers: &[Modifier]) -> Self {
        Self::new(modifiers.to_vec(), None)
    }

    pub fn is_modifier_only(&self) -> bool {
        self.key.is_none()
    }

    pub fn contains(&self, kind: ModifierKind) -> bool {
        self.modifiers.iter().any(|m| m.kind == kind)
    }

    /// Whether exactly these modifier kinds are held, each on an accepted side.
    pub fn modifiers_match(&self, held: &[Modifier]) -> bool {
        let held_kinds = kinds(held);
        let wanted_kinds: Vec<ModifierKind> = self.modifiers.iter().map(|m| m.kind).collect();
        held_kinds == wanted_kinds && self.modifiers.iter().all(|wanted| held.iter().any(|h| wanted.accepts(h)))
    }

    /// Whether every modifier of this shortcut is held (others may be held too).
    fn modifiers_held(&self, held: &[Modifier]) -> bool {
        self.modifiers.iter().all(|wanted| held.iter().any(|h| wanted.accepts(h)))
    }

    /// Whether this shortcut can be held to dictate without getting in the way of typing: it
    /// needs a modifier other than Shift, or a key that doesn't type (F-keys and the like).
    pub fn is_usable_for_holding(&self, key_types_text: impl Fn(u32) -> bool) -> bool {
        let has_real_modifier = self.modifiers.iter().any(|m| m.kind != ModifierKind::Shift);
        match &self.key {
            None => !self.modifiers.is_empty(),
            Some(key) => has_real_modifier || !key_types_text(key.code),
        }
    }

    /// "Ctrl + Win", "Right Alt", "⌃⌥Space", "fn".
    pub fn display(&self, notation: Notation) -> String {
        let mut parts: Vec<String> = self.modifiers.iter().map(|m| modifier_name(*m, notation, self.modifiers.len() == 1 && self.key.is_none())).collect();
        if let Some(key) = &self.key {
            parts.push(key.label.clone());
        }
        match notation {
            Notation::Mac => {
                // Symbols run together (⌃⌥Space); "fn" and "Right ⌥" stay as words.
                let words = self.modifiers.iter().any(|m| m.kind == ModifierKind::Fn || m.side == Side::Right);
                parts.join(if words { " " } else { "" })
            }
            Notation::Windows | Notation::Linux => parts.join(" + "),
        }
    }

    /// The parts as separate keycaps: ["Ctrl", "Win"].
    pub fn keycaps(&self, notation: Notation) -> Vec<String> {
        let single = self.modifiers.len() == 1 && self.key.is_none();
        let mut caps: Vec<String> = self.modifiers.iter().map(|m| modifier_name(*m, notation, single)).collect();
        if let Some(key) = &self.key {
            caps.push(key.label.clone());
        }
        caps
    }
}

fn kinds(held: &[Modifier]) -> Vec<ModifierKind> {
    let mut kinds: Vec<ModifierKind> = held.iter().map(|m| m.kind).collect();
    kinds.sort();
    kinds.dedup();
    kinds
}

fn modifier_name(modifier: Modifier, notation: Notation, alone: bool) -> String {
    let base = match (notation, modifier.kind) {
        (_, ModifierKind::Fn) => "fn",
        (Notation::Mac, ModifierKind::Control) => "⌃",
        (Notation::Mac, ModifierKind::Alt) => "⌥",
        (Notation::Mac, ModifierKind::Shift) => "⇧",
        (Notation::Mac, ModifierKind::Super) => "⌘",
        (_, ModifierKind::Control) => "Ctrl",
        (_, ModifierKind::Alt) => "Alt",
        (_, ModifierKind::Shift) => "Shift",
        (Notation::Windows, ModifierKind::Super) => "Win",
        (Notation::Linux, ModifierKind::Super) => "Super",
    };
    // The side matters to the user only when it was chosen on purpose (a lone right modifier,
    // or any modifier recorded as right-only).
    match modifier.side {
        Side::Right => format!("Right {base}"),
        Side::Left if alone => format!("Left {base}"),
        _ => base.to_string(),
    }
}

/// The shortcuts a user can change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shortcuts {
    /// Hold to dictate (tap or double-tap for hands-free, depending on the recording mode).
    pub dictate: Shortcut,
    /// Hold to speak an instruction for the selected text.
    pub command: Option<Shortcut>,
    /// Press to paste the last transcript again.
    pub paste_last: Option<Shortcut>,
    /// Press to start or stop taking meeting notes. Settings saved before meeting notes
    /// existed get the default; `null` means switched off.
    #[serde(default = "Shortcuts::default_meeting")]
    pub meeting: Option<Shortcut>,
}

impl Shortcuts {
    /// Ctrl + Win to dictate, Ctrl + Win + Alt for Command mode, Ctrl + Alt + V to paste the
    /// last transcript. `v_code` is the platform's key code for V.
    pub fn default_pc(v_code: u32) -> Self {
        use ModifierKind::*;
        Self {
            dictate: Shortcut::modifiers(&[Modifier::any(Control), Modifier::any(Super)]),
            command: Some(Shortcut::modifiers(&[Modifier::any(Control), Modifier::any(Super), Modifier::any(Alt)])),
            paste_last: Some(Shortcut::new(
                vec![Modifier::any(Control), Modifier::any(Alt)],
                Some(Key { code: v_code, label: "V".into() }),
            )),
            meeting: Self::default_meeting(),
        }
    }

    /// Ctrl + Alt + N: start or stop meeting notes (N is virtual key 0x4E on Windows, evdev
    /// key 49 on Linux).
    pub fn default_meeting() -> Option<Shortcut> {
        let n_code = if cfg!(windows) { 0x4E } else { 49 };
        Some(Shortcut::new(
            vec![Modifier::any(ModifierKind::Control), Modifier::any(ModifierKind::Alt)],
            Some(Key { code: n_code, label: "N".into() }),
        ))
    }

    /// Problems that make a set of shortcuts unusable, in words for the settings page.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.dictate.modifiers.is_empty() && self.dictate.key.is_none() {
            problems.push("The dictation shortcut is empty.".to_string());
        }
        if let Some(command) = &self.command {
            if *command == self.dictate {
                problems.push("Command mode needs a different shortcut from dictation.".to_string());
            }
        }
        if let Some(paste) = &self.paste_last {
            if *paste == self.dictate || Some(paste) == self.command.as_ref() {
                problems.push("Paste last transcript needs its own shortcut.".to_string());
            }
        }
        if let Some(meeting) = &self.meeting {
            if *meeting == self.dictate || Some(meeting) == self.command.as_ref() || Some(meeting) == self.paste_last.as_ref() {
                problems.push("Meeting notes needs its own shortcut.".to_string());
            }
        }
        problems
    }
}

/// What the user did with the shortcuts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyEvent {
    /// The dictation shortcut went down.
    Pressed,
    /// The dictation shortcut was released.
    Released,
    /// The hold turned out to be part of another shortcut, or Escape was pressed.
    Cancelled,
    /// The Command mode shortcut went down (or the dictation hold became one).
    CommandPressed,
    /// The Command mode shortcut was released.
    CommandReleased,
    /// The paste-last shortcut was pressed.
    PasteLast,
    /// The meeting notes shortcut was pressed.
    Meeting,
}

/// The matcher's answer to one key event.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub events: Vec<HotkeyEvent>,
    /// Hide the event from other apps (it belongs to a Kudu shortcut).
    pub swallow: bool,
}

impl Outcome {
    fn pass() -> Self {
        Self::default()
    }

    fn swallow(events: Vec<HotkeyEvent>) -> Self {
        Self { events, swallow: true }
    }

    fn events(events: Vec<HotkeyEvent>) -> Self {
        Self { events, swallow: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Active {
    Dictate,
    Command,
}

/// Recognizes [`Shortcuts`] from a stream of key events. Not tied to any platform: feed it the
/// modifiers held (with their sides) and regular key presses.
#[derive(Clone, Debug)]
pub struct Matcher {
    pub shortcuts: Shortcuts,
    /// Platform key code for Escape, which cancels a dictation.
    pub escape_code: u32,
    /// A modifier joining the dictation hold within this window makes a chord.
    pub chord_window: Duration,
    /// Set by the app while a dictation is in progress, so Escape can cancel it.
    pub is_dictating: bool,
    held: Vec<Modifier>,
    active: Option<Active>,
    active_since: Option<Instant>,
    /// After a cancelled hold, nothing matches until every modifier is released, so letting go
    /// of the extra key doesn't start a dictation.
    blocked: bool,
}

impl Matcher {
    pub fn new(shortcuts: Shortcuts, escape_code: u32) -> Self {
        Self {
            shortcuts,
            escape_code,
            chord_window: Duration::from_millis(300),
            is_dictating: false,
            held: Vec::new(),
            active: None,
            active_since: None,
            blocked: false,
        }
    }

    /// Replaces the shortcuts (from settings) and forgets any hold in progress.
    pub fn set_shortcuts(&mut self, shortcuts: Shortcuts) {
        self.shortcuts = shortcuts;
        self.active = None;
        self.blocked = !self.held.is_empty();
    }

    pub fn held(&self) -> &[Modifier] {
        &self.held
    }

    /// The set of physical modifier keys held changed. `held` lists each key with its side
    /// (`Left` or `Right`; `fn` uses `Any`).
    pub fn modifiers_changed(&mut self, held: &[Modifier], now: Instant) -> Outcome {
        self.held = held.to_vec();
        if held.is_empty() {
            self.blocked = false;
        }
        let in_chord_window = self.active_since.is_some_and(|since| now.duration_since(since) < self.chord_window);
        let dictate = self.shortcuts.dictate.clone();
        let command = self.shortcuts.command.clone();

        match self.active {
            None => {
                if self.blocked {
                    return Outcome::pass();
                }
                if command.as_ref().is_some_and(|c| c.is_modifier_only() && c.modifiers_match(held)) {
                    return self.activate(Active::Command, now, HotkeyEvent::CommandPressed);
                }
                if dictate.is_modifier_only() && dictate.modifiers_match(held) {
                    // If the Command shortcut extends this one, the hold becomes Command mode
                    // when its extra modifier joins.
                    return self.activate(Active::Dictate, now, HotkeyEvent::Pressed);
                }
                Outcome::pass()
            }
            Some(Active::Dictate) if dictate.is_modifier_only() => {
                if !dictate.modifiers_held(held) {
                    self.finish();
                    return Outcome::events(vec![HotkeyEvent::Released]);
                }
                if held.len() > dictate.modifiers.len() || !dictate.modifiers_match(held) {
                    // Another modifier joined the hold.
                    if in_chord_window && command.as_ref().is_some_and(|c| c.is_modifier_only() && c.modifiers_match(held)) {
                        self.active = Some(Active::Command);
                        return Outcome::events(vec![HotkeyEvent::CommandPressed]);
                    }
                    if in_chord_window {
                        // A system shortcut that starts with the same keys (Ctrl + Win + D…).
                        self.finish();
                        self.blocked = true;
                        return Outcome::events(vec![HotkeyEvent::Cancelled]);
                    }
                }
                Outcome::pass()
            }
            Some(Active::Command) if command.as_ref().is_some_and(|c| c.is_modifier_only()) => {
                let command = command.expect("checked above");
                // Ends once the user lets go of the keys: when none of the Command modifiers
                // that aren't also part of dictation are held, or the dictation part is released.
                let still_held = command.modifiers.iter().any(|wanted| held.iter().any(|h| wanted.accepts(h)));
                let base_released = dictate.is_modifier_only()
                    && dictate.modifiers.iter().all(|m| command.modifiers.contains(m))
                    && !dictate.modifiers_held(held);
                if !still_held || base_released {
                    self.finish();
                    self.blocked = !held.is_empty();
                    return Outcome::events(vec![HotkeyEvent::CommandReleased]);
                }
                Outcome::pass()
            }
            // A key-based shortcut is active: modifiers changing doesn't end it; its key-up does.
            Some(_) => Outcome::pass(),
        }
    }

    /// A regular key went down. `is_repeat` is true for auto-repeat while held.
    pub fn key_down(&mut self, code: u32, is_repeat: bool, now: Instant) -> Outcome {
        let held = self.held.clone();

        // Key-based dictation or Command shortcuts.
        if let Some(active) = self.active {
            let shortcut = self.shortcut(active);
            if shortcut.key.as_ref().is_some_and(|k| k.code == code) {
                return Outcome::swallow(vec![]); // auto-repeat of the held key
            }
        }

        if code == self.escape_code && (self.is_dictating || self.active.is_some()) {
            self.finish();
            self.blocked = !held.is_empty();
            return Outcome::swallow(vec![HotkeyEvent::Cancelled]);
        }

        if self.active.is_none() && !is_repeat {
            if let Some(paste) = &self.shortcuts.paste_last {
                if paste.key.as_ref().is_some_and(|k| k.code == code) && paste.modifiers_match(&held) {
                    return Outcome::swallow(vec![HotkeyEvent::PasteLast]);
                }
            }
            if let Some(meeting) = &self.shortcuts.meeting {
                if meeting.key.as_ref().is_some_and(|k| k.code == code) && meeting.modifiers_match(&held) {
                    return Outcome::swallow(vec![HotkeyEvent::Meeting]);
                }
            }
            if let Some(command) = &self.shortcuts.command {
                if command.key.as_ref().is_some_and(|k| k.code == code) && command.modifiers_match(&held) {
                    return self.activate_swallowing(Active::Command, now, HotkeyEvent::CommandPressed);
                }
            }
            let dictate = &self.shortcuts.dictate;
            if dictate.key.as_ref().is_some_and(|k| k.code == code) && dictate.modifiers_match(&held) {
                return self.activate_swallowing(Active::Dictate, now, HotkeyEvent::Pressed);
            }
        }

        // Any other key while a modifier-only shortcut is held: the user is typing a shortcut.
        if self.active.is_some() {
            self.finish();
            self.blocked = true;
            return Outcome::events(vec![HotkeyEvent::Cancelled]);
        }
        Outcome::pass()
    }

    /// A regular key went up.
    pub fn key_up(&mut self, code: u32, _now: Instant) -> Outcome {
        let Some(active) = self.active else { return Outcome::pass() };
        let shortcut = self.shortcut(active);
        if shortcut.key.as_ref().is_some_and(|k| k.code == code) {
            self.finish();
            let event = match active {
                Active::Dictate => HotkeyEvent::Released,
                Active::Command => HotkeyEvent::CommandReleased,
            };
            return Outcome::swallow(vec![event]);
        }
        Outcome::pass()
    }

    /// Whether a shortcut is being held right now (so the platform can keep hiding its keys,
    /// e.g. stop the Start menu from opening when Win is released).
    pub fn is_holding(&self) -> bool {
        self.active.is_some()
    }

    fn shortcut(&self, active: Active) -> Shortcut {
        match active {
            Active::Dictate => self.shortcuts.dictate.clone(),
            Active::Command => self.shortcuts.command.clone().unwrap_or_else(|| self.shortcuts.dictate.clone()),
        }
    }

    fn activate(&mut self, active: Active, now: Instant, event: HotkeyEvent) -> Outcome {
        self.active = Some(active);
        self.active_since = Some(now);
        Outcome::events(vec![event])
    }

    fn activate_swallowing(&mut self, active: Active, now: Instant, event: HotkeyEvent) -> Outcome {
        let mut outcome = self.activate(active, now, event);
        outcome.swallow = true;
        outcome
    }

    fn finish(&mut self) {
        self.active = None;
        self.active_since = None;
    }
}

/// Records a new shortcut from what the user presses in the settings page: the largest set of
/// modifiers held, plus the first regular key. Finishes when a key is pressed or when every
/// modifier is released.
#[derive(Clone, Debug, Default)]
pub struct Recorder {
    most: Vec<Modifier>,
    held: Vec<Modifier>,
}

/// What recording a shortcut produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recorded {
    /// Still waiting for the user to finish.
    Pending,
    /// The user pressed Escape on its own.
    Cancelled,
    Done(Shortcut),
}

impl Recorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// The modifiers held changed. Physical sides are kept as `Right` only when the user used
    /// right-hand keys exclusively for a lone modifier; otherwise either side will do.
    pub fn modifiers_changed(&mut self, held: &[Modifier]) -> Recorded {
        self.held = held.to_vec();
        if held.len() >= self.most.len() {
            self.most = held.to_vec();
        }
        if held.is_empty() && !self.most.is_empty() {
            return Recorded::Done(Shortcut::new(Self::generalize(&self.most), None));
        }
        Recorded::Pending
    }

    pub fn key_down(&mut self, key: Key, escape_code: u32) -> Recorded {
        if key.code == escape_code && self.held.is_empty() {
            return Recorded::Cancelled;
        }
        Recorded::Done(Shortcut::new(Self::generalize(&self.held), Some(key)))
    }

    /// Shows what's held so far while recording: ["Ctrl", "Win"].
    pub fn preview(&self) -> Vec<Modifier> {
        Self::generalize(if self.held.is_empty() { &self.most } else { &self.held })
    }

    fn generalize(held: &[Modifier]) -> Vec<Modifier> {
        if held.len() == 1 && held[0].side == Side::Right {
            return held.to_vec(); // "Right Alt" on its own is a deliberate choice
        }
        let mut result: Vec<Modifier> = held.iter().map(|m| Modifier::any(m.kind)).collect();
        result.sort();
        result.dedup();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ModifierKind::*;

    const ESC: u32 = 27;
    const V: u32 = 86;
    const SPACE: u32 = 32;
    const D: u32 = 68;

    fn l(kind: ModifierKind) -> Modifier {
        Modifier::new(kind, Side::Left)
    }
    fn r(kind: ModifierKind) -> Modifier {
        Modifier::new(kind, Side::Right)
    }

    fn matcher() -> (Matcher, Instant) {
        (Matcher::new(Shortcuts::default_pc(V), ESC), Instant::now())
    }

    fn ms(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    #[test]
    fn holding_the_dictation_modifiers_starts_and_releasing_stops() {
        let (mut m, t) = matcher();
        assert!(m.modifiers_changed(&[l(Control)], t).events.is_empty());
        assert_eq!(m.modifiers_changed(&[l(Control), l(Super)], ms(t, 20)).events, vec![HotkeyEvent::Pressed]);
        assert_eq!(m.modifiers_changed(&[l(Control)], ms(t, 2000)).events, vec![HotkeyEvent::Released]);
        assert!(m.modifiers_changed(&[], ms(t, 2010)).events.is_empty());
    }

    #[test]
    fn adding_the_command_modifier_quickly_turns_the_hold_into_command_mode() {
        let (mut m, t) = matcher();
        m.modifiers_changed(&[l(Control), l(Super)], t);
        assert_eq!(
            m.modifiers_changed(&[l(Control), l(Super), l(Alt)], ms(t, 120)).events,
            vec![HotkeyEvent::CommandPressed]
        );
        // Letting go of Alt first doesn't end it while Ctrl + Win are still held…
        assert!(m.modifiers_changed(&[l(Control), l(Super)], ms(t, 900)).events.is_empty());
        // …releasing the base does.
        assert_eq!(m.modifiers_changed(&[l(Control)], ms(t, 950)).events, vec![HotkeyEvent::CommandReleased]);
        assert!(m.modifiers_changed(&[], ms(t, 960)).events.is_empty());
    }

    #[test]
    fn pressing_all_command_modifiers_at_once_is_command_mode() {
        let (mut m, t) = matcher();
        assert_eq!(
            m.modifiers_changed(&[l(Control), l(Super), r(Alt)], t).events,
            vec![HotkeyEvent::CommandPressed]
        );
    }

    #[test]
    fn another_modifier_joining_quickly_is_a_system_shortcut() {
        let (mut m, t) = matcher();
        m.modifiers_changed(&[l(Control), l(Super)], t);
        assert_eq!(
            m.modifiers_changed(&[l(Control), l(Super), l(Shift)], ms(t, 80)).events,
            vec![HotkeyEvent::Cancelled]
        );
        // Releasing Shift must not start a new dictation.
        assert!(m.modifiers_changed(&[l(Control), l(Super)], ms(t, 200)).events.is_empty());
        assert!(m.modifiers_changed(&[], ms(t, 300)).events.is_empty());
        assert_eq!(m.modifiers_changed(&[l(Control), l(Super)], ms(t, 400)).events, vec![HotkeyEvent::Pressed]);
    }

    #[test]
    fn a_regular_key_during_the_hold_cancels_it() {
        let (mut m, t) = matcher();
        m.modifiers_changed(&[l(Control), l(Super)], t);
        let outcome = m.key_down(D, false, ms(t, 50));
        assert_eq!(outcome.events, vec![HotkeyEvent::Cancelled]);
        assert!(!outcome.swallow, "Ctrl + Win + D still reaches the system");
    }

    #[test]
    fn escape_cancels_a_dictation_and_is_hidden() {
        let (mut m, t) = matcher();
        m.is_dictating = true;
        let outcome = m.key_down(ESC, false, t);
        assert_eq!(outcome.events, vec![HotkeyEvent::Cancelled]);
        assert!(outcome.swallow);
        m.is_dictating = false;
        assert_eq!(m.key_down(ESC, false, t), Outcome::default());
    }

    #[test]
    fn paste_last_is_a_pressed_shortcut() {
        let (mut m, t) = matcher();
        m.modifiers_changed(&[l(Control), r(Alt)], t);
        let outcome = m.key_down(V, false, t);
        assert_eq!(outcome.events, vec![HotkeyEvent::PasteLast]);
        assert!(outcome.swallow);
        // Plain V types normally.
        m.modifiers_changed(&[], t);
        assert_eq!(m.key_down(V, false, t), Outcome::default());
    }

    #[test]
    fn meeting_is_a_pressed_shortcut() {
        let (mut m, t) = matcher();
        let n = Shortcuts::default_meeting().unwrap().key.unwrap().code;
        m.modifiers_changed(&[l(Control), l(Alt)], t);
        assert_eq!(m.key_down(n, false, t), Outcome::swallow(vec![HotkeyEvent::Meeting]));
        let mut off = m.shortcuts.clone();
        off.meeting = None;
        m.set_shortcuts(off);
        m.modifiers_changed(&[], t);
        m.modifiers_changed(&[l(Control), l(Alt)], t);
        assert_eq!(m.key_down(n, false, t), Outcome::default());
    }

    #[test]
    fn settings_from_before_meeting_notes_get_its_default_but_keep_it_off() {
        let mut saved = serde_json::to_value(Shortcuts::default_pc(V)).unwrap();
        saved.as_object_mut().unwrap().remove("meeting");
        let old: Shortcuts = serde_json::from_value(saved).unwrap();
        assert_eq!(old.meeting, Shortcuts::default_meeting());
        let mut off = Shortcuts::default_pc(V);
        off.meeting = None;
        let back: Shortcuts = serde_json::from_str(&serde_json::to_string(&off).unwrap()).unwrap();
        assert_eq!(back.meeting, None);
    }

    #[test]
    fn a_key_based_dictation_shortcut_holds_and_hides_its_key() {
        let shortcuts = Shortcuts {
            dictate: Shortcut::new(vec![Modifier::any(Alt)], Some(Key { code: SPACE, label: "Space".into() })),
            command: None,
            paste_last: None,
            meeting: None,
        };
        let mut m = Matcher::new(shortcuts, ESC);
        let t = Instant::now();
        m.modifiers_changed(&[l(Alt)], t);
        assert_eq!(m.key_down(SPACE, false, t), Outcome::swallow(vec![HotkeyEvent::Pressed]));
        assert_eq!(m.key_down(SPACE, true, ms(t, 500)), Outcome::swallow(vec![]));
        // Releasing Alt early doesn't stop it; releasing Space does.
        assert!(m.modifiers_changed(&[], ms(t, 900)).events.is_empty());
        assert_eq!(m.key_up(SPACE, ms(t, 1000)), Outcome::swallow(vec![HotkeyEvent::Released]));
    }

    #[test]
    fn a_right_only_shortcut_ignores_the_left_key() {
        let shortcuts = Shortcuts { dictate: Shortcut::modifiers(&[r(Alt)]), command: None, paste_last: None, meeting: None };
        let mut m = Matcher::new(shortcuts, ESC);
        let t = Instant::now();
        assert!(m.modifiers_changed(&[l(Alt)], t).events.is_empty());
        m.modifiers_changed(&[], t);
        assert_eq!(m.modifiers_changed(&[r(Alt)], t).events, vec![HotkeyEvent::Pressed]);
    }

    #[test]
    fn records_modifier_only_and_key_shortcuts() {
        let mut recorder = Recorder::new();
        assert_eq!(recorder.modifiers_changed(&[l(Control)]), Recorded::Pending);
        assert_eq!(recorder.modifiers_changed(&[l(Control), l(Super)]), Recorded::Pending);
        assert_eq!(recorder.modifiers_changed(&[l(Super)]), Recorded::Pending);
        assert_eq!(
            recorder.modifiers_changed(&[]),
            Recorded::Done(Shortcut::modifiers(&[Modifier::any(Control), Modifier::any(Super)]))
        );

        let mut recorder = Recorder::new();
        recorder.modifiers_changed(&[r(Alt)]);
        assert_eq!(recorder.modifiers_changed(&[]), Recorded::Done(Shortcut::modifiers(&[r(Alt)])));

        let mut recorder = Recorder::new();
        recorder.modifiers_changed(&[l(Alt)]);
        assert_eq!(
            recorder.key_down(Key { code: SPACE, label: "Space".into() }, ESC),
            Recorded::Done(Shortcut::new(vec![Modifier::any(Alt)], Some(Key { code: SPACE, label: "Space".into() })))
        );

        let mut recorder = Recorder::new();
        assert_eq!(recorder.key_down(Key { code: ESC, label: "Esc".into() }, ESC), Recorded::Cancelled);
    }

    #[test]
    fn displays_the_platform_way() {
        let shortcuts = Shortcuts::default_pc(V);
        assert_eq!(shortcuts.dictate.display(Notation::Windows), "Ctrl + Win");
        assert_eq!(shortcuts.dictate.display(Notation::Linux), "Ctrl + Super");
        assert_eq!(shortcuts.paste_last.as_ref().unwrap().display(Notation::Windows), "Ctrl + Alt + V");
        assert_eq!(Shortcut::modifiers(&[r(Alt)]).display(Notation::Windows), "Right Alt");
        assert_eq!(Shortcut::modifiers(&[r(Alt)]).display(Notation::Mac), "Right ⌥");
        assert_eq!(shortcuts.dictate.display(Notation::Mac), "⌃⌘");
    }

    #[test]
    fn rejects_shortcuts_that_would_get_in_the_way() {
        let types_text = |code: u32| code != 120; // 120 = F9 on Windows
        assert!(!Shortcut::new(vec![], Some(Key { code: V, label: "V".into() })).is_usable_for_holding(types_text));
        assert!(!Shortcut::new(vec![Modifier::any(Shift)], Some(Key { code: V, label: "V".into() })).is_usable_for_holding(types_text));
        assert!(Shortcut::new(vec![], Some(Key { code: 120, label: "F9".into() })).is_usable_for_holding(types_text));
        let mut same = Shortcuts::default_pc(V);
        same.command = Some(same.dictate.clone());
        assert_eq!(same.problems().len(), 1);
    }
}
