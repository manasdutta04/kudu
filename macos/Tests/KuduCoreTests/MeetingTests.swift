import Foundation
import Testing
@testable import KuduCore

@Suite struct SpeechChunkerTests {
    let rate = 16_000

    func speech(_ seconds: Double) -> [Float] {
        (0..<Int(seconds * Double(rate))).map { i in
            let x = Float(i)
            return sin(x * 0.05) * 0.2 * (1 + sin(x * 0.0007)) / 2 + 0.05
        }
    }

    func silence(_ seconds: Double) -> [Float] {
        (0..<Int(seconds * Double(rate))).map { Float(($0 * 7919) % 13) * 0.0001 }
    }

    @Test func cutsInThePauseAfterEnoughSpeech() throws {
        let audio = speech(5) + silence(0.6) + speech(2)
        let cut = try #require(SpeechChunker(rate: rate).nextCut(audio))
        let seconds = Double(cut) / Double(rate)
        #expect((5.1...5.5).contains(seconds))
    }

    @Test func waitsForTheMinimumLength() {
        #expect(SpeechChunker(rate: rate).nextCut(speech(2) + silence(0.6) + speech(1.5)) == nil)
    }

    @Test func ignoresShortBreaths() {
        #expect(SpeechChunker(rate: rate).nextCut(speech(5) + silence(0.15) + speech(2)) == nil)
    }

    @Test func cutsAtTheMaximumWithoutAPause() {
        let chunker = SpeechChunker(rate: rate)
        #expect(chunker.nextCut(speech(29)) == Int(28 * Double(rate)))
    }

    @Test func quietRoomHasNoSpeech() {
        #expect(!SpeechChunker.hasSpeech(silence(2)))
        #expect(SpeechChunker.hasSpeech(speech(1)))
    }
}

@Suite struct TranscriptAssemblerTests {
    func seg(_ start: Double, _ end: Double, _ source: AudioSource, _ text: String) -> MeetingSegment {
        MeetingSegment(start: start, end: end, source: source, text: text)
    }

    @Test func micOnlyIsReleasedAtOnce() {
        var assembler = TranscriptAssembler(hasSystemAudio: false)
        #expect(assembler.add(seg(0, 5, .mic, "Hello everyone")).count == 1)
    }

    @Test func dropsTheSpeakersHeardByTheMicrophone() {
        var assembler = TranscriptAssembler(hasSystemAudio: true)
        #expect(assembler.add(seg(10, 16, .mic, "so the launch moves to next Tuesday because of the review")).isEmpty)
        let released = assembler.add(seg(9.6, 15.8, .system, "So the launch moves to next Tuesday, because of the review."))
        #expect(released.map(\.source) == [.system])
        #expect(assembler.systemHeard(until: 30).isEmpty)
    }

    @Test func keepsWhatTheUserSaid() {
        var assembler = TranscriptAssembler(hasSystemAudio: true)
        _ = assembler.add(seg(0, 6, .system, "Can you send the deck before the call?"))
        #expect(assembler.add(seg(6.5, 9, .mic, "Sure, I'll send it this afternoon.")).isEmpty)
        let released = assembler.systemHeard(until: 14)
        #expect(released.map(\.text) == ["Sure, I'll send it this afternoon."])
    }

    @Test func holdsTheMicUntilTheOthersAreTranscribedPastIt() {
        var assembler = TranscriptAssembler(hasSystemAudio: true)
        #expect(assembler.add(seg(0, 5, .mic, "Let's start with the budget")).isEmpty)
        #expect(assembler.systemHeard(until: 8).isEmpty)
        #expect(assembler.systemHeard(until: 9).count == 1)
    }

    @Test func finishReleasesTheRest() {
        var assembler = TranscriptAssembler(hasSystemAudio: true)
        _ = assembler.add(seg(0, 5, .mic, "Thanks everyone, talk soon"))
        #expect(assembler.finish().count == 1)
    }

    @Test func shortRepliesMustMatchCompletely() {
        let heard = Set(["yes", "that", "works"])
        #expect(TranscriptAssembler.overlap(["yes"], heard) == 1)
        #expect(TranscriptAssembler.overlap(["no", "way"], heard) == 0)
    }
}

@Suite struct MeetingDetectorTests {
    let zoom = MeetingApp(name: "Zoom", kind: .meeting)
    let chrome = MeetingApp(name: "Google Chrome", kind: .browser)
    let t0 = Date(timeIntervalSince1970: 1_000)

    @Test func startsAfterAFewSecondsOfMicUse() {
        var detector = MeetingDetector()
        #expect(detector.observe([zoom], at: t0) == nil)
        #expect(detector.observe([zoom], at: t0 + 2) == nil)
        #expect(detector.observe([zoom], at: t0 + 3) == .started(zoom))
        #expect(detector.observe([zoom], at: t0 + 5) == nil)
    }

    @Test func aQuickMicTestIsNotAMeeting() {
        var detector = MeetingDetector()
        _ = detector.observe([zoom], at: t0)
        _ = detector.observe([], at: t0 + 2)
        #expect(detector.observe([zoom], at: t0 + 4) == nil)
    }

    @Test func endsOnlyAfterTheMicIsReleasedForAWhile() {
        var detector = MeetingDetector()
        _ = detector.observe([zoom], at: t0)
        _ = detector.observe([zoom], at: t0 + 3)
        #expect(detector.observe([], at: t0 + 10) == nil)
        #expect(detector.observe([zoom], at: t0 + 12) == nil)
        #expect(detector.observe([], at: t0 + 30) == nil)
        #expect(detector.observe([], at: t0 + 32) == .ended)
        #expect(detector.active == nil)
    }

    @Test func prefersAMeetingAppOverABrowser() {
        var detector = MeetingDetector()
        _ = detector.observe([chrome, zoom], at: t0)
        #expect(detector.observe([chrome, zoom], at: t0 + 3) == .started(zoom))
    }

    @Test func recognisesHelperProcesses() {
        #expect(MeetingApps.app(bundleID: "us.zoom.xos")?.name == "Zoom")
        #expect(MeetingApps.app(bundleID: "com.google.Chrome.helper")?.kind == .browser)
        #expect(MeetingApps.app(bundleID: "com.microsoft.teams2")?.name == "Microsoft Teams")
        #expect(MeetingApps.app(bundleID: "app.kudu.Kudu") == nil)
    }
}

@Suite struct MeetingNotesTests {
    let reply = """
        TITLE: Launch plan review
        SUMMARY: The team reviewed the launch plan. The date moves to next Tuesday.
        KEY POINTS:
        - The review found two blocking bugs
        - Marketing is ready
        DECISIONS:
        - Launch moves to Tuesday
        ACTION ITEMS:
        - Priya will fix the login bug by Friday
        - You: send the deck to Sakshi
        """

    @Test func readsTheNotesFormat() throws {
        let (title, notes) = try #require(MeetingNotesPrompt.parse(reply, generator: .appleIntelligence))
        #expect(title == "Launch plan review")
        #expect(notes.summary == "The team reviewed the launch plan. The date moves to next Tuesday.")
        #expect(notes.keyPoints == ["The review found two blocking bugs", "Marketing is ready"])
        #expect(notes.decisions == ["Launch moves to Tuesday"])
        #expect(notes.actions.count == 2)
    }

    @Test func toleratesMarkdownAndNone() throws {
        let markdown = """
            **Title:** Weekly sync
            ## Summary
            Quick status round.
            ## Key points
            1. Hiring is on track
            * Budget approved
            ## Decisions
            None
            ## Action items
            - [ ] Manas books the room
            """
        let (title, notes) = try #require(MeetingNotesPrompt.parse(markdown, generator: .kuduWriter))
        #expect(title == "Weekly sync")
        #expect(notes.summary == "Quick status round.")
        #expect(notes.keyPoints == ["Hiring is on track", "Budget approved"])
        #expect(notes.decisions.isEmpty)
        #expect(notes.actions == ["Manas books the room"])
    }

    @Test func rejectsAReplyThatIsntNotes() {
        #expect(MeetingNotesPrompt.parse("Sure! Here is what they talked about: the launch.", generator: .kuduWriter) == nil)
    }

    @Test func rejectsTheTranscriptPlayedBack() {
        let long = MeetingNotes(summary: String(repeating: "word ", count: 400), generator: .kuduWriter)
        #expect(!MeetingNotesPrompt.isUsable(long, transcriptWords: 300))
        let short = MeetingNotes(summary: "A short summary.", keyPoints: ["One"], generator: .kuduWriter)
        #expect(MeetingNotesPrompt.isUsable(short, transcriptWords: 300))
    }

    @Test func splitsLongTranscriptsAtLines() {
        let line = String(repeating: "word ", count: 100).trimmingCharacters(in: .whitespaces)
        let transcript = Array(repeating: "Others: \(line)", count: 30).joined(separator: "\n")
        let parts = MeetingNotesPrompt.parts(transcript, maxWords: 1_400)
        #expect(parts.count == 3)
        #expect(parts.allSatisfy { WordCount.count($0) <= 1_400 })
        #expect(MeetingNotesPrompt.parts("You: short").count == 1)
    }

    @Test func extractiveNotesFindTasksAndDecisions() {
        let segments = [
            MeetingSegment(start: 0, end: 10, source: .system, text: "Thanks for joining the planning call today everyone. We looked at the numbers from last quarter and they were better than expected across regions."),
            MeetingSegment(start: 10, end: 20, source: .mic, text: "We agreed to launch on Tuesday next week. I'll send the deck to Sakshi by Friday."),
            MeetingSegment(start: 20, end: 25, source: .system, text: "Okay. Can you also update the pricing page?"),
        ]
        let notes = MeetingNotesPrompt.extractive(segments)
        #expect(notes.generator == .extractive)
        #expect(notes.decisions == ["We agreed to launch on Tuesday next week."])
        #expect(notes.actions == ["I'll send the deck to Sakshi by Friday.", "Can you also update the pricing page?"])
        #expect(!notes.summary.isEmpty)
        #expect(!notes.keyPoints.isEmpty)
    }
}

@Suite struct MeetingFormatTests {
    @Test func clockAndLength() {
        #expect(MeetingFormat.clock(65) == "1:05")
        #expect(MeetingFormat.clock(3_729) == "1:02:09")
        #expect(MeetingFormat.length(30) == "under a minute")
        #expect(MeetingFormat.length(34 * 60) == "34 min")
        #expect(MeetingFormat.length(65 * 60) == "1 h 5 min")
    }

    @Test func joinsTurnsFromTheSameSpeaker() {
        let segments = [
            MeetingSegment(start: 0, end: 5, source: .system, text: "Hi there."),
            MeetingSegment(start: 5, end: 9, source: .system, text: "How was the trip?"),
            MeetingSegment(start: 9, end: 12, source: .mic, text: "Great, thanks."),
        ]
        #expect(MeetingFormat.transcriptText(segments, labelled: true) == "Others: Hi there. How was the trip?\nYou: Great, thanks.")
        #expect(MeetingFormat.transcriptText(segments, labelled: false) == "Hi there. How was the trip?\nGreat, thanks.")
    }

    @Test func markdownHasNotesAndTranscript() {
        var meeting = Meeting(title: "Weekly sync", started: Date(timeIntervalSince1970: 0), app: "Zoom", sources: [.mic, .system])
        meeting.ended = Date(timeIntervalSince1970: 600)
        meeting.notes = MeetingNotes(summary: "Short.", actions: ["Send the deck"], generator: .appleIntelligence)
        let text = MeetingFormat.markdown(meeting, segments: [MeetingSegment(start: 3, end: 6, source: .mic, text: "Hello.")], dateText: "Today")
        #expect(text.contains("# Weekly sync"))
        #expect(text.contains("Today · 10 min · Zoom"))
        #expect(text.contains("- [ ] Send the deck"))
        #expect(text.contains("**You** (0:03): Hello."))
    }

    @Test func meetingRoundTripsThroughJSON() throws {
        var meeting = Meeting(title: "Sync", started: Date(timeIntervalSince1970: 1_000), app: nil, sources: [.mic])
        meeting.notes = MeetingNotes(summary: "S", generator: .extractive)
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let data = try encoder.encode(meeting)
        #expect(String(decoding: data, as: UTF8.self).contains("\"generator\":\"extractive\""))
        #expect(try decoder.decode(Meeting.self, from: data) == meeting)
    }
}
