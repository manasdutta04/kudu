import Testing
@testable import KuduCore

@Suite struct WhitespaceNormalizerTests {
    let stage = WhitespaceNormalizer()

    @Test(arguments: [
        ("  hello   world  ", "hello world"),
        ("wait , what ?", "wait, what?"),
        ("one\ttwo", "one\ttwo"),
    ])
    func normalizes(input: String, expected: String) {
        #expect(stage.apply(input, context: TextContext()) == expected)
    }
}

@Suite struct CursorFitterTests {
    let stage = CursorFitter()

    @Test func noContextLeavesTextAlone() {
        #expect(stage.apply("Hello there.", context: TextContext()) == "Hello there.")
    }

    @Test func continuesAfterWord() {
        let context = TextContext(precedingCharacter: "d")
        #expect(stage.apply("And then more.", context: context) == " and then more.")
    }

    @Test func keepsPronounICapitalized() {
        let context = TextContext(precedingCharacter: ",")
        #expect(stage.apply("I think so.", context: context) == " I think so.")
    }

    @Test func keepsAcronyms() {
        let context = TextContext(precedingCharacter: "e")
        #expect(stage.apply("NASA launched.", context: context) == " NASA launched.")
    }

    @Test func noSpaceAfterWhitespaceOrOpenParen() {
        #expect(stage.apply("Yes.", context: TextContext(precedingCharacter: " ")) == "Yes.")
        #expect(stage.apply("Yes.", context: TextContext(precedingCharacter: "(")) == "Yes.")
    }

    @Test func newSentenceKeepsCapital() {
        let context = TextContext(precedingCharacter: ".")
        #expect(stage.apply("Next one.", context: context) == " Next one.")
    }

    @Test func codeContextDropsTrailingPeriod() {
        let context = TextContext(category: .code)
        #expect(stage.apply("git status.", context: context) == "git status")
        #expect(stage.apply("wait...", context: context) == "wait...")
    }
}

@Suite struct TextPipelineTests {
    @Test func basicPipelineRunsAllStages() {
        let context = TextContext(category: .code, precedingCharacter: "x")
        #expect(TextPipeline.basic.process("  Run the tests .  ", context: context) == " run the tests")
    }
}
