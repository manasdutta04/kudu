import Testing
@testable import KuduCore

@Suite struct DictionaryStageTests {
    let stage = DictionaryStage(entries: [
        DictionaryEntry(heard: ["kudoo", "coo doo"], written: "Kudu"),
        DictionaryEntry(heard: ["git hub"], written: "GitHub"),
        DictionaryEntry(heard: ["post gress", "postgres"], written: "PostgreSQL"),
    ])

    @Test(arguments: [
        ("I pushed kudoo to git hub.", "I pushed Kudu to GitHub."),
        ("The coo doo app is fast.", "The Kudu app is fast."),
        ("Coo-doo rocks.", "Kudu rocks."),
        ("we moved to postgres", "we moved to PostgreSQL"),
        ("open github now", "open GitHub now"),
    ])
    func replaces(input: String, expected: String) {
        #expect(stage.apply(input, context: TextContext()) == expected)
    }

    @Test func matchesWholeWordsOnly() {
        #expect(stage.apply("Kudoos to the team", context: TextContext()) == "Kudoos to the team")
    }

    @Test func emptyDictionaryIsNoOp() {
        #expect(DictionaryStage(entries: []).apply("anything", context: TextContext()) == "anything")
    }
}
