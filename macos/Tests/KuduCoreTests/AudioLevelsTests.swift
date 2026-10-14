import Testing
@testable import KuduCore

@Suite struct AudioLevelsTests {
    @Test func zerosAreSilent() {
        #expect(AudioLevels.isSilent([Float](repeating: 0, count: 16_000)))
        #expect(AudioLevels.isSilent([]))
    }

    @Test func quietSpeechIsNotSilent() {
        var samples = [Float](repeating: 0, count: 16_000)
        samples[8_000] = 0.002
        #expect(!AudioLevels.isSilent(samples))
    }
}
