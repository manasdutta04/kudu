import Testing
@testable import KuduCore

@Suite struct TermExtractorTests {
    @Test func picksNamesHandlesAndCode() {
        let terms = TermExtractor.terms(from: [
            "Thanks Siddharth, I pushed the fix to GitHub. Ask @priya to review userService.ts and session_token.",
            "Meeting with Siddharth on Friday.",
        ])
        #expect(terms.first == "Siddharth") // seen twice
        #expect(Set(terms).isSuperset(of: ["GitHub", "priya", "userService.ts", "session_token"]))
    }

    @Test func skipsSentenceStartsCommonWordsAndLinks() {
        let terms = TermExtractor.terms(from: ["Today we shipped. The build is green. See https://example.com/path on Friday."])
        #expect(!terms.contains("Today"))
        #expect(!terms.contains("The"))
        #expect(!terms.contains("Friday"))
        #expect(!terms.contains { $0.contains("example") })
    }
}
