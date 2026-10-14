import AppKit
import Carbon.HIToolbox
import ApplicationServices
import os

/// Puts text at the cursor of whatever app is focused.
///
/// Strategy, in order:
/// 1. Accessibility: set `kAXSelectedTextAttribute` on the focused element. Instant, leaves
///    the clipboard alone, and works in native AppKit/SwiftUI text views.
/// 2. Paste: put the text on the pasteboard, post ⌘V, then restore the user's clipboard.
///    Works nearly everywhere, including Electron apps, browsers and terminals, which accept
///    the AX write but silently ignore it.
@MainActor
public final class TextInserter {
    private let log = Logger(subsystem: "app.kudu", category: "insert")

    /// Apps where AX writes report success but don't actually insert text.
    private let pasteOnlyBundleIDs: Set<String> = [
        "com.apple.Terminal", "com.googlecode.iterm2", "dev.warp.Warp-Stable", "com.mitchellh.ghostty",
        "com.microsoft.VSCode", "com.todesktop.230313mzl4w4u92", // Cursor
        "com.google.Chrome", "com.brave.Browser", "company.thebrowser.Browser", "org.mozilla.firefox",
        "com.tinyspeck.slackmacgap", "com.hnc.Discord", "notion.id", "com.microsoft.teams2",
    ]

    /// How long to wait before restoring the clipboard after posting ⌘V. The target app reads
    /// the pasteboard asynchronously, so restoring too early pastes the old contents.
    public var clipboardRestoreDelay: Duration = .milliseconds(350)

    public init() {}

    public enum Method: String, Sendable { case accessibility, paste }

    @discardableResult
    public func insert(_ text: String) async -> Method {
        guard !text.isEmpty else { return .accessibility }
        let app = NSWorkspace.shared.frontmostApplication
        if let app, shouldTryAccessibility(for: app), insertViaAccessibility(text) {
            return .accessibility
        }
        await insertViaPaste(text)
        return .paste
    }

    /// The character right before the cursor, used to decide spacing and capitalization.
    public func precedingCharacter() -> Character? {
        guard let element = focusedElement(),
              let value = copyAttribute(element, kAXValueAttribute) as? String,
              let rangeValue = copyAttribute(element, kAXSelectedTextRangeAttribute),
              CFGetTypeID(rangeValue) == AXValueGetTypeID()
        else { return nil }
        var range = CFRange()
        guard AXValueGetValue(rangeValue as! AXValue, .cfRange, &range), range.location > 0 else { return nil }
        let utf16 = value.utf16
        guard range.location <= utf16.count,
              let index = utf16.index(utf16.startIndex, offsetBy: range.location - 1, limitedBy: utf16.endIndex),
              let scalarIndex = index.samePosition(in: value)
        else { return nil }
        return value[scalarIndex]
    }

    // MARK: - Accessibility

    private func shouldTryAccessibility(for app: NSRunningApplication) -> Bool {
        if let id = app.bundleIdentifier, pasteOnlyBundleIDs.contains(id) { return false }
        // Electron and Chromium shells accept AX writes but ignore them.
        if let bundleURL = app.bundleURL {
            let electron = bundleURL.appendingPathComponent("Contents/Frameworks/Electron Framework.framework")
            if FileManager.default.fileExists(atPath: electron.path) { return false }
        }
        return true
    }

    private func insertViaAccessibility(_ text: String) -> Bool {
        guard let element = focusedElement() else { return false }
        let role = copyAttribute(element, kAXRoleAttribute) as? String
        let textRoles: Set<String> = [kAXTextFieldRole, kAXTextAreaRole, kAXComboBoxRole, "AXSearchField"]
        guard let role, textRoles.contains(role) else { return false }

        var settable: DarwinBoolean = false
        guard AXUIElementIsAttributeSettable(element, kAXSelectedTextAttribute as CFString, &settable) == .success,
              settable.boolValue
        else { return false }

        let before = (copyAttribute(element, kAXValueAttribute) as? String)?.utf16.count
        let status = AXUIElementSetAttributeValue(element, kAXSelectedTextAttribute as CFString, text as CFString)
        guard status == .success else { return false }

        // Verify when we can: if the value is readable it must have changed length.
        if let before, let after = (copyAttribute(element, kAXValueAttribute) as? String)?.utf16.count,
           after == before, !text.isEmpty {
            log.info("AX write reported success but value unchanged; falling back to paste")
            return false
        }
        return true
    }

    private func focusedElement() -> AXUIElement? {
        let systemWide = AXUIElementCreateSystemWide()
        guard let value = copyAttribute(systemWide, kAXFocusedUIElementAttribute),
              CFGetTypeID(value) == AXUIElementGetTypeID()
        else { return nil }
        return (value as! AXUIElement)
    }

    private func copyAttribute(_ element: AXUIElement, _ attribute: String) -> CFTypeRef? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, attribute as CFString, &value) == .success else { return nil }
        return value
    }

    // MARK: - Paste

    private func insertViaPaste(_ text: String) async {
        let pasteboard = NSPasteboard.general
        let saved = savePasteboard(pasteboard)

        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
        // Ask clipboard managers not to record this temporary write (nspasteboard.org convention).
        pasteboard.setData(Data(), forType: NSPasteboard.PasteboardType("org.nspasteboard.TransientType"))
        let ourChangeCount = pasteboard.changeCount

        postCommandV()

        try? await Task.sleep(for: clipboardRestoreDelay)
        // Only restore if nobody else (the user, another app) changed the clipboard meanwhile.
        guard pasteboard.changeCount == ourChangeCount else { return }
        restorePasteboard(pasteboard, items: saved)
    }

    private func postCommandV() {
        let source = CGEventSource(stateID: .combinedSessionState)
        let vKey = KeyboardLayout.keyCode(for: "v", fallback: CGKeyCode(kVK_ANSI_V))
        let down = CGEvent(keyboardEventSource: source, virtualKey: vKey, keyDown: true)
        let up = CGEvent(keyboardEventSource: source, virtualKey: vKey, keyDown: false)
        // Set flags explicitly so a still-held trigger key (fn, ⌥) doesn't turn this into another shortcut.
        down?.flags = .maskCommand
        up?.flags = .maskCommand
        down?.post(tap: .cghidEventTap)
        up?.post(tap: .cghidEventTap)
    }

    private func savePasteboard(_ pasteboard: NSPasteboard) -> [[NSPasteboard.PasteboardType: Data]] {
        (pasteboard.pasteboardItems ?? []).map { item in
            var entry: [NSPasteboard.PasteboardType: Data] = [:]
            for type in item.types {
                if let data = item.data(forType: type) { entry[type] = data }
            }
            return entry
        }
    }

    private func restorePasteboard(_ pasteboard: NSPasteboard, items: [[NSPasteboard.PasteboardType: Data]]) {
        pasteboard.clearContents()
        guard !items.isEmpty else { return }
        let restored = items.map { entry -> NSPasteboardItem in
            let item = NSPasteboardItem()
            for (type, data) in entry { item.setData(data, forType: type) }
            return item
        }
        pasteboard.writeObjects(restored)
    }
}
