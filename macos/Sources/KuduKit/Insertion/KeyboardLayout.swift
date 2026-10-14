import Carbon.HIToolbox
import CoreGraphics
import Foundation

/// Finds which physical key types a given character on the user's current keyboard layout.
/// ⌘V has to be posted with the key that produces "v" (it moves on Dvorak, AZERTY, Colemak...),
/// otherwise a non-QWERTY user gets ⌘. or another shortcut instead of paste.
@MainActor
enum KeyboardLayout {
    private static var cache: [Character: CGKeyCode] = [:]
    private static var observer: NSObjectProtocol?

    static func keyCode(for character: Character, fallback: CGKeyCode) -> CGKeyCode {
        startObservingIfNeeded()
        if let cached = cache[character] { return cached }
        let code = lookUp(character) ?? fallback
        cache[character] = code
        return code
    }

    /// Keys that don't type a character, by name. F-keys, arrows and the like can be held on
    /// their own as a shortcut; keys that type text can't.
    private static let named: [Int: String] = [
        49: "Space", 36: "Return", 76: "Enter", 48: "Tab", 51: "Delete", 117: "⌦", 53: "esc",
        123: "←", 124: "→", 125: "↓", 126: "↑", 115: "Home", 119: "End", 116: "Page Up", 121: "Page Down",
        122: "F1", 120: "F2", 99: "F3", 118: "F4", 96: "F5", 97: "F6", 98: "F7", 100: "F8", 101: "F9",
        109: "F10", 103: "F11", 111: "F12", 105: "F13", 107: "F14", 113: "F15", 106: "F16", 64: "F17",
        79: "F18", 80: "F19", 90: "F20",
    ]

    /// Whether a key types text (so it can't be held alone as a dictation shortcut).
    public static func typesText(_ keyCode: Int) -> Bool {
        named[keyCode] == nil || keyCode == 49
    }

    /// What a key shows on the current layout: "V", "Space", "F5".
    static func label(for keyCode: Int) -> String {
        if let name = named[keyCode] { return name }
        guard let source = TISCopyCurrentASCIICapableKeyboardLayoutInputSource()?.takeRetainedValue(),
              let layoutPointer = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData)
        else { return "Key \(keyCode)" }
        let layoutData = Unmanaged<CFData>.fromOpaque(layoutPointer).takeUnretainedValue() as Data
        let text = layoutData.withUnsafeBytes { raw -> String? in
            guard let layout = raw.baseAddress?.assumingMemoryBound(to: UCKeyboardLayout.self) else { return nil }
            var deadKeyState: UInt32 = 0
            var length = 0
            var chars = [UniChar](repeating: 0, count: 4)
            let status = UCKeyTranslate(
                layout, UInt16(keyCode), UInt16(kUCKeyActionDisplay), 0, UInt32(LMGetKbdType()),
                OptionBits(kUCKeyTranslateNoDeadKeysBit), &deadKeyState, chars.count, &length, &chars
            )
            return status == noErr && length > 0 ? String(utf16CodeUnits: chars, count: length) : nil
        }
        return text.map { $0.uppercased() } ?? "Key \(keyCode)"
    }

    private static func lookUp(_ character: Character) -> CGKeyCode? {
        guard let source = TISCopyCurrentASCIICapableKeyboardLayoutInputSource()?.takeRetainedValue(),
              let layoutPointer = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData)
        else { return nil }
        let layoutData = Unmanaged<CFData>.fromOpaque(layoutPointer).takeUnretainedValue() as Data
        let target = String(character).lowercased()

        return layoutData.withUnsafeBytes { raw -> CGKeyCode? in
            guard let layout = raw.baseAddress?.assumingMemoryBound(to: UCKeyboardLayout.self) else { return nil }
            for keyCode in 0..<128 {
                var deadKeyState: UInt32 = 0
                var length = 0
                var chars = [UniChar](repeating: 0, count: 4)
                let status = UCKeyTranslate(
                    layout, UInt16(keyCode), UInt16(kUCKeyActionDown), 0, UInt32(LMGetKbdType()),
                    OptionBits(kUCKeyTranslateNoDeadKeysBit), &deadKeyState, chars.count, &length, &chars
                )
                if status == noErr, length > 0,
                   String(utf16CodeUnits: chars, count: length).lowercased() == target {
                    return CGKeyCode(keyCode)
                }
            }
            return nil
        }
    }

    private static func startObservingIfNeeded() {
        guard observer == nil else { return }
        observer = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name(kTISNotifySelectedKeyboardInputSourceChanged as String),
            object: nil, queue: .main
        ) { _ in
            MainActor.assumeIsolated { cache.removeAll() }
        }
    }
}
