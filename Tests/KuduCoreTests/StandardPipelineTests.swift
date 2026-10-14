import Testing
@testable import KuduCore

@Suite struct StandardPipelineTests {
    let pipeline = TextPipeline.standard(
        settings: CleanupSettings(),
        dictionary: [DictionaryEntry(heard: ["kudoo"], written: "Kudu")],
        snippets: [Snippet(trigger: "my email", expansion: "me@example.com")]
    )

    @Test func endToEnd() {
        let raw = "Um, let's demo kudoo on Tuesday, no, Wednesday at 10.30. New line. Thanks."
        #expect(pipeline.process(raw) == "Let's demo Kudu on Wednesday at 10:30.\nThanks.")
    }

    @Test func snippet() {
        #expect(pipeline.process("My email.") == "me@example.com")
    }

    @Test func respectsDisabledStages() {
        var settings = CleanupSettings()
        settings.removeFillers = false
        let raw = "Um, hello."
        #expect(TextPipeline.standard(settings: settings).process(raw) == "Um, hello.")
    }

    @Test func codeContextContinuation() {
        let context = TextContext(category: .code, precedingCharacter: "x")
        #expect(pipeline.process("Uh, Run the tests.", context: context) == " run the tests")
    }
}
