import AppKit
import ApplicationServices
import KuduCore
import os

/// Watches the field Kudu just typed into, for a short while, to learn from corrections:
/// if the user fixes a name Kudu misspelled, `CorrectionLearner` finds it and `onLearned`
/// saves it to the dictionary so it's right next time.
///
/// It reads the field's text through Accessibility only, only after a dictation, for at most
/// `duration`, and stops when focus moves elsewhere. Nothing is stored except the learned word.
/// Fields that don't expose their text (many web and Electron apps) are skipped.
@MainActor
final class EditObserver {
    var onLearned: (([LearnedCorrection]) -> Void)?
    var duration: Duration = .seconds(45)
    var interval: Duration = .milliseconds(1500)

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
        guard WordCount.count(inserted) >= 2 else { return }
        task = Task { [weak self, duration, interval] in
            // Give the target app a moment to apply the paste before taking the snapshot.
            try? await Task.sleep(for: .milliseconds(400))
            guard let element = Self.focusedElement(), let snapshot = Self.value(of: element),
                  let range = snapshot.range(of: inserted, options: .backwards)
            else { return }
            let prefix = String(snapshot[..<range.lowerBound])
            let suffix = String(snapshot[range.upperBound...])

            var latestEdit: String?
            let deadline = ContinuousClock.now + duration
            while ContinuousClock.now < deadline, !Task.isCancelled {
                try? await Task.sleep(for: interval)
                guard let focused = Self.focusedElement(), CFEqual(focused, element),
                      let current = Self.value(of: element),
                      current.hasPrefix(prefix), current.hasSuffix(suffix),
                      current.count >= prefix.count + suffix.count
                else { break } // focus moved, the field was cleared (sent), or text around it changed
                let start = current.index(current.startIndex, offsetBy: prefix.count)
                let end = current.index(current.endIndex, offsetBy: -suffix.count)
                let edited = String(current[start..<end]).trimmingCharacters(in: .whitespacesAndNewlines)
                latestEdit = edited == inserted ? nil : edited
            }
            guard !Task.isCancelled, let self, let latestEdit else { return }
            let learned = CorrectionLearner.learn(inserted: inserted, edited: latestEdit, known: known)
            self.log.info("Observed an edit; learned \(learned.count) correction(s)")
            if !learned.isEmpty { self.onLearned?(learned) }
        }
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
