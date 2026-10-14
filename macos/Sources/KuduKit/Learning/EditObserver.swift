import AppKit
import ApplicationServices
import KuduCore
import os

/// Watches the field Kudu just typed into, for a short while, to learn from corrections:
/// if the user fixes a word Kudu got wrong, `CorrectionLearner` finds it and `onLearned`
/// saves it to the dictionary so it's right next time.
///
/// It reads the field's text through Accessibility only, only after a dictation, for at most
/// `duration`, and stops when you leave the app. A correction is learned as soon as the edit
/// settles (unchanged for two checks), so the notice appears right after you fix the word.
/// Nothing is stored except the learned word.
@MainActor
final class EditObserver {
    var onLearned: (([LearnedCorrection]) -> Void)?
    var duration: Duration = .seconds(60)
    var interval: Duration = .milliseconds(1000)

    private var task: Task<Void, Never>?
    private let log = Logger(subsystem: "app.kudu", category: "learning")

    func cancel() {
        task?.cancel()
        task = nil
    }

    /// Call right after inserting `inserted`. `known` are spellings already in the dictionary.
    func watch(inserted: String, known: Set<String>) {
        cancel()
        let inserted = inserted.trimmingCharacters(in: .whitespacesAndNewlines)
        guard WordCount.count(inserted) >= 1, let pid = NSWorkspace.shared.frontmostApplication?.processIdentifier
        else { return }
        task = Task { [weak self, duration, interval] in
            // Give the target app a moment to apply the paste; web views can take a while to
            // expose the new text.
            var found: (AXUIElement, String, String)?
            for delay in [400, 600, 1000] {
                try? await Task.sleep(for: .milliseconds(delay))
                if Task.isCancelled { return }
                if let element = Self.focusedElement(), let snapshot = Self.value(of: element),
                   let range = Self.locate(inserted, in: snapshot) {
                    found = (element, String(snapshot[..<range.lowerBound]), String(snapshot[range.upperBound...]))
                    break
                }
            }
            guard let (start, prefix, suffix) = found else {
                self?.log.info("Edit observer: the field's text isn't readable here")
                return
            }
            var element = start

            var latestEdit: String?
            var stableChecks = 0
            let deadline = ContinuousClock.now + duration
            while ContinuousClock.now < deadline, !Task.isCancelled {
                try? await Task.sleep(for: interval)
                guard NSWorkspace.shared.frontmostApplication?.processIdentifier == pid else { break }
                // Web views sometimes hand out a new element for the same field; follow it.
                var current = Self.value(of: element)
                if current.map({ !Self.surrounds($0, prefix, suffix) }) ?? true,
                   let focused = Self.focusedElement(), let value = Self.value(of: focused), Self.surrounds(value, prefix, suffix) {
                    element = focused
                    current = value
                }
                guard let current, Self.surrounds(current, prefix, suffix) else { break } // sent, cleared or left
                let startIndex = current.index(current.startIndex, offsetBy: prefix.count)
                let endIndex = current.index(current.endIndex, offsetBy: -suffix.count)
                let edited = String(current[startIndex..<endIndex]).trimmingCharacters(in: .whitespacesAndNewlines)
                let edit = edited == inserted ? nil : edited
                if edit == latestEdit {
                    stableChecks += 1
                } else {
                    latestEdit = edit
                    stableChecks = 0
                }
                // The edit has settled: learn now, so the notice comes while it's relevant.
                if let latestEdit, stableChecks >= 2, self?.learn(inserted: inserted, edited: latestEdit, known: known) == true {
                    return
                }
            }
            guard !Task.isCancelled, let latestEdit else { return }
            _ = self?.learn(inserted: inserted, edited: latestEdit, known: known)
        }
    }

    private func learn(inserted: String, edited: String, known: Set<String>) -> Bool {
        let learned = CorrectionLearner.learn(inserted: inserted, edited: edited, known: known) { word in
            NSSpellChecker.shared.checkSpelling(of: word, startingAt: 0).location == NSNotFound
        }
        log.info("Observed an edit; learned \(learned.count) correction(s)")
        guard !learned.isEmpty else { return false }
        onLearned?(learned)
        return true
    }

    private static func surrounds(_ text: String, _ prefix: String, _ suffix: String) -> Bool {
        text.count >= prefix.count + suffix.count && text.hasPrefix(prefix) && text.hasSuffix(suffix)
    }

    /// Where Kudu's text sits in the field. Web views may turn spaces into non-breaking ones
    /// or newlines into paragraph breaks, so whitespace matches loosely.
    static func locate(_ inserted: String, in text: String) -> Range<String.Index>? {
        if let range = text.range(of: inserted, options: .backwards) { return range }
        let words = inserted.split(whereSeparator: \.isWhitespace).map { NSRegularExpression.escapedPattern(for: String($0)) }
        guard !words.isEmpty,
              let regex = try? NSRegularExpression(pattern: words.joined(separator: "[\\s\\u00a0\\u200b]+"))
        else { return nil }
        let matches = regex.matches(in: text, range: NSRange(text.startIndex..., in: text))
        return matches.last.flatMap { Range($0.range, in: text) }
    }

    private static func focusedElement() -> AXUIElement? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(AXUIElementCreateSystemWide(), kAXFocusedUIElementAttribute as CFString, &value) == .success,
              let value, CFGetTypeID(value) == AXUIElementGetTypeID()
        else { return nil }
        let element = value as! AXUIElement
        AXUIElementSetMessagingTimeout(element, 0.15)
        return element
    }

    private static func value(of element: AXUIElement) -> String? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, kAXValueAttribute as CFString, &value) == .success else { return nil }
        return value as? String
    }
}
