import AppKit
import ApplicationServices
import Carbon.HIToolbox

/// Reads the text currently selected in the frontmost app: through Accessibility when the app
/// supports it, otherwise by simulating ⌘C and restoring the clipboard right after.
@MainActor
enum SelectionReader {
    static func selectedText() async -> String? {
        if let text = accessibilitySelectionNow(), !text.isEmpty { return text }
        return await copySelection()
    }

    /// The selection through Accessibility only: instant, safe while keys are held.
    static func accessibilitySelectionNow() -> String? {
        let systemWide = AXUIElementCreateSystemWide()
        var focused: CFTypeRef?
        guard AXUIElementCopyAttributeValue(systemWide, kAXFocusedUIElementAttribute as CFString, &focused) == .success,
              let focused, CFGetTypeID(focused) == AXUIElementGetTypeID()
        else { return nil }
        let element = focused as! AXUIElement
        AXUIElementSetMessagingTimeout(element, 0.1)
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, kAXSelectedTextAttribute as CFString, &value) == .success else {
            return nil
        }
        return value as? String
    }

    /// Simulated ⌘C. Only trusted if the clipboard actually changed (otherwise nothing was
    /// selected and the old clipboard would be mistaken for a selection).
    private static func copySelection() async -> String? {
        let pasteboard = NSPasteboard.general
        let saved = (pasteboard.pasteboardItems ?? []).map { item in
            item.types.reduce(into: [NSPasteboard.PasteboardType: Data]()) { entry, type in
                if let data = item.data(forType: type) { entry[type] = data }
            }
        }
        let before = pasteboard.changeCount

        let source = CGEventSource(stateID: .combinedSessionState)
        let cKey = KeyboardLayout.keyCode(for: "c", fallback: CGKeyCode(kVK_ANSI_C))
        for down in [true, false] {
            let event = CGEvent(keyboardEventSource: source, virtualKey: cKey, keyDown: down)
            event?.flags = .maskCommand
            event?.post(tap: .cghidEventTap)
        }

        var copied: String?
        for _ in 0..<8 { // up to ~200 ms for the app to answer
            try? await Task.sleep(for: .milliseconds(25))
            if pasteboard.changeCount != before {
                copied = pasteboard.string(forType: .string)
                break
            }
        }
        guard pasteboard.changeCount != before else { return nil }
        pasteboard.clearContents()
        let restored = saved.map { entry -> NSPasteboardItem in
            let item = NSPasteboardItem()
            for (type, data) in entry { item.setData(data, forType: type) }
            return item
        }
        if !restored.isEmpty { pasteboard.writeObjects(restored) }
        return copied
    }
}
