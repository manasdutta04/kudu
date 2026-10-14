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
    /// The microphone to record from (its Core Audio UID); nil follows the system default.
    @Published public var microphoneUID: String? {
        didSet {
            UserDefaults.standard.set(microphoneUID, forKey: Keys.microphone)
            recorder.setInputDevice(uid: microphoneUID)
        }
    }
    public var microphones: [AudioInputDevice] { AudioDevices.inputs() }

    /// Learn names and terms from the corrections made after a dictation.
    @Published public var learnFromCorrections: Bool {
        didSet {
            UserDefaults.standard.set(learnFromCorrections, forKey: Keys.learnFromCorrections)
            if !learnFromCorrections { editObserver.cancel() }
        }
    }

    @Published public var soundsEnabled: Bool {
        didSet {
            sounds.isEnabled = soundsEnabled
            UserDefaults.standard.set(soundsEnabled, forKey: Keys.soundsEnabled)
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
    /// Daily usage totals (no text) for the Insights page.
    public let ledger: UsageLedger
    /// Learns spellings from the corrections the user makes right after a dictation.
    private let editObserver = EditObserver()
    private let formatter = SmartFormatter()
    /// Live voice levels, shared with the menu-bar icon.
    public var levels: OverlayModel { overlay.model }
    /// Whether this Mac can run smart formatting (macOS 26+ with Apple Intelligence).
    public var smartFormattingAvailable: Bool { formatter.isAvailable }
    private var recordingStartedAt: ContinuousClock.Instant?
    private var transcriptionTask: Task<Void, Never>?
    /// Transcribes and formats phrases while the user is still speaking.
    private var streaming: StreamingSession?
    /// Where the current dictation is going, captured when it starts.
    private var target: Target?

    private struct Target {
        var bundleID: String?
        var context: TextContext
    }
    private var lastShortTapAt: ContinuousClock.Instant?
    private var ignoreNextRelease = false
    private let doubleTapWindow: Duration = .milliseconds(400)

    private let log = Logger(subsystem: "app.kudu", category: "dictation")
    private let signposter = OSSignposter(subsystem: "app.kudu", category: .pointsOfInterest)

    /// Holds shorter than this are treated as accidental taps.
    private let minimumDuration: Duration = .milliseconds(250)

    private enum Keys {
        static let trigger = "triggerKey"
        static let soundsEnabled = "soundsEnabled"
        static let recordingMode = "recordingMode"
        static let microphone = "microphoneUID"
        static let learnFromCorrections = "learnFromCorrections"
        static let preventEmojiPicker = "preventEmojiPicker"
    }

    public init(store: UserDataStore = UserDataStore(), history: HistoryStore = HistoryStore(), ledger: UsageLedger? = nil) {
        self.store = store
        self.history = history
        self.ledger = ledger ?? UsageLedger(backfillFrom: history)
        let defaults = UserDefaults.standard
        let trigger = defaults.string(forKey: Keys.trigger).flatMap(TriggerKey.init(rawValue:)) ?? .fn
        self.trigger = trigger
        // The always-warm microphone option was removed: the mic is open only while dictating.
        defaults.removeObject(forKey: "keepMicWarm")
        self.preventEmojiPicker = defaults.object(forKey: Keys.preventEmojiPicker) as? Bool ?? true
        self.recordingMode = defaults.string(forKey: Keys.recordingMode)
            .flatMap(RecordingMode.init(rawValue:)) ?? .holdOrDoubleTap
        self.soundsEnabled = defaults.object(forKey: Keys.soundsEnabled) as? Bool ?? true
        self.microphoneUID = defaults.string(forKey: Keys.microphone)
        self.learnFromCorrections = defaults.object(forKey: Keys.learnFromCorrections) as? Bool ?? true
        self.hotkey = HotkeyMonitor(trigger: trigger)
        sounds.isEnabled = soundsEnabled
        hotkey.suppressFn = preventEmojiPicker
        hotkey.onEvent = { [weak self] event in self?.handle(event) }
        editObserver.onLearned = { [weak self] corrections in self?.remember(corrections) }
        let overlayModel = overlay.model
        recorder.onLevel = { level in
            Task { @MainActor in overlayModel.push(rms: level) }
        }
        // The start sound plays when the microphone is actually live, so speaking on the
        // tone never loses the first word (the mic is off between dictations).
        recorder.onLive = { [weak self] in
            Task { @MainActor in
                guard let self, self.isRecording else { return }
                self.sounds.playStart()
            }
        }
    }

    /// Call once at launch, and again after permissions change.
    public func activate() {
        if Permissions.microphoneGranted {
            if microphoneUID != nil {
                recorder.setInputDevice(uid: microphoneUID)
            } else {
                recorder.prepare()
            }
        }
        hotkeyActive = Permissions.accessibilityGranted && hotkey.start()
        updateFnGuard()
        if case .missing = modelState, ParakeetEngine.isDownloaded {
            Task { await loadModel() }
        }
        if store.cleanup.editingStrength != .verbatim { formatter.prewarm() }
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

    /// Saves learned spellings to the dictionary: a new entry, or one more way of hearing an
    /// existing spelling.
    private func remember(_ corrections: [LearnedCorrection]) {
        for correction in corrections {
            if let index = store.dictionary.firstIndex(where: { $0.written == correction.written }) {
                if !store.dictionary[index].heard.contains(where: { $0.lowercased() == correction.heard }) {
                    store.dictionary[index].heard.append(correction.heard)
                }
            } else {
                store.dictionary.append(DictionaryEntry(heard: [correction.heard], written: correction.written, learned: true))
            }
        }
        ledger.recordLearned(corrections.count)
        let names = corrections.map { "“\($0.written)”" }.joined(separator: ", ")
        log.info("Learned \(corrections.count) spelling(s)")
        overlay.flash("✦ Learned \(names)", for: .seconds(2.5))
    }

    /// Types the most recent result at the cursor again (⌃⌘V, or the menu).
    public func pasteLastTranscript() {
        guard let last = history.records.first else { return }
        reinsert(last)
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
        case .commandPressed:
            beginCommand()
        case .commandReleased:
            finishCommand()
        case .pasteLast:
            pasteLastTranscript()
        }
    }

    // MARK: - Command mode

    /// Whether the current recording is a Command mode instruction.
    @Published public private(set) var isCommandMode = false
    /// Text selected when the command started (read through Accessibility at once).
    private var commandSelection: String?
    private let commands = CommandEngine()
    public var commandModeAvailable: Bool { commands.isAvailable }

    /// fn+⌃ went down. If plain dictation already started a moment earlier (fn first), the
    /// recording simply becomes the instruction.
    private func beginCommand() {
        guard !isHandsFree else { return }
        if !isRecording { beginRecording() }
        guard isRecording else { return }
        isCommandMode = true
        streaming?.cancel()
        streaming = nil
        overlay.model.isCommand = true
        commandSelection = SelectionReader.accessibilitySelectionNow()
    }

    private func finishCommand() {
        guard isRecording, isCommandMode else { return }
        let samples = recorder.stop()
        isRecording = false
        hotkey.isDictating = false
        isCommandMode = false
        let selectionAtStart = commandSelection
        commandSelection = nil

        if let started = recordingStartedAt, ContinuousClock.now - started < minimumDuration {
            overlay.hide()
            return
        }
        if AudioLevels.isSilent(samples) {
            overlay.flash("No sound from the microphone", for: .seconds(3))
            return
        }
        guard commands.isAvailable else {
            overlay.flash("Command mode needs Apple Intelligence", for: .seconds(3))
            return
        }

        overlay.show(.transcribing)
        overlay.model.isCommand = true
        isTranscribing = true
        sounds.playStop()
        let releasedAt = ContinuousClock.now
        let bundleID = target?.bundleID
        let category = target?.context.category ?? .general
        let audioSeconds = Double(samples.count) / AudioRecorder.sampleRate

        transcriptionTask = Task { [engine, commands] in
            defer {
                self.transcriptionTask = nil
                self.isTranscribing = false
            }
            do {
                let spoken = try await engine.transcribe(samples)
                let instruction = FillerRemover().apply(spoken, context: TextContext())
                guard !instruction.isEmpty else {
                    self.overlay.flash("Didn't catch that")
                    return
                }
                // Fall back to copying the selection now that the keys are up.
                var selection = selectionAtStart
                if selection?.isEmpty ?? true { selection = await SelectionReader.selectedText() }
                let result = try await commands.run(instruction: instruction, selection: selection, category: category)
                self.overlay.hide()
                await self.inserter.insert(result)
                let latency = Int((ContinuousClock.now - releasedAt) / .milliseconds(1))
                self.lastTranscript = result
                self.lastLatencyMs = latency
                let record = DictationRecord(
                    raw: instruction, text: result, audioSeconds: audioSeconds, latencyMs: latency,
                    bundleID: bundleID, formatted: true, command: true
                )
                self.history.append(record)
                self.ledger.record(record)
                self.log.info("Command: \(audioSeconds, format: .fixed(precision: 1))s instruction → \(latency) ms (selection: \(selection?.isEmpty == false))")
            } catch {
                self.log.error("Command failed: \(error.localizedDescription)")
                self.overlay.flash((error as? CommandEngine.Failure)?.errorDescription ?? "Command failed", for: .seconds(3))
            }
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
        editObserver.cancel()
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
        startStreaming()
    }

    /// Captures the destination (app, style, text around the cursor) and starts working on
    /// the dictation while it is being spoken.
    private func startStreaming() {
        let bundleID = NSWorkspace.shared.frontmostApplication?.bundleIdentifier
        let category = AppContext.category(for: bundleID)
        let settings = store.cleanup
        let fieldBefore = inserter.textBeforeCursor()
        let context = TextContext(
            category: category, precedingCharacter: fieldBefore?.last, style: settings.styles.style(for: category)
        )
        target = Target(bundleID: bundleID, context: context)
        let session = StreamingSession(
            recorder: recorder, engine: engine, formatter: formatter,
            settings: StreamingSession.Settings(
                before: TextPipeline.beforeFormatting(settings: settings, dictionary: store.dictionary),
                request: FormatRequest(
                    text: "", category: category, style: context.style, strength: settings.editingStrength,
                    terms: store.dictionary.map(\.written), fieldBefore: fieldBefore
                ),
                // Code editors and terminals get the rule cleanup only: no prose formatting.
                formattingAvailable: formatter.isAvailable && category != .code && settings.editingStrength != .verbatim
            )
        )
        streaming = session
        session.start()
    }

    private func cancelRecording() {
        guard isRecording else { return }
        _ = recorder.stop()
        isRecording = false
        hotkey.isDictating = false
        isCommandMode = false
        commandSelection = nil
        streaming?.cancel()
        streaming = nil
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

        // Stop the background work unless this turns into a real dictation below.
        defer {
            streaming?.cancel()
            streaming = nil
        }

        if let started = recordingStartedAt, ContinuousClock.now - started < minimumDuration {
            overlay.hide()
            return
        }

        if AudioLevels.isSilent(samples) {
            log.error("Recording was pure silence; microphone muted or unavailable?")
            overlay.flash("No sound from the microphone", for: .seconds(3))
            return
        }

        guard let session = streaming, let target else { return }
        streaming = nil // handed to the task below; the deferred cancel no longer applies
        overlay.show(.transcribing)
        isTranscribing = true
        sounds.playStop()
        let releasedAt = ContinuousClock.now
        let settings = store.cleanup
        // Read the clipboard now, on the main thread, and only if a snippet can use it.
        let clipboardText = store.snippets.contains { $0.expansion.contains("{clipboard}") }
            ? NSPasteboard.general.string(forType: .string) : nil
        let snippets = SnippetStage(snippets: store.snippets, clipboard: { clipboardText })
        let after = TextPipeline.afterFormatting(settings: settings)
        let audioSeconds = Double(samples.count) / AudioRecorder.sampleRate
        let dictionary = DictionaryStage(entries: store.dictionary)
        let knownSpellings = Set(store.dictionary.map(\.written))

        transcriptionTask = Task { [signposter] in
            defer {
                self.transcriptionTask = nil
                self.isTranscribing = false
            }
            let interval = signposter.beginInterval("dictation")
            do {
                let result = try await session.finish(samples: samples)
                var text = result.text
                var formatted = result.formatted
                if let expansion = snippets.match(result.raw) {
                    text = expansion
                    formatted = false
                }
                text = after.process(text, context: target.context)

                self.overlay.hide()
                if !text.isEmpty {
                    let method = await self.inserter.insert(text)
                    self.log.debug("Inserted via \(method.rawValue)")
                }
                let latency = Int((ContinuousClock.now - releasedAt) / .milliseconds(1))
                self.lastTranscript = text
                self.lastLatencyMs = latency
                if !text.isEmpty {
                    let record = DictationRecord(
                        raw: result.raw, text: text, audioSeconds: audioSeconds, latencyMs: latency,
                        bundleID: target.bundleID, formatted: formatted,
                        dictionaryFixes: dictionary.corrections(in: result.raw)
                    )
                    self.history.append(record)
                    self.ledger.record(record)
                    if self.learnFromCorrections {
                        self.editObserver.watch(inserted: text, known: knownSpellings)
                    }
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
