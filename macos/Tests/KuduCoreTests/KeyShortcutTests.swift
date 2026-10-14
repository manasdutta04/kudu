import Testing
@testable import KuduCore

@Suite struct KeyShortcutTests {
    let fn = ModifierKey(.fn)
    func left(_ kind: ModifierKind) -> ModifierKey { ModifierKey(kind, side: .left) }
    func right(_ kind: ModifierKind) -> ModifierKey { ModifierKey(kind, side: .right) }

    @Test func holdingFnDictatesAndReleasingStops() {
        var matcher = ShortcutMatcher(shortcuts: .defaults)
        #expect(matcher.modifiersChanged([fn], at: 0).events == [.pressed])
        #expect(matcher.modifiersChanged([], at: 2).events == [.released])
    }

    @Test func addingControlQuicklyIsCommandMode() {
        var matcher = ShortcutMatcher(shortcuts: .defaults)
        _ = matcher.modifiersChanged([fn], at: 0)
        #expect(matcher.modifiersChanged([fn, left(.control)], at: 0.1).events == [.commandPressed])
        // Letting go of ⌃ first keeps it going while fn is held; releasing fn ends it.
        #expect(matcher.modifiersChanged([fn], at: 1).events.isEmpty)
        #expect(matcher.modifiersChanged([], at: 1.1).events == [.commandReleased])
    }

    @Test func controlAlreadyHeldStartsCommandModeStraightAway() {
        var matcher = ShortcutMatcher(shortcuts: .defaults)
        _ = matcher.modifiersChanged([left(.control)], at: 0)
        #expect(matcher.modifiersChanged([fn, left(.control)], at: 0.5).events == [.commandPressed])
    }

    @Test func anotherModifierJoiningQuicklyIsASystemShortcut() {
        var matcher = ShortcutMatcher(shortcuts: ShortcutSet(dictate: .rightCommand, command: nil, pasteLast: nil))
        _ = matcher.modifiersChanged([right(.command)], at: 0)
        #expect(matcher.modifiersChanged([right(.command), left(.shift)], at: 0.1).events == [.cancelled])
        #expect(matcher.modifiersChanged([right(.command)], at: 0.3).events.isEmpty)
        _ = matcher.modifiersChanged([], at: 0.4)
        #expect(matcher.modifiersChanged([right(.command)], at: 0.5).events == [.pressed])
    }

    @Test func aRegularKeyDuringTheHoldCancelsIt() {
        var matcher = ShortcutMatcher(shortcuts: .defaults)
        _ = matcher.modifiersChanged([fn], at: 0)
        let outcome = matcher.keyDown(123, isRepeat: false, at: 0.05) // fn + ← (Home)
        #expect(outcome.events == [.cancelled])
        #expect(!outcome.swallow)
    }

    @Test func escapeCancelsADictation() {
        var matcher = ShortcutMatcher(shortcuts: .defaults)
        matcher.isDictating = true
        #expect(matcher.keyDown(53, isRepeat: false, at: 0) == ShortcutOutcome(events: [.cancelled], swallow: true))
    }

    @Test func pasteLastIsAPressedShortcut() {
        var matcher = ShortcutMatcher(shortcuts: .defaults)
        _ = matcher.modifiersChanged([left(.control), right(.command)], at: 0)
        #expect(matcher.keyDown(9, isRepeat: false, at: 0) == ShortcutOutcome(events: [.pasteLast], swallow: true))
        _ = matcher.modifiersChanged([], at: 0.1)
        #expect(matcher.keyDown(9, isRepeat: false, at: 0.2) == ShortcutOutcome())
    }

    @Test func aKeyShortcutHoldsAndHidesItsKey() {
        let space = KeyShortcut([ModifierKey(.option)], keyCode: 49, keyLabel: "Space")
        var matcher = ShortcutMatcher(shortcuts: ShortcutSet(dictate: space, command: nil, pasteLast: nil))
        _ = matcher.modifiersChanged([left(.option)], at: 0)
        #expect(matcher.keyDown(49, isRepeat: false, at: 0) == ShortcutOutcome(events: [.pressed], swallow: true))
        #expect(matcher.keyDown(49, isRepeat: true, at: 0.5) == ShortcutOutcome(swallow: true))
        #expect(matcher.modifiersChanged([], at: 0.9).events.isEmpty)
        #expect(matcher.keyUp(49, at: 1) == ShortcutOutcome(events: [.released], swallow: true))
    }

    @Test func aRightOnlyShortcutIgnoresTheLeftKey() {
        var matcher = ShortcutMatcher(shortcuts: ShortcutSet(dictate: .rightOption, command: nil, pasteLast: nil))
        #expect(matcher.modifiersChanged([left(.option)], at: 0).events.isEmpty)
        _ = matcher.modifiersChanged([], at: 0.1)
        #expect(matcher.modifiersChanged([right(.option)], at: 0.2).events == [.pressed])
    }

    @Test func recordsShortcuts() {
        var recorder = ShortcutRecorder()
        #expect(recorder.modifiersChanged([left(.control)]) == .pending)
        #expect(recorder.modifiersChanged([left(.control), left(.option)]) == .pending)
        #expect(recorder.preview == ["⌃", "⌥"])
        #expect(recorder.modifiersChanged([]) == .done(KeyShortcut([ModifierKey(.control), ModifierKey(.option)])))

        recorder = ShortcutRecorder()
        _ = recorder.modifiersChanged([right(.option)])
        #expect(recorder.modifiersChanged([]) == .done(.rightOption))

        recorder = ShortcutRecorder()
        _ = recorder.modifiersChanged([left(.option)])
        #expect(recorder.keyDown(49, label: "Space") == .done(KeyShortcut([ModifierKey(.option)], keyCode: 49, keyLabel: "Space")))

        var escape = ShortcutRecorder()
        #expect(escape.keyDown(53, label: "esc") == .cancelled)
    }

    @Test func displaysTheMacWay() {
        #expect(KeyShortcut.fn.displayName == "fn")
        #expect(KeyShortcut.rightOption.displayName == "Right ⌥")
        #expect(ShortcutSet.defaults.command?.displayName == "fn ⌃")
        #expect(ShortcutSet.defaults.pasteLast?.displayName == "⌃⌘V")
        #expect(ShortcutSet.defaults.pasteLast?.keycaps == ["⌃", "⌘", "V"])
    }

    @Test func rejectsShortcutsThatGetInTheWay() {
        let typesText: (Int) -> Bool = { $0 != 101 } // 101 = F9
        #expect(!KeyShortcut([], keyCode: 9, keyLabel: "V").isUsableForHolding(keyTypesText: typesText))
        #expect(!KeyShortcut([ModifierKey(.shift)], keyCode: 9, keyLabel: "V").isUsableForHolding(keyTypesText: typesText))
        #expect(KeyShortcut([], keyCode: 101, keyLabel: "F9").isUsableForHolding(keyTypesText: typesText))
        var same = ShortcutSet.defaults
        same.command = same.dictate
        #expect(same.problems.count == 1)
    }
}
