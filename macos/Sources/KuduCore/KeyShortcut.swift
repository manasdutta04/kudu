import Foundation

/// Customizable shortcuts and the state machine that recognizes them. The same design runs on
/// Windows and Linux (Rust, `kudu-core/src/shortcut.rs`), so shortcuts behave alike everywhere.
///
/// Shortcuts are either modifier-only, held like a walkie-talkie button (fn, Right ⌥, ⌃⌥), or a
/// key with modifiers (⌥Space held, ⌃⌘V pressed). Holding the dictation shortcut and adding the
/// extra modifier of the Command shortcut within a moment turns the hold into a Command mode
/// instruction; adding any other modifier means a system shortcut, so the hold is cancelled.

public enum ModifierKind: String, Codable, Sendable, CaseIterable, Comparable {
    case fn, control, option, shift, command

    public static func < (lhs: Self, rhs: Self) -> Bool {
        allCases.firstIndex(of: lhs)! < allCases.firstIndex(of: rhs)!
    }

    var symbol: String {
        switch self {
        case .fn: "fn"
        case .control: "⌃"
        case .option: "⌥"
        case .shift: "⇧"
        case .command: "⌘"
        }
    }
}

/// Which key of a pair. Shortcuts usually accept either; a lone right-hand modifier
/// ("Right ⌥") is specific.
public enum KeySide: String, Codable, Sendable {
    case any, left, right
}

public struct ModifierKey: Codable, Sendable, Hashable, Comparable {
    public var kind: ModifierKind
    public var side: KeySide

    public init(_ kind: ModifierKind, side: KeySide = .any) {
        self.kind = kind
        self.side = side
    }

    public static func < (lhs: Self, rhs: Self) -> Bool { lhs.kind < rhs.kind }

    /// Whether a physically held key satisfies this requirement.
    func accepts(_ held: ModifierKey) -> Bool {
        kind == held.kind && (side == .any || side == held.side)
    }
}

public struct KeyShortcut: Codable, Sendable, Hashable {
    /// Sorted, one per kind.
    public var modifiers: [ModifierKey]
    /// A regular key (macOS virtual key code), if any.
    public var keyCode: Int?
    /// What that key shows ("Space", "V").
    public var keyLabel: String?

    public init(_ modifiers: [ModifierKey], keyCode: Int? = nil, keyLabel: String? = nil) {
        var seen = Set<ModifierKind>()
        self.modifiers = modifiers.sorted().filter { seen.insert($0.kind).inserted }
        self.keyCode = keyCode
        self.keyLabel = keyLabel
    }

    public var isModifierOnly: Bool { keyCode == nil }

    public func contains(_ kind: ModifierKind) -> Bool { modifiers.contains { $0.kind == kind } }

    /// Exactly these modifier kinds are held, each on an accepted side.
    func modifiersMatch(_ held: [ModifierKey]) -> Bool {
        Set(held.map(\.kind)) == Set(modifiers.map(\.kind)) && modifiersHeld(held)
    }

    /// Every modifier of this shortcut is held (others may be too).
    func modifiersHeld(_ held: [ModifierKey]) -> Bool {
        modifiers.allSatisfy { wanted in held.contains { wanted.accepts($0) } }
    }

    /// Can be held to dictate without getting in the way of typing: needs a modifier other
    /// than ⇧, or a key that doesn't type (F-keys and the like).
    public func isUsableForHolding(keyTypesText: (Int) -> Bool) -> Bool {
        let realModifier = modifiers.contains { $0.kind != .shift }
        guard let keyCode else { return !modifiers.isEmpty }
        return realModifier || !keyTypesText(keyCode)
    }

    /// The parts as keycaps: ["fn", "⌃"], ["Right ⌥"], ["⌃", "⌘", "V"].
    public var keycaps: [String] {
        let alone = modifiers.count == 1 && keyCode == nil
        var caps = modifiers.map { modifier -> String in
            switch modifier.side {
            case .right: "Right \(modifier.kind.symbol)"
            case .left where alone: "Left \(modifier.kind.symbol)"
            default: modifier.kind.symbol
            }
        }
        if let keyLabel { caps.append(keyLabel) }
        return caps
    }

    /// "fn", "Right ⌥", "⌃⌘V", "fn ⌃".
    public var displayName: String {
        let words = modifiers.contains { $0.kind == .fn || $0.side != .any }
        return keycaps.joined(separator: words ? " " : "")
    }

    // Presets.
    public static let fn = KeyShortcut([ModifierKey(.fn)])
    public static let rightCommand = KeyShortcut([ModifierKey(.command, side: .right)])
    public static let rightOption = KeyShortcut([ModifierKey(.option, side: .right)])
}

/// The shortcuts a user can change.
public struct ShortcutSet: Codable, Sendable, Equatable {
    /// Hold to dictate (tap or double-tap for hands-free, depending on the recording mode).
    public var dictate: KeyShortcut
    /// Hold to speak an instruction for the selected text.
    public var command: KeyShortcut?
    /// Press to paste the last transcript again.
    public var pasteLast: KeyShortcut?
    /// Press to start or stop taking meeting notes.
    public var meeting: KeyShortcut?

    public init(dictate: KeyShortcut, command: KeyShortcut?, pasteLast: KeyShortcut?, meeting: KeyShortcut? = ShortcutSet.defaultMeeting) {
        self.dictate = dictate
        self.command = command
        self.pasteLast = pasteLast
        self.meeting = meeting
    }

    /// ⌃⌥N: start or stop meeting notes.
    public static let defaultMeeting = KeyShortcut([ModifierKey(.control), ModifierKey(.option)], keyCode: 45, keyLabel: "N")

    /// fn to dictate, fn + ⌃ for Command mode, ⌃⌘V to paste the last transcript, ⌃⌥N for
    /// meeting notes.
    public static let defaults = ShortcutSet(
        dictate: .fn,
        command: KeyShortcut([ModifierKey(.fn), ModifierKey(.control)]),
        pasteLast: KeyShortcut([ModifierKey(.control), ModifierKey(.command)], keyCode: 9, keyLabel: "V"),
        meeting: defaultMeeting
    )

    private enum CodingKeys: String, CodingKey { case dictate, command, pasteLast, meeting }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        dictate = try container.decode(KeyShortcut.self, forKey: .dictate)
        command = try container.decodeIfPresent(KeyShortcut.self, forKey: .command)
        pasteLast = try container.decodeIfPresent(KeyShortcut.self, forKey: .pasteLast)
        // Saved before meeting notes existed: use the default. Saved as null: switched off.
        meeting = container.contains(.meeting)
            ? try container.decodeIfPresent(KeyShortcut.self, forKey: .meeting)
            : Self.defaultMeeting
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(dictate, forKey: .dictate)
        try container.encodeIfPresent(command, forKey: .command)
        try container.encodeIfPresent(pasteLast, forKey: .pasteLast)
        try container.encode(meeting, forKey: .meeting)
    }

    /// Whether fn is part of a hold shortcut (so fn presses must be hidden from the system).
    public var usesFn: Bool { dictate.contains(.fn) || command?.contains(.fn) == true }

    /// Problems that make these shortcuts unusable, in words for Settings.
    public var problems: [String] {
        var problems: [String] = []
        if dictate.modifiers.isEmpty && dictate.keyCode == nil { problems.append("The dictation shortcut is empty.") }
        if command == dictate { problems.append("Command mode needs a different shortcut from dictation.") }
        if let pasteLast, pasteLast == dictate || pasteLast == command {
            problems.append("Paste last transcript needs its own shortcut.")
        }
        if let meeting, meeting == dictate || meeting == command || meeting == pasteLast {
            problems.append("Meeting notes needs its own shortcut.")
        }
        return problems
    }
}

public enum ShortcutEvent: Sendable, Equatable {
    case pressed, released, cancelled, commandPressed, commandReleased, pasteLast, meeting
}

/// The matcher's answer to one key event.
public struct ShortcutOutcome: Sendable, Equatable {
    public var events: [ShortcutEvent] = []
    /// Hide the event from other apps (it belongs to a Kudu shortcut).
    public var swallow = false

    static let pass = ShortcutOutcome()
}

/// Recognizes a `ShortcutSet` from key events: the modifier keys held (with sides), and
/// regular key presses. Pure state, no system calls, so it's unit-tested.
public struct ShortcutMatcher: Sendable {
    public var shortcuts: ShortcutSet {
        didSet {
            active = nil
            blocked = !held.isEmpty
        }
    }
    /// Set while a dictation is in progress, so Escape can cancel it.
    public var isDictating = false
    public var chordWindow: TimeInterval = 0.3
    public static let escapeKeyCode = 53

    private enum Active { case dictate, command }
    private var held: [ModifierKey] = []
    private var active: Active?
    private var activeSince: TimeInterval?
    /// After a cancelled hold, nothing matches until every modifier is released.
    private var blocked = false

    public init(shortcuts: ShortcutSet) {
        self.shortcuts = shortcuts
    }

    public var isHolding: Bool { active != nil }

    public mutating func modifiersChanged(_ held: [ModifierKey], at now: TimeInterval) -> ShortcutOutcome {
        self.held = held
        if held.isEmpty { blocked = false }
        let inChordWindow = activeSince.map { now - $0 < chordWindow } ?? false
        let dictate = shortcuts.dictate
        let command = shortcuts.command
        let commandMatches = command.map { $0.isModifierOnly && $0.modifiersMatch(held) } ?? false

        switch active {
        case nil:
            guard !blocked else { return .pass }
            if commandMatches { return activate(.command, at: now, .commandPressed) }
            if dictate.isModifierOnly && dictate.modifiersMatch(held) { return activate(.dictate, at: now, .pressed) }
            return .pass

        case .dictate where dictate.isModifierOnly:
            if !dictate.modifiersHeld(held) {
                finish()
                return ShortcutOutcome(events: [.released])
            }
            if held.count > dictate.modifiers.count || !dictate.modifiersMatch(held) {
                if inChordWindow && commandMatches {
                    active = .command
                    return ShortcutOutcome(events: [.commandPressed])
                }
                if inChordWindow {
                    // A system shortcut that starts with the same keys.
                    finish()
                    blocked = true
                    return ShortcutOutcome(events: [.cancelled])
                }
            }
            return .pass

        case .command where command?.isModifierOnly == true:
            guard let command else { return .pass }
            // Ends when the user lets go: none of its keys held, or the dictation part released.
            let stillHeld = command.modifiers.contains { wanted in held.contains { wanted.accepts($0) } }
            let baseReleased = dictate.isModifierOnly
                && dictate.modifiers.allSatisfy { command.modifiers.contains($0) }
                && !dictate.modifiersHeld(held)
            if !stillHeld || baseReleased {
                finish()
                blocked = !held.isEmpty
                return ShortcutOutcome(events: [.commandReleased])
            }
            return .pass

        default:
            // A key-based shortcut is active: its key-up ends it.
            return .pass
        }
    }

    public mutating func keyDown(_ code: Int, isRepeat: Bool, at now: TimeInterval) -> ShortcutOutcome {
        if let active, shortcut(active).keyCode == code {
            return ShortcutOutcome(swallow: true) // auto-repeat of the held key
        }
        if code == Self.escapeKeyCode, isDictating || active != nil {
            finish()
            blocked = !held.isEmpty
            return ShortcutOutcome(events: [.cancelled], swallow: true)
        }
        if active == nil, !isRepeat {
            if let paste = shortcuts.pasteLast, paste.keyCode == code, paste.modifiersMatch(held) {
                return ShortcutOutcome(events: [.pasteLast], swallow: true)
            }
            if let meeting = shortcuts.meeting, meeting.keyCode == code, meeting.modifiersMatch(held) {
                return ShortcutOutcome(events: [.meeting], swallow: true)
            }
            if let command = shortcuts.command, command.keyCode == code, command.modifiersMatch(held) {
                var outcome = activate(.command, at: now, .commandPressed)
                outcome.swallow = true
                return outcome
            }
            if shortcuts.dictate.keyCode == code, shortcuts.dictate.modifiersMatch(held) {
                var outcome = activate(.dictate, at: now, .pressed)
                outcome.swallow = true
                return outcome
            }
        }
        // Any other key during a hold: the user is typing a shortcut.
        if active != nil {
            finish()
            blocked = true
            return ShortcutOutcome(events: [.cancelled])
        }
        return .pass
    }

    public mutating func keyUp(_ code: Int, at now: TimeInterval) -> ShortcutOutcome {
        guard let active, shortcut(active).keyCode == code else { return .pass }
        finish()
        return ShortcutOutcome(events: [active == .dictate ? .released : .commandReleased], swallow: true)
    }

    private func shortcut(_ active: Active) -> KeyShortcut {
        active == .dictate ? shortcuts.dictate : (shortcuts.command ?? shortcuts.dictate)
    }

    private mutating func activate(_ which: Active, at now: TimeInterval, _ event: ShortcutEvent) -> ShortcutOutcome {
        active = which
        activeSince = now
        return ShortcutOutcome(events: [event])
    }

    private mutating func finish() {
        active = nil
        activeSince = nil
    }
}

/// Records a new shortcut from what the user presses: the largest set of modifiers held, plus
/// the first regular key. Finishes on a key press or when every modifier is released.
public struct ShortcutRecorder: Sendable {
    public enum Result: Sendable, Equatable {
        case pending
        case cancelled
        case done(KeyShortcut)
    }

    private var most: [ModifierKey] = []
    private var held: [ModifierKey] = []

    public init() {}

    public mutating func modifiersChanged(_ held: [ModifierKey]) -> Result {
        self.held = held
        if held.count >= most.count { most = held }
        if held.isEmpty, !most.isEmpty { return .done(KeyShortcut(Self.generalize(most))) }
        return .pending
    }

    public mutating func keyDown(_ code: Int, label: String?) -> Result {
        if code == ShortcutMatcher.escapeKeyCode, held.isEmpty { return .cancelled }
        return .done(KeyShortcut(Self.generalize(held), keyCode: code, keyLabel: label))
    }

    /// What's held so far, as keycaps.
    public var preview: [String] {
        KeyShortcut(Self.generalize(held.isEmpty ? most : held)).keycaps
    }

    /// A lone right-hand modifier stays specific ("Right ⌥"); otherwise either side will do.
    static func generalize(_ held: [ModifierKey]) -> [ModifierKey] {
        if held.count == 1, held[0].side == .right { return held }
        return held.map { ModifierKey($0.kind) }
    }
}
