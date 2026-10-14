import Testing
@testable import KuduCore

@Suite struct SelfCorrectionTests {
    let stage = SelfCorrection()

    @Test(arguments: [
        ("Let's meet Tuesday, no, Wednesday.", "Let's meet Wednesday."),
        ("Call me at 3, sorry, 4 pm.", "Call me at 4 pm."),
        ("Book the red, no wait, blue room.", "Book the blue room."),
        ("Send it to Alex. Scratch that. Send it to Sam.", "Send it to Sam."),
        ("Scratch that, ship it today.", "Ship it today."),
    ])
    func appliesCorrections(input: String, expected: String) {
        #expect(stage.apply(input, context: TextContext()) == expected)
    }

    @Test(arguments: [
        "No, I don't think so.",
        "Sorry, I missed that.",
        "There is no way, sorry.",
        "Say no to meetings.",
    ])
    func leavesOrdinaryUsesAlone(text: String) {
        #expect(stage.apply(text, context: TextContext()) == text)
    }
}
