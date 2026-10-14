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
    /// Speech recognized, now cleaning up / formatting.
    @Published public private(set) var isTranscribing = false

    @Published public var trigger: TriggerKey {
        didSet {
            hotkey.trigger = trigger
            UserDefaults.standard.set(trigger.rawValue, forKey: Keys.trigger)
            updateFnGuard()
        }
    }
    /// While fn is the dictation key, pause the system's 🌐 action and hide fn presses, so taps
    /// and double-taps don't open the emoji picker. Restored as soon as Kudu quits.
    @Published public var preventEmojiPicker: Bool {
        didSet {
            UserDefaults.standard.set(preventEmojiPicker, forKey: Keys.preventEmojiPicker)
            hotkey.suppressFn = preventEmojiPicker
            updateFnGuard()
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
    private let fnGuard = FnKeyGuard()
    public let store: UserDataStore
    /// Dictation history and stats, saved on this Mac.
    public let history: HistoryStore
    private let formatter = SmartFormatter()
    /// Live voice levels, shared with the menu-bar icon.
    public var levels: OverlayModel { overlay.model }
    /// Whether this Mac can run smart formatting (macOS 26+ with Apple Intelligence).
    public var smartFormattingAvailable: Bool { formatter.isAvailable }
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

    public init(store: UserDataStore = UserDataStore(), history: HistoryStore = HistoryStore()) {
        self.store = store
        self.history = history
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
        hotkey.onEvent = { [weak self] event in self?.handle(event) }
        let overlayModel = overlay.model
        recorder.onLevel = { level in
            Task { @MainActor in overlayModel.push(rms: level) }
        }
    }

    /// Call once at launch, and again after permissions change.
    public func activate() {
        if Permissions.microphoneGranted {
            recorder.keepWarm = keepMicWarm
            recorder.prepare()
        }
        hotkeyActive = Permissions.accessibilityGranted && hotkey.start()
        updateFnGuard()
        if case .missing = modelState, ParakeetEngine.isDownloaded {
            Task { await loadModel() }
        }
        if store.cleanup.smartFormatting != .off { formatter.prewarm() }
    }

    /// Plays the overlay through every state with a synthetic voice-like signal, so its look
    /// can be checked without speaking. Launch with `--overlay-demo`.
    public func runOverlayDemo() {
        Task { @MainActor in
            let model = overlay.model
            overlay.show(.recording)
            let start = ContinuousClock.now
            var step = 0
            while ContinuousClock.now - start < .seconds(6) {
                if step == 150 { model.isLocked = true }
                // Syllable-like bursts riding on a slower phrase envelope.
                let t = Double(step) * 0.02
                let syllable = max(0, sin(t * 2 * .pi * 3.1)) * (0.6 + 0.4 * sin(t * 2 * .pi * 0.45))
                model.push(rms: Float(0.002 + 0.12 * syllable * syllable))
                step += 1
                try? await Task.sleep(for: .milliseconds(20))
            }
            overlay.show(.transcribing)
            try? await Task.sleep(for: .seconds(1.5))
            overlay.flash("No sound from the microphone", for: .seconds(2))
        }
    }

    /// Call when the app is quitting: gives the 🌐 key its own action back.
    public func shutdown() {
        fnGuard.disengage()
        hotkey.stop()
    }

    private func updateFnGuard() {
        if hotkeyActive && trigger == .fn && preventEmojiPicker {
            fnGuard.engage()
        } else {
            // Also cleans up after a previous run that was killed while engaged.
            fnGuard.disengage()
        }
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
    public func reinsert(_ record: DictationRecord, raw: Bool = false) {
        Task { await inserter.insert(raw ? record.raw : record.text) }
    }

    public func copyToClipboard(_ record: DictationRecord, raw: Bool = false) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(raw ? record.raw : record.text, forType: .string)
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
        isTranscribing = true
        sounds.playStop()
        let releasedAt = ContinuousClock.now
        let frontmost = NSWorkspace.shared.frontmostApplication?.bundleIdentifier
        let category = AppContext.category(for: frontmost)
        let context = TextContext(category: category, precedingCharacter: inserter.precedingCharacter())
        let settings = store.cleanup
        // Read the clipboard now, on the main thread, and only if a snippet can use it.
        let clipboardText = store.snippets.contains { $0.expansion.contains("{clipboard}") }
            ? NSPasteboard.general.string(forType: .string) : nil
        let snippets = SnippetStage(snippets: store.snippets, clipboard: { clipboardText })
        let before = TextPipeline.beforeFormatting(settings: settings, dictionary: store.dictionary)
        let after = TextPipeline.afterFormatting(settings: settings)
        let terms = store.dictionary.map(\.written)
        let audioSeconds = Double(samples.count) / AudioRecorder.sampleRate

        transcriptionTask = Task { [engine, formatter, signposter] in
            defer {
                self.transcriptionTask = nil
                self.isTranscribing = false
            }
            let interval = signposter.beginInterval("dictation")
            do {
                let raw = try await engine.transcribe(samples)
                var text: String
                var formatted = false
                if let expansion = snippets.match(raw) {
                    text = expansion
                } else {
                    text = before.process(raw, context: context)
                    // Code editors and terminals get the rule cleanup only: no prose formatting.
                    if category != .code, settings.smartFormatting.applies(toWordCount: WordCount.count(text)),
                       let result = await formatter.format(text, category: category, terms: terms) {
                        text = result
                        formatted = true
                    }
                }
                text = after.process(text, context: context)

                self.overlay.hide()
                if !text.isEmpty {
                    let method = await self.inserter.insert(text)
                    self.log.debug("Inserted via \(method.rawValue)")
                }
                let latency = Int((ContinuousClock.now - releasedAt) / .milliseconds(1))
                self.lastTranscript = text
                self.lastLatencyMs = latency
                if !text.isEmpty {
                    self.history.append(DictationRecord(
                        raw: raw, text: text, audioSeconds: audioSeconds, latencyMs: latency,
                        bundleID: frontmost, formatted: formatted
                    ))
                }
                self.log.info(
                    "Dictation: \(audioSeconds, format: .fixed(precision: 1))s audio → \(latency) ms (formatted: \(formatted))"
                )
            } catch {
                self.log.error("Transcription failed: \(error.localizedDescription)")
                self.overlay.flash("Couldn't transcribe")
            }
            signposter.endInterval("dictation", interval)
        }
    }
}
