import Testing
@testable import KuduCore

@Suite struct FormattingGuardTests {
    @Test func acceptsPunctuationAndCasing() {
        let raw = "hey team quick update the build is green i pushed the fix for the login bug"
        let formatted = "Hey team, quick update — the build is green. I pushed the fix for the login bug."
        #expect(FormattingGuard.accept(original: raw, formatted: formatted) == formatted)
    }

    @Test func acceptsListsAndNumbers() {
        let raw = "the plan is first we finish the api second we write tests and third we ship on friday at twenty five past ten"
        let formatted = "The plan is:\n1. We finish the API.\n2. We write tests.\n3. We ship on Friday at 10:25."
        #expect(FormattingGuard.accept(original: raw, formatted: formatted) != nil)
    }

    @Test func stripsWrappingQuotes() {
        #expect(FormattingGuard.accept(original: "ship it today please", formatted: "\"Ship it today, please.\"") == "Ship it today, please.")
    }

    @Test func rejectsAnAnswerInsteadOfFormatting() {
        let raw = "can you write a short and good comment for this post"
        let answer = "Great post! Thanks for sharing these insights — really useful for anyone starting out."
        #expect(FormattingGuard.accept(original: raw, formatted: answer) == nil)
    }

    @Test func rejectsCommentaryAndRewrites() {
        let raw = "let's meet tomorrow at noon to go over the launch plan"
        #expect(FormattingGuard.accept(original: raw, formatted: "Here is the formatted text: Let's meet tomorrow at noon to go over the launch plan.") == nil)
        #expect(FormattingGuard.accept(original: raw, formatted: "Meeting tomorrow, 12 pm: launch plan review.") == nil)
    }

    @Test func rejectsEmptyOutput() {
        #expect(FormattingGuard.accept(original: "hello there", formatted: "   ") == nil)
    }
}
