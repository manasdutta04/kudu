import Testing
@testable import KuduCore

@Suite struct SpokenCommandsTests {
    let stage = SpokenCommands()

    @Test func newLine() {
        #expect(stage.apply("Hi Sam. New line. Are you free?", context: TextContext()) == "Hi Sam.\nAre you free?")
    }

    @Test func newParagraph() {
        #expect(
            stage.apply("Thanks for the update, new paragraph, best, Manas", context: TextContext())
                == "Thanks for the update\n\nBest, Manas"
        )
    }

    @Test func lowercaseFollowingTextIsCapitalized() {
        #expect(stage.apply("first item new line second item", context: TextContext()) == "First item\nSecond item")
    }

    @Test func ordinaryTextIsUntouched() {
        let text = "We need a newline character here."
        #expect(stage.apply(text, context: TextContext()) == text)
    }
}
