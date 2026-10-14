import AppKit
import KuduCore
import os

/// Runs the whole hold → speak → release → text-at-cursor loop.
@MainActor
public final class DictationController: ObservableObject {
    public enum ModelState: Equatable {
        case missing
        case downloading(Double)
        case loading
        case ready
        case failed(String)
    }

    @Published public private(set) var modelState: ModelState = .missing
    @Published public private(set) var isRecording = false
    /// Recording continues without holding the key (double-tap to start, tap to stop).
    @Published public private(set) var isHandsFree = false
    @Published public private(set) var lastTranscript = ""
    /// Key-up → text inserted, in milliseconds, for the most recent dictation.
    @Published public private(set) var lastLatencyMs: Int?
    @Published public private(set) var hotkeyActive = false
    /// Most recent first, capped at `historyLimit`.
    @Published public private(set) var history: [HistoryItem] = []
    public var historyLimit = 25

    @Published public var trigger: TriggerKey {
        didSet {
            hotkey.trigger = trigger
            UserDefaults.standard.set(trigger.rawValue, forKey: Keys.trigger)
        }
    }
    /// While fn is the dictation key, hide fn presses from the system so taps and double-taps
    /// don't open the emoji picker. fn behaves normally again as soon as Kudu quits.
    @Published public var preventEmojiPicker: Bool {
        didSet {
            UserDefaults.standard.set(preventEmojiPicker, forKey: Keys.preventEmojiPicker)
            hotkey.suppressFn = preventEmojiPicker
        }
    }
    @Published public var recordingMode: RecordingMode {
        didSet { UserDefaults.standard.set(recordingMode.rawValue, forKey: Keys.recordingMode) }
    }
    @Published public var soundsEnabled: Bool {
        didSet {
            sounds.isEnabled = soundsEnabled
            UserDefaults.standard.set(soundsEnabled, forKey: Keys.soundsEnabled)
        }
    }
    @Published public var keepMicWarm: Bool {
        didSet {
            recorder.keepWarm = keepMicWarm
            UserDefaults.standard.set(keepMicWarm, forKey: Keys.keepMicWarm)
        }
    }

    private let hotkey: HotkeyMonitor
    private let recorder = AudioRecorder()
    private let engine = ParakeetEngine()
    private let inserter = TextInserter()
    private let overlay = OverlayPanel()
    private let sounds = SoundCues()
    public let store: UserDataStore
    private var recordingStartedAt: ContinuousClock.Instant?
    private var transcriptionTask: Task<Void, Never>?
    private var lastShortTapAt: ContinuousClock.Instant?
    private var ignoreNextRelease = false
    private let doubleTapWindow: Duration = .milliseconds(400)

    private let log = Logger(subsystem: "app.kudu", category: "dictation")
    private let signposter = OSSignposter(subsystem: "app.kudu", category: .pointsOfInterest)

    /// Holds shorter than this are treated as accidental taps.
    private let minimumDuration: Duration = .milliseconds(250)

    private enum Keys {
        static let trigger = "triggerKey"
        static let keepMicWarm = "keepMicWarm"
        static let soundsEnabled = "soundsEnabled"
        static let recordingMode = "recordingMode"
        static let preventEmojiPicker = "preventEmojiPicker"
    }

    public init(store: UserDataStore = UserDataStore()) {
        self.store = store
        let defaults = UserDefaults.standard
        let trigger = defaults.string(forKey: Keys.trigger).flatMap(TriggerKey.init(rawValue:)) ?? .fn
        self.trigger = trigger
        self.keepMicWarm = defaults.bool(forKey: Keys.keepMicWarm)
        self.preventEmojiPicker = defaults.object(forKey: Keys.preventEmojiPicker) as? Bool ?? true
        self.recordingMode = defaults.string(forKey: Keys.recordingMode)
            .flatMap(RecordingMode.init(rawValue:)) ?? .holdOrDoubleTap
        self.soundsEnabled = defaults.object(forKey: Keys.soundsEnabled) as? Bool ?? true
        self.hotkey = HotkeyMonitor(trigger: trigger)
        sounds.isEnabled = soundsEnabled
        hotkey.suppressFn = preventEmojiPicker
        // Version 0.2 paused the system 🌐 setting instead; macOS only applies that at login,
        // so it didn't help. Give back the user's original setting if it is still paused.
        FnKeyGuard().disengage()
        hotkey.onEvent = { [weak self] event in self?.handle(event) }
        let overlayModel = overlay.model
        recorder.onLevel = { level in
            Task { @MainActor in overlayModel.push(level: level) }
        }
    }

    /// Call once at launch, and again after permissions change.
    public func activate() {
        if Permissions.microphoneGranted {
            recorder.keepWarm = keepMicWarm
            recorder.prepare()
        }
        hotkeyActive = Permissions.accessibilityGranted && hotkey.start()
        if case .missing = modelState, ParakeetEngine.isDownloaded {
            Task { await loadModel() }
        }
    }

    /// Call when the app is quitting.
    public func shutdown() {
        hotkey.stop()
    }

    // MARK: - Model

    public func downloadModel() async {
        if case .downloading = modelState { return }
        modelState = .downloading(0)
        do {
            try await ParakeetEngine.download { [weak self] fraction in
                Task { @MainActor in
                    guard let self, case .downloading = self.modelState else { return }
                    self.modelState = .downloading(fraction)
                }
            }
            await loadModel()
        } catch {
            log.error("Model download failed: \(error.localizedDescription)")
            modelState = .failed(error.localizedDescription)
        }
    }

    private func loadModel() async {
        modelState = .loading
        do {
            try await engine.load()
            modelState = .ready
        } catch {
            log.error("Model load failed: \(error.localizedDescription)")
            modelState = .failed(error.localizedDescription)
        }
    }

    // MARK: - History

    /// Types a previous result at the cursor again (e.g. after it went to the wrong window).
    public func reinsert(_ item: HistoryItem, raw: Bool = false) {
        Task { await inserter.insert(raw ? item.raw : item.text) }
    }

    public func copyToClipboard(_ item: HistoryItem, raw: Bool = false) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(raw ? item.raw : item.text, forType: .string)
    }

    public func clearHistory() {
        history.removeAll()
        lastTranscript = ""
        lastLatencyMs = nil
    }

    // MARK: - Hotkey flow

    private func handle(_ event: HotkeyEvent) {
        switch event {
        case .pressed:
            if isHandsFree {
                // Tap while locked: stop and transcribe. Ignore this press's release.
                isHandsFree = false
                ignoreNextRelease = true
                finishRecording()
            } else {
                beginRecording()
            }
        case .released:
            if ignoreNextRelease {
                ignoreNextRelease = false
            } else if !isHandsFree {
                handleRelease()
            }
        case .cancelled:
            isHandsFree = false
            ignoreNextRelease = false
            cancelRecording()
        }
    }

    /// A long hold always finishes on release (push-to-talk). What a short tap does depends
    /// on the recording mode: lock hands-free right away, lock on the second tap, or nothing.
    private func handleRelease() {
        guard isRecording, let started = recordingStartedAt else { return }
        let now = ContinuousClock.now
        guard now - started < minimumDuration else {
            finishRecording()
            return
        }
        switch recordingMode {
        case .tapToToggle:
            lockHandsFree()
        case .holdOrDoubleTap:
            if let lastTap = lastShortTapAt, now - lastTap < doubleTapWindow {
                lastShortTapAt = nil
                lockHandsFree()
            } else {
                lastShortTapAt = now
                finishRecording()
            }
        case .holdOnly:
            finishRecording()
        }
    }

    private func lockHandsFree() {
        isHandsFree = true
        overlay.model.isLocked = true
    }

    private func beginRecording() {
        guard !isRecording, transcriptionTask == nil else { return }
        guard modelState == .ready else {
            overlay.flash(modelState == .loading ? "Loading model…" : "Model not ready")
            return
        }
        do {
            try recorder.start()
        } catch {
            log.error("Could not start audio: \(error.localizedDescription)")
            overlay.flash("Mic unavailable")
            return
        }
        isRecording = true
        hotkey.isDictating = true
        recordingStartedAt = .now
        overlay.show(.recording)
        sounds.playStart()
    }

    private func cancelRecording() {
        guard isRecording else { return }
        _ = recorder.stop()
        isRecording = false
        hotkey.isDictating = false
        overlay.hide()
        // Only make a sound for a real dictation being cancelled, not a key chord.
        if let started = recordingStartedAt, ContinuousClock.now - started >= minimumDuration {
            sounds.playCancel()
        }
    }

    private func finishRecording() {
        guard isRecording else { return }
        let samples = recorder.stop()
        isRecording = false
        hotkey.isDictating = false

        if let started = recordingStartedAt, ContinuousClock.now - started < minimumDuration {
            overlay.hide()
            return
        }

        if AudioLevels.isSilent(samples) {
            log.error("Recording was pure silence; microphone muted or unavailable?")
            overlay.flash("No sound from the microphone", for: .seconds(3))
            return
        }

        overlay.show(.transcribing)
        sounds.playStop()
        let releasedAt = ContinuousClock.now
        let category = AppContext.frontmostCategory()
        let precedingCharacter = inserter.precedingCharacter()
        // Read the clipboard now, on the main thread, and only if a snippet can use it.
        let clipboardText = store.snippets.contains { $0.expansion.contains("{clipboard}") }
            ? NSPasteboard.general.string(forType: .string) : nil
        let pipeline = TextPipeline.standard(
            settings: store.cleanup,
            dictionary: store.dictionary,
            snippets: store.snippets,
            clipboard: { clipboardText }
        )

        transcriptionTask = Task { [engine, signposter] in
            defer { self.transcriptionTask = nil }
            let interval = signposter.beginInterval("dictation")
            do {
                let raw = try await engine.transcribe(samples)
                let text = pipeline.process(
                    raw, context: TextContext(category: category, precedingCharacter: precedingCharacter)
                )
                self.overlay.hide()
                if !text.isEmpty {
                    let method = await self.inserter.insert(text)
                    self.log.debug("Inserted via \(method.rawValue)")
                }
                self.lastTranscript = text
                let latency = Int((ContinuousClock.now - releasedAt) / .milliseconds(1))
                self.lastLatencyMs = latency
                if !text.isEmpty {
                    self.history.insert(HistoryItem(date: .now, raw: raw, text: text, latencyMs: latency), at: 0)
                    if self.history.count > self.historyLimit { self.history.removeLast() }
                }
                let audioSeconds = Double(samples.count) / AudioRecorder.sampleRate
                self.log.info(
                    "Dictation: \(audioSeconds, format: .fixed(precision: 1))s audio → \(self.lastLatencyMs ?? 0) ms"
                )
            } catch {
                self.log.error("Transcription failed: \(error.localizedDescription)")
                self.overlay.flash("Couldn't transcribe")
            }
            signposter.endInterval("dictation", interval)
        }
    }
}
