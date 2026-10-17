import Foundation
import KuduCore
import os

/// What Kudu does when it notices a meeting starting.
public enum MeetingAutoStart: String, CaseIterable, Identifiable, Sendable {
    /// Nothing: notes start only with the shortcut, the menu or the Meetings page.
    case off
    /// Asks with a small prompt in the corner of the screen.
    case ask
    /// Starts taking notes, and stops when the meeting ends.
    case automatic

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .off: "Do nothing"
        case .ask: "Ask me"
        case .automatic: "Start taking notes"
        }
    }
}

/// Takes notes for one meeting at a time: records the microphone (and the Mac's sound when
/// allowed), transcribes it piece by piece as the meeting goes, saves the transcript as it
/// grows, and writes the notes when the meeting ends. The audio itself is never saved.
@MainActor
public final class MeetingRecorder: ObservableObject {
    public enum State: Equatable {
        case idle
        case recording(Meeting)
        /// Writing the notes for a finished meeting: `done` of `total` steps.
        case writing(UUID, done: Int, total: Int)
    }

    @Published public private(set) var state: State = .idle
    /// The latest lines of the meeting being recorded, for the live view.
    @Published public private(set) var live: [MeetingSegment] = []
    /// The last meeting heard nothing from the Mac's sound, though it was on: macOS is
    /// probably not allowing Kudu to record it.
    @Published public internal(set) var systemAudioBlocked = false

    public let store: MeetingStore
    /// The current meeting was started because a meeting app was detected, so it also stops
    /// when that meeting ends.
    public private(set) var startedFromDetection = false

    private let engine: ParakeetEngine
    private let writer = MeetingNotesWriter()
    private var capture: MeetingCapture?
    private var assembler = TranscriptAssembler(hasSystemAudio: false)
    private var pump: Task<Void, Never>?
    private var stopping = false
    private var dictionary: () -> [DictionaryEntry]
    private let log = Logger(subsystem: "app.kudu", category: "meetings")

    /// Long meetings stop by themselves after this long.
    public static let maximumLength: TimeInterval = 4 * 60 * 60
    /// Shorter recordings aren't kept.
    static let minimumLength: TimeInterval = 10

    init(engine: ParakeetEngine, store: MeetingStore = MeetingStore(), dictionary: @escaping () -> [DictionaryEntry]) {
        self.engine = engine
        self.store = store
        self.dictionary = dictionary
    }

    public var isRecording: Bool {
        if case .recording = state { return true }
        return false
    }

    public var current: Meeting? {
        if case let .recording(meeting) = state { return meeting }
        return nil
    }

    /// Whether notes are written by Apple Intelligence (otherwise by Kudu's rules).
    public var notesUseAppleIntelligence: Bool { writer.usesModel }

    /// Starts recording a meeting. Throws if the microphone can't be opened.
    func start(app: String?, fromDetection: Bool, includeSystemAudio: Bool, microphoneUID: String?) throws {
        guard case .idle = state else { return }
        let capture = MeetingCapture()
        try capture.start(includeSystemAudio: includeSystemAudio, microphoneUID: microphoneUID)
        let sources: [AudioSource] = capture.hasSystemAudio ? [.mic, .system] : [.mic]
        let meeting = Meeting(title: Meeting.defaultTitle(app: app), app: app, sources: sources)
        self.capture = capture
        assembler = TranscriptAssembler(hasSystemAudio: capture.hasSystemAudio)
        startedFromDetection = fromDetection
        live = []
        store.save(meeting)
        state = .recording(meeting)
        log.info("Meeting notes started (app: \(app ?? "none", privacy: .public), others' audio: \(capture.hasSystemAudio))")
        pump = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(700))
                guard !Task.isCancelled, let self else { return }
                await self.transcribeReady()
                if let meeting = self.current, meeting.duration() >= Self.maximumLength {
                    self.log.info("Meeting reached the maximum length; stopping")
                    Task { await self.stop() }
                    return
                }
            }
        }
    }

    /// Silences the microphone while the user dictates into another app.
    func setMicMuted(_ muted: Bool) {
        capture?.setMicMuted(muted)
    }

    /// Stops recording, transcribes what's left, and writes the notes. Returns the meeting,
    /// or nil if it was too short to keep.
    @discardableResult
    public func stop() async -> Meeting? {
        guard case .recording = state, let capture, !stopping else { return nil }
        stopping = true
        defer { stopping = false }
        // Let a piece being transcribed finish, so it's saved once and in order.
        let running = pump
        pump = nil
        running?.cancel()
        await running?.value
        guard case var .recording(meeting) = state else { return nil }
        capture.stop()
        self.capture = nil
        state = .writing(meeting.id, done: 0, total: 1)
        await transcribe(capture.takePieces(final: true), into: &meeting)
        save(assembler.finish(), into: &meeting)
        meeting.ended = .now
        if capture.systemAudioSeemsBlocked {
            log.error("The Mac's sound was silent throughout; System Audio Recording is probably not allowed")
            meeting.sources = [.mic]
            systemAudioBlocked = true
        } else if capture.hasSystemAudio {
            systemAudioBlocked = false
        }
        live = []

        if meeting.duration() < Self.minimumLength || meeting.words == 0 {
            log.info("Meeting too short or silent; not kept")
            store.delete(meeting.id)
            state = .idle
            return nil
        }
        await writeNotes(&meeting)
        state = .idle
        return meeting
    }

    /// Writes the notes for a saved meeting again.
    public func regenerateNotes(_ id: UUID) async {
        guard case .idle = state, var meeting = store.meeting(id) else { return }
        await writeNotes(&meeting)
        state = .idle
    }

    /// Finishes meetings left recording or half-written when Kudu last quit: keeps their
    /// transcript and writes their notes.
    func recoverInterrupted() async {
        for var meeting in store.interrupted {
            guard case .idle = state else { return }
            let segments = store.segments(of: meeting.id)
            if segments.isEmpty {
                store.delete(meeting.id)
                continue
            }
            meeting.ended = meeting.ended ?? meeting.started.addingTimeInterval(segments.map(\.end).max() ?? 0)
            meeting.words = segments.map { WordCount.count($0.text) }.reduce(0, +)
            log.info("Finishing an interrupted meeting")
            await writeNotes(&meeting)
            state = .idle
        }
    }

    // MARK: - Private

    private func writeNotes(_ meeting: inout Meeting) async {
        meeting.status = .writing
        store.save(meeting)
        let id = meeting.id
        state = .writing(id, done: 0, total: 1)
        let segments = store.segments(of: id)
        let (title, notes) = await writer.write(segments, heardOthers: meeting.heardOthers) { [weak self] done, total in
            self?.state = .writing(id, done: done, total: total)
        }
        if let title, !title.isEmpty { meeting.title = title }
        meeting.notes = notes
        meeting.status = .done
        store.save(meeting)
    }

    private func transcribeReady() async {
        guard case var .recording(meeting) = state, let capture else { return }
        let pieces = capture.takePieces()
        await transcribe(pieces, into: &meeting)
        if let quietUntil = capture.systemQuietUntil() {
            save(assembler.systemHeard(until: quietUntil), into: &meeting)
        }
        // Still recording (stop() may have started meanwhile and owns the state now).
        if case let .recording(current) = state, current.id == meeting.id { state = .recording(meeting) }
    }

    private func transcribe(_ pieces: [MeetingCapture.Piece], into meeting: inout Meeting) async {
        let dictionary = DictionaryStage(entries: self.dictionary())
        for piece in pieces where SpeechChunker.hasSpeech(piece.samples) {
            do {
                let raw = try await engine.transcribe(piece.samples)
                let text = TextPipeline.basic.process(dictionary.apply(raw, context: TextContext()))
                guard WordCount.count(text) > 0 else { continue }
                let segment = MeetingSegment(start: piece.start, end: piece.end, source: piece.source, text: text)
                save(assembler.add(segment), into: &meeting)
                if piece.source == .system {
                    save(assembler.systemHeard(until: piece.end), into: &meeting)
                }
            } catch {
                log.error("Couldn't transcribe part of the meeting: \(error.localizedDescription, privacy: .public)")
            }
        }
    }

    private func save(_ segments: [MeetingSegment], into meeting: inout Meeting) {
        guard !segments.isEmpty else { return }
        store.append(segments, to: meeting.id)
        meeting.words += segments.map { WordCount.count($0.text) }.reduce(0, +)
        store.save(meeting)
        live = Array((live + segments).suffix(6))
    }
}
