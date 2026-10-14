import Testing
@testable import KuduCore

@Suite struct CorrectionLearnerTests {
    @Test func learnsAMisspelledName() {
        let learned = CorrectionLearner.learn(inserted: "Thanks kudoo team, see you soon.", edited: "Thanks Kudu team, see you soon.")
        #expect(learned == [LearnedCorrection(heard: "kudoo", written: "Kudu")])
    }

    @Test func learnsAMultiWordMishearing() {
        let learned = CorrectionLearner.learn(inserted: "I pushed it to git hub today.", edited: "I pushed it to GitHub today.")
        #expect(learned == [LearnedCorrection(heard: "git hub", written: "GitHub")])
    }

    @Test func learnsSoundAlikesThatAreSpelledDifferently() {
        let learned = CorrectionLearner.learn(inserted: "We use coo doo for dictation.", edited: "We use Kudu for dictation.")
        #expect(learned == [LearnedCorrection(heard: "coo doo", written: "Kudu")])
    }

    @Test func learnsProperNounCapitalizationMidSentence() {
        let learned = CorrectionLearner.learn(inserted: "Please ask priya about it.", edited: "Please ask Priya about it.")
        #expect(learned == [LearnedCorrection(heard: "priya", written: "Priya")])
    }

    @Test func ignoresCapitalizationAtSentenceStart() {
        #expect(CorrectionLearner.learn(inserted: "sounds good to me.", edited: "Sounds good to me.").isEmpty)
    }

    @Test func ignoresRewordingAndEverydayWords() {
        #expect(CorrectionLearner.learn(inserted: "Send it to the team.", edited: "Send it to a team.").isEmpty)
        #expect(CorrectionLearner.learn(inserted: "Please send it today.", edited: "Please email it today.").isEmpty)
    }

    @Test func ignoresRewrites() {
        #expect(CorrectionLearner.learn(inserted: "Can we meet tomorrow at noon?", edited: "Lunch on Friday works better for me.").isEmpty)
    }

    @Test func ignoresNumbersAndKnownWords() {
        #expect(CorrectionLearner.learn(inserted: "It costs 40 dollars.", edited: "It costs 45 dollars.").isEmpty)
        #expect(CorrectionLearner.learn(inserted: "Ask kudoo now.", edited: "Ask Kudu now.", known: ["kudu"]).isEmpty)
    }

    @Test func phoneticKeysMatchSoundAlikes() {
        #expect(CorrectionLearner.phoneticKey("coodoo") == CorrectionLearner.phoneticKey("kudu"))
        #expect(CorrectionLearner.soundsAlike("priyah", "Priya"))
        #expect(!CorrectionLearner.soundsAlike("send", "email"))
    }
}
