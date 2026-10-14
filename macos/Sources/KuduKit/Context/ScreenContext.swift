import AppKit
import ApplicationServices
import KuduCore

/// Reads the text visible in the focused window through Accessibility, to find the names and
/// terms on screen (`TermExtractor`). Bounded in time and size; runs off the main thread when a
/// dictation starts, and nothing is stored.
enum ScreenContext {
    static func terms(budget: Duration = .milliseconds(150), maxElements: Int = 400) -> [String] {
        guard let pid = NSWorkspace.shared.frontmostApplication?.processIdentifier else { return [] }
        let app = AXUIElementCreateApplication(pid)
        AXUIElementSetMessagingTimeout(app, 0.05)
        guard let window = element(app, kAXFocusedWindowAttribute) else { return [] }

        let deadline = ContinuousClock.now + budget
        var queue = [window]
        var texts: [String] = []
        var visited = 0
        var characters = 0
        while !queue.isEmpty, visited < maxElements, characters < 40_000, ContinuousClock.now < deadline {
            let current = queue.removeFirst()
            visited += 1
            AXUIElementSetMessagingTimeout(current, 0.05)
            for attribute in [kAXValueAttribute, kAXTitleAttribute, kAXDescriptionAttribute] {
                if let text = string(current, attribute), !text.isEmpty {
                    let clipped = String(text.prefix(4_000))
                    texts.append(clipped)
                    characters += clipped.count
                }
            }
            if let children = copy(current, kAXChildrenAttribute) as? [AXUIElement] {
                queue.append(contentsOf: children.prefix(60))
            }
        }
        return TermExtractor.terms(from: texts)
    }

    private static func copy(_ element: AXUIElement, _ attribute: String) -> CFTypeRef? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, attribute as CFString, &value) == .success else { return nil }
        return value
    }

    private static func element(_ parent: AXUIElement, _ attribute: String) -> AXUIElement? {
        guard let value = copy(parent, attribute), CFGetTypeID(value) == AXUIElementGetTypeID() else { return nil }
        return (value as! AXUIElement)
    }

    private static func string(_ element: AXUIElement, _ attribute: String) -> String? {
        copy(element, attribute) as? String
    }
}

/// `TermSpeller` with the macOS spell checker deciding which words are ordinary vocabulary.
@MainActor
enum Speller {
    static func make(dictionary: [String], screen: [String]) -> TermSpeller {
        TermSpeller(dictionary: dictionary, screen: screen) { word in
            MainActor.assumeIsolated {
                NSSpellChecker.shared.checkSpelling(of: word, startingAt: 0).location == NSNotFound
            }
        }
    }
}
