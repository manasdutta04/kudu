import Testing
@testable import KuduCore

@Suite struct TimeNormalizerTests {
    let stage = TimeNormalizer()

    @Test(arguments: [
        ("Can we move the stand-up to Wednesday at 10.30?", "Can we move the stand-up to Wednesday at 10:30?"),
        ("Lunch is 12.15 pm.", "Lunch is 12:15 pm."),
        ("Free from 9.00 to 11.45.", "Free from 9:00 to 11:45."),
        ("Call me around 7.05 a.m.", "Call me around 7:05 a.m."),
    ])
    func rewritesTimes(input: String, expected: String) {
        #expect(stage.apply(input, context: TextContext()) == expected)
    }

    @Test(arguments: [
        "Upgrade to version 2.10.4 today.",
        "Pi is roughly 3.14.",
        "Revenue grew 12.50 percent.",
        "We hit 24.99 dollars.",
    ])
    func leavesNumbersAlone(text: String) {
        #expect(stage.apply(text, context: TextContext()) == text)
    }
}
