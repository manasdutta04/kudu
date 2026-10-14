import Testing
@testable import KuduCore

@Suite struct CommandOutputTests {
    @Test(arguments: [
        ("Sure! Here's a more formal version:\nI will be late to the meeting.", "I will be late to the meeting."),
        ("Here is the translation: नमस्ते, आप कैसे हैं?", "नमस्ते, आप कैसे हैं?"),
        ("\"Thanks for the update.\"", "Thanks for the update."),
        ("```\nlet x = 1\n```", "let x = 1"),
        ("Thanks, I'll review it today.\n\nLet me know if you want any changes!", "Thanks, I'll review it today."),
        ("Dear, may I request the deck by tomorrow?", "May I request the deck by tomorrow?"),
    ])
    func strips(output: String, expected: String) {
        #expect(CommandOutput.clean(output) == expected)
    }

    @Test(arguments: ["Here we go again: the launch slipped to Friday.", "Ok so the launch slipped to Friday.", "Dear Priya, thanks for the update."])
    func leavesPlainTextAlone(text: String) {
        #expect(CommandOutput.clean(text) == text)
    }
}
