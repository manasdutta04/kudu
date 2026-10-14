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

@Suite struct LineStartCapitalizerTests {
    let stage = LineStartCapitalizer()

    @Test func capitalizesEachNewLine() {
        #expect(stage.apply("Dear Priya,\nthanks for this.\n\nbest,\nManas", context: TextContext())
            == "Dear Priya,\nThanks for this.\n\nBest,\nManas")
    }

    @Test func handlesNumberedListItems() {
        #expect(stage.apply("We need:\n1. the page\n2. the email", context: TextContext())
            == "We need:\n1. The page\n2. The email")
    }

    @Test func leavesFirstLineAndSingleLinesAlone() {
        #expect(stage.apply("and then more", context: TextContext()) == "and then more")
        #expect(stage.apply("and then\nmore", context: TextContext()) == "and then\nMore")
    }
}
