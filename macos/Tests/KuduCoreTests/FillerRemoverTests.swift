import Testing
@testable import KuduCore

@Suite struct FillerRemoverTests {
    let stage = FillerRemover()

    @Test(arguments: [
        ("Um, so we should ship it.", "So we should ship it."),
        ("So, uh, we should ship it.", "So, we should ship it."),
        ("I think, umm, Tuesday works.", "I think, Tuesday works."),
        ("Hmm. Let me check.", "Let me check."),
        ("Ship it, uh.", "Ship it."),
    ])
    func removesFillers(input: String, expected: String) {
        #expect(stage.apply(input, context: TextContext()) == expected)
    }

    @Test func leavesWordsContainingFillersAlone() {
        let text = "The umbrella is under the drum."
        #expect(stage.apply(text, context: TextContext()) == text)
    }

    @Test func leavesCleanTextUntouched() {
        let text = "already clean text"
        #expect(stage.apply(text, context: TextContext()) == text)
    }
}
