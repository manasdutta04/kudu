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

/// Fixes words the speech model misheard when they sound like a name or term on screen:
/// "Sidarth" → "Siddharth", "github" → "GitHub". Only words the macOS spell checker doesn't
/// know are changed, so a real word never turns into a name by accident; the exception is the
/// same word with a name's inner capitals ("github" → "GitHub").
@MainActor
struct ScreenSpeller {
    let terms: [String]

    func apply(_ text: String) -> String {
        guard !terms.isEmpty else { return text }
        let checker = NSSpellChecker.shared
        var result = text
        let pattern = try! NSRegularExpression(pattern: "[\\p{L}][\\p{L}'’]{2,}")
        for match in pattern.matches(in: text, range: NSRange(text.startIndex..., in: text)).reversed() {
            guard let range = Range(match.range, in: result) else { continue }
            let word = String(result[range])
            if let exact = terms.first(where: { $0.lowercased() == word.lowercased() && $0 != word && $0.dropFirst().contains(where: \.isUppercase) }) {
                result.replaceSubrange(range, with: exact)
                continue
            }
            let misspelled = checker.checkSpelling(of: word, startingAt: 0).location != NSNotFound
            guard misspelled,
                  let term = terms.first(where: {
                      !$0.contains(".") && !$0.contains("_") && abs($0.count - word.count) <= 3
                          && CorrectionLearner.soundsAlike(word, $0)
                  })
            else { continue }
            result.replaceSubrange(range, with: term)
        }
        return result
    }
}
