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
