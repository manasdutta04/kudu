import Foundation
import Testing
@testable import KuduCore

@Suite struct StyleStageTests {
    let stage = StyleStage()

    @Test func formalLeavesTextAlone() {
        let context = TextContext(category: .workMessage, style: .formal)
        #expect(stage.apply("Sounds good. See you at 3.", context: context) == "Sounds good. See you at 3.")
    }

    @Test func casualDropsFinalPeriodOfShortMessages() {
        let context = TextContext(category: .personalMessage, style: .casual)
        #expect(stage.apply("Sounds good. See you at 3.", context: context) == "Sounds good. See you at 3")
        #expect(stage.apply("Are you coming?", context: context) == "Are you coming?")
    }

    @Test func casualKeepsPeriodOutsideMessagesAndForLongText() {
        #expect(stage.apply("See you at 3.", context: TextContext(category: .email, style: .casual)) == "See you at 3.")
        let long = Array(repeating: "word", count: 40).joined(separator: " ") + "."
        #expect(stage.apply(long, context: TextContext(category: .workMessage, style: .casual)) == long)
    }

    @Test func veryCasualLowercasesButKeepsNames() {
        let context = TextContext(category: .personalMessage, style: .veryCasual)
        #expect(stage.apply("Sounds good. I pushed it to GitHub. NASA called.", context: context)
            == "sounds good. I pushed it to GitHub. NASA called")
    }

    @Test func codeIsNeverStyled() {
        let context = TextContext(category: .code, style: .veryCasual)
        #expect(stage.apply("Run the tests.", context: context) == "Run the tests.")
    }
}

@Suite struct FormattingNeedTests {
    @Test func plainSentencesSkipTheModel() {
        #expect(!FormattingNeed.shouldFormat("Can we move the stand-up to Wednesday?", strength: .light))
        #expect(!FormattingNeed.shouldFormat("Hey team, the build is green and I pushed the fix for the login bug.", strength: .light))
    }

    @Test func structureOrLengthUsesIt() {
        #expect(FormattingNeed.shouldFormat("first the api second the tests", strength: .light))
        #expect(FormattingNeed.shouldFormat("budget is twelve thousand dollars", strength: .light))
        let long = Array(repeating: "word", count: 45).joined(separator: " ")
        #expect(FormattingNeed.shouldFormat(long, strength: .light))
    }

    @Test func verbatimNeverUsesItAndPolishedUsesItMore() {
        #expect(!FormattingNeed.shouldFormat("first the api second the tests and third the release notes", strength: .verbatim))
        #expect(FormattingNeed.shouldFormat("so i think we should probably maybe ship it", strength: .polished))
    }
}

@Suite struct SentenceChunkerTests {
    @Test func splitsSentencesKeepingPunctuation() {
        #expect(SentenceChunker.sentences("Hi there. How are you? Great!") == ["Hi there.", "How are you?", "Great!"])
        #expect(SentenceChunker.sentences("Meet at 3.30 tomorrow. Ok") == ["Meet at 3.30 tomorrow.", "Ok"])
    }

    @Test func groupsIntoChunksAndHoldsBackTheLastSentence() {
        let text = "The build is green and the fix is in. Please review it today if you can. Thanks a lot."
        let chunks = SentenceChunker.chunks(text, minimumWords: 12)
        #expect(chunks.stable == ["The build is green and the fix is in. Please review it today if you can."])
        #expect(chunks.rest == "Thanks a lot.")
    }

    @Test func samePrefixGivesSameChunks() {
        let early = SentenceChunker.chunks("One two three four five six seven. Eight nine ten eleven twelve thirteen. And more")
        let later = SentenceChunker.chunks("One two three four five six seven. Eight nine ten eleven twelve thirteen. And more words here. Final")
        #expect(early.stable == Array(later.stable.prefix(early.stable.count)))
    }

    @Test func singleSentenceIsAllRest() {
        #expect(SentenceChunker.chunks("Just one sentence here").stable.isEmpty)
    }
}

@Suite struct CleanupSettingsDecodingTests {
    @Test func readsSettingsSavedByVersion03() throws {
        let off = try JSONDecoder().decode(CleanupSettings.self, from: Data(#"{"removeFillers":false,"smartFormatting":"off"}"#.utf8))
        #expect(off.editingStrength == .verbatim)
        #expect(off.removeFillers == false)
        let on = try JSONDecoder().decode(CleanupSettings.self, from: Data(#"{"smartFormatting":"longDictations"}"#.utf8))
        #expect(on.editingStrength == .light)
        #expect(on.styles == StyleSettings())
    }

    @Test func roundTrips() throws {
        var settings = CleanupSettings()
        settings.editingStrength = .polished
        settings.styles.email = .casual
        let decoded = try JSONDecoder().decode(CleanupSettings.self, from: JSONEncoder().encode(settings))
        #expect(decoded == settings)
    }
}

@Suite struct MessageLayoutTests {
    let stage = MessageLayout()

    @Test func formalGreetingAndSignOffEverywhere() {
        let text = "Dear Priya thanks for the contract. We are ready to sign. Thanks again for your patience. Best regards manas."
        #expect(stage.apply(text, context: TextContext())
            == "Dear Priya,\nthanks for the contract. We are ready to sign. Thanks again for your patience.\n\nBest regards,\nManas")
    }

    @Test func casualOnesOnlyInEmail() {
        let text = "Hi Sam, the deck is attached. Thanks, Manas."
        #expect(stage.apply(text, context: TextContext(category: .workMessage)) == text)
        #expect(stage.apply(text, context: TextContext(category: .email)) == "Hi Sam,\nthe deck is attached.\n\nThanks,\nManas")
    }

    @Test func ordinarySentencesAreUntouched() {
        let text = "Hey team, quick update: the build is green. Thanks."
        #expect(stage.apply(text, context: TextContext(category: .email)) == text)
    }
}

@Suite struct MentionAndCodeGuardTests {
    @Test func tagsBecomeMentionsInChatOnly() {
        let text = "Auth flow is done, tag priya, she should review it."
        #expect(MentionStage().apply(text, context: TextContext(category: .workMessage)) == "Auth flow is done, @Priya, she should review it.")
        #expect(MentionStage().apply(text, context: TextContext(category: .email)) == text)
    }

    @Test func codeGuardAcceptsSymbolsAndCamelCase() {
        #expect(FormattingGuard.acceptCode(original: "git commit dash m fixed the auth retry loop",
                                           formatted: "git commit -m \"fixed the auth retry loop\"") != nil)
        #expect(FormattingGuard.acceptCode(original: "open user service dot ts and rename get user to fetch user",
                                           formatted: "Open userService.ts and rename getUser to fetchUser.") != nil)
    }

    @Test func codeGuardRejectsInventedCode() {
        #expect(FormattingGuard.acceptCode(original: "rename get user to fetch user",
                                           formatted: "function fetchUser(id) { return db.users.find(id) }") == nil)
    }

    @Test func codeCuesAndProseCues() {
        #expect(FormattingNeed.shouldFormatCode("git commit dash m fixed it", strength: .light))
        #expect(!FormattingNeed.shouldFormatCode("yes", strength: .light))
        #expect(FormattingNeed.shouldFormat("add two kilo atta", strength: .light))
        #expect(FormattingNeed.shouldFormat("quick update three things first design", strength: .light))
    }
}
