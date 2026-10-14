import Foundation
import Testing
@testable import KuduCore

@Suite struct SnippetStageTests {
    let stage = SnippetStage(
        snippets: [
            Snippet(trigger: "my email", expansion: "me@example.com"),
            Snippet(trigger: "sign off", expansion: "Thanks,\nManas"),
            Snippet(trigger: "paste link", expansion: "Link: {clipboard}"),
        ],
        clipboard: { "https://example.com" }
    )

    @Test(arguments: ["My email.", "my email", "  My   Email! "])
    func expandsWholeUtterance(input: String) {
        #expect(stage.apply(input, context: TextContext()) == "me@example.com")
    }

    @Test func multiLineExpansion() {
        #expect(stage.apply("Sign off.", context: TextContext()) == "Thanks,\nManas")
    }

    @Test func clipboardPlaceholder() {
        #expect(stage.apply("Paste link.", context: TextContext()) == "Link: https://example.com")
    }

    @Test func triggerInsideSentenceIsLeftAlone() {
        let text = "Please send it to my email tomorrow."
        #expect(stage.apply(text, context: TextContext()) == text)
    }

    @Test func datePlaceholder() {
        let fixed = Date(timeIntervalSince1970: 0)
        let dated = SnippetStage(snippets: [Snippet(trigger: "today", expansion: "{date}")], now: { fixed })
        #expect(dated.apply("Today.", context: TextContext()) == fixed.formatted(date: .long, time: .omitted))
    }
}
