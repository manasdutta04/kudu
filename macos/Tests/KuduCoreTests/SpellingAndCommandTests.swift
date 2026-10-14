import Testing
@testable import KuduCore

@Suite struct TermSpellerTests {
    /// A tiny stand-in for the system spell checker.
    static let vocabulary: Set<String> = [
        "the", "is", "looking", "good", "can", "you", "tag", "heat", "map", "pride", "price", "of", "team", "we",
        "use", "super", "base", "for", "data", "pia", "said", "hi", "could", "do", "that", "paid", "a", "with",
    ]
    static func speller(_ dictionary: [String], screen: [String] = []) -> TermSpeller {
        TermSpeller(dictionary: dictionary, screen: screen) { vocabulary.contains($0.lowercased()) }
    }

    @Test func fixesAMisheardDictionaryWord() {
        let result = Self.speller(["heatmap"]).apply("The hitmap is looking good.")
        #expect(result.text == "The heatmap is looking good.")
        #expect(result.fixes == 1)
    }

    @Test func joinsWordsSpokenApartIntoADictionaryTerm() {
        #expect(Self.speller(["heatmap"]).apply("The heat map is looking good.").text == "The heatmap is looking good.")
        #expect(Self.speller(["Supabase"]).apply("We use super base for data.").text == "We use Supabase for data.")
    }

    @Test func turnsASoundAlikeNameIntoTheDictionaryName() {
        #expect(Self.speller(["Priya"]).apply("Can you tag Pia?").text == "Can you tag Priya?")
        // At a sentence start a capital says nothing: leave it.
        #expect(Self.speller(["Priya"]).apply("Pia said hi.").text == "Pia said hi.")
    }

    @Test func neverTurnsRealWordsIntoTerms() {
        let text = "The pride of the team is the price we paid."
        #expect(Self.speller(["Priya", "heatmap"], screen: ["Priya"]).apply(text).text == text)
        #expect(Self.speller(["Kudu"]).apply("We could do that.").text == "We could do that.")
    }

    @Test func usesScreenTermsForUnknownWordsAndInnerCapitals() {
        let speller = Self.speller([], screen: ["Siddharth", "GitHub"])
        #expect(speller.apply("Thanks Sidarth, push it to github.").text == "Thanks Siddharth, push it to GitHub.")
    }
}

@Suite struct FileNameStageTests {
    func apply(_ text: String) -> String { FileNameStage().apply(text, context: TextContext()) }

    @Test func writesSpokenFileNames() {
        #expect(apply("Okay, so fix the use case dot ts.") == "Okay, so fix the useCase.ts.")
        #expect(apply("open user service dot ts") == "open userService.ts")
        #expect(apply("check package dot json and the api client dot py") == "check package.json and the apiClient.py")
        #expect(apply("I attached the report dot pdf") == "I attached the report.pdf")
    }

    @Test func knowsConventionalNames() {
        #expect(apply("update the read me dot md") == "update the README.md")
    }

    @Test func leavesOtherDotsAlone() {
        #expect(apply("connect the dots on the map") == "connect the dots on the map")
        #expect(apply("polka dot dress") == "polka dot dress")
    }
}

@Suite struct MentionTests {
    @Test func mentionsOutsideChatAppsToo() {
        #expect(MentionStage().apply("Can you tag Priya?", context: TextContext()) == "Can you @Priya?")
    }

    @Test func skipsWordsThatAreNotNames() {
        let context = TextContext(category: .workMessage)
        #expect(MentionStage().apply("Can I tag along?", context: context) == "Can I tag along?")
        #expect(MentionStage().apply("Please tag me on it", context: context) == "Please tag me on it")
    }
}

@Suite struct CommandSelectionTests {
    @Test func editingInstructionsNeedASelection() {
        #expect(CommandOutput.needsSelection("Fix this"))
        #expect(CommandOutput.needsSelection("make it more formal"))
        #expect(CommandOutput.needsSelection("Translate that to Hindi."))
        #expect(CommandOutput.needsSelection("fix the grammar"))
    }

    @Test func draftsDoNot() {
        #expect(!CommandOutput.needsSelection("write a polite reply saying I'll be late"))
        #expect(!CommandOutput.needsSelection("translate good morning to Hindi"))
        #expect(!CommandOutput.needsSelection("draft a tweet about our launch"))
    }

    @Test func dropsAnEchoedPrompt() {
        #expect(CommandOutput.clean("Write the text now and return only the text.", instruction: "Fix this") == "")
        #expect(CommandOutput.clean("Fix this", instruction: "Fix this") == "")
        #expect(CommandOutput.clean("Request: say hi\nHi there!") == "Hi there!")
    }
}

@Suite struct LearningFromEditsTests {
    @Test func learnsAMisheardWordFixedInPlace() {
        let learned = CorrectionLearner.learn(
            inserted: "And inside the page the hitmap is looking good",
            edited: "And inside the page the heatmap is looking good")
        #expect(learned == [LearnedCorrection(heard: "hitmap", written: "heatmap")])
    }

    @Test func learnsAMisheardName() {
        let learned = CorrectionLearner.learn(inserted: "Can you tag Pia on this?", edited: "Can you tag Priya on this?")
        #expect(learned == [LearnedCorrection(heard: "pia", written: "Priya")])
    }
}
