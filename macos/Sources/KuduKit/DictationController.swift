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

    /// Dictation, Command mode and paste-last shortcuts, changeable in Settings.
    @Published public var shortcuts: ShortcutSet {
        didSet {
            hotkey.shortcuts = shortcuts
            if let data = try? JSONEncoder().encode(shortcuts) { UserDefaults.standard.set(data, forKey: Keys.shortcuts) }
            updateFnGuard()
        }
    }
    /// The dictation shortcut alone, for the quick picker in the menu.
    public var dictateShortcut: KeyShortcut {
        get { shortcuts.dictate }
        set {
            var updated = shortcuts
            updated.dictate = newValue
            // Command mode follows the dictation key when it was "dictation key + ⌃".
            if let command = shortcuts.command, command.modifiers == shortcuts.dictate.modifiers + [ModifierKey(.control)] {
                updated.command = KeyShortcut(newValue.modifiers + [ModifierKey(.control)])
            }
            shortcuts = updated
        }
    }
    /// Which shortcut is being recorded in Settings, and the keys held so far.
    @Published public private(set) var recordingShortcut: ShortcutSlot?
    @Published public private(set) var recordingPreview: [String] = []
    /// Why the last recorded shortcut wasn't accepted.
    @Published public private(set) var shortcutError: String?
    /// While fn is part of a shortcut, pause the system's 🌐 action and hide fn presses, so taps
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

    /// Looks for newer releases of Kudu (daily, can be switched off) and installs them.
    public let updates = UpdateChecker()

    /// The speech model to use (Kudu Tiny or Kudu Base). Changing it downloads or loads it.
    @Published public private(set) var speechModel: SpeechModel {
        didSet { UserDefaults.standard.set(speechModel.rawValue, forKey: Keys.speechModel) }
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
        static let shortcuts = "shortcuts"
        static let soundsEnabled = "soundsEnabled"
        static let recordingMode = "recordingMode"
        static let microphone = "microphoneUID"
        static let learnFromCorrections = "learnFromCorrections"
        static let speechModel = "speechModel"
        static let preventEmojiPicker = "preventEmojiPicker"
    }

    public init(store: UserDataStore = UserDataStore(), history: HistoryStore = HistoryStore(), ledger: UsageLedger? = nil) {
        self.store = store
        self.history = history
        self.ledger = ledger ?? UsageLedger(backfillFrom: history)
        let defaults = UserDefaults.standard
        let shortcuts = Self.loadShortcuts(defaults)
        self.shortcuts = shortcuts
        // The always-warm microphone option was removed: the mic is open only while dictating.
        defaults.removeObject(forKey: "keepMicWarm")
        self.preventEmojiPicker = defaults.object(forKey: Keys.preventEmojiPicker) as? Bool ?? true
        self.recordingMode = defaults.string(forKey: Keys.recordingMode)
            .flatMap(RecordingMode.init(rawValue:)) ?? .holdOrDoubleTap
        self.soundsEnabled = defaults.object(forKey: Keys.soundsEnabled) as? Bool ?? true
        self.microphoneUID = defaults.string(forKey: Keys.microphone)
        self.learnFromCorrections = defaults.object(forKey: Keys.learnFromCorrections) as? Bool ?? true
        self.speechModel = defaults.string(forKey: Keys.speechModel).flatMap(SpeechModel.init(rawValue:)) ?? .base
        self.hotkey = HotkeyMonitor(shortcuts: shortcuts)
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
        AppAccessibility.start()
        updates.start()
        if Permissions.microphoneGranted {
            if microphoneUID != nil {
                recorder.setInputDevice(uid: microphoneUID)
            } else {
                recorder.prepare()
            }
        }
        hotkeyActive = Permissions.accessibilityGranted && hotkey.start()
        updateFnGuard()
        if case .missing = modelState, speechModel.isDownloaded {
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
        if hotkeyActive && shortcuts.usesFn && preventEmojiPicker {
            fnGuard.engage()
        } else {
            // Also cleans up after a previous run that was killed while engaged.
            fnGuard.disengage()
        }
    }

    // MARK: - Shortcuts

    /// Saved shortcuts, or the dictation key chosen in earlier versions (fn, Right ⌘, Right ⌥).
    private static func loadShortcuts(_ defaults: UserDefaults) -> ShortcutSet {
        if let data = defaults.data(forKey: Keys.shortcuts),
           let saved = try? JSONDecoder().decode(ShortcutSet.self, from: data) {
            return saved
        }
        var shortcuts = ShortcutSet.defaults
        switch defaults.string(forKey: Keys.trigger) {
        case "rightCommand": shortcuts.dictate = .rightCommand
        case "rightOption": shortcuts.dictate = .rightOption
        default: break
        }
        if shortcuts.dictate != .fn { shortcuts.command = KeyShortcut(shortcuts.dictate.modifiers + [ModifierKey(.control)]) }
        return shortcuts
    }

    /// Records the next key combination as the shortcut for `slot`.
    public func recordShortcut(_ slot: ShortcutSlot) {
        hotkey.cancelRecording()
        shortcutError = nil
        recordingShortcut = slot
        recordingPreview = []
        hotkey.recordShortcut { [weak self] preview in
            self?.recordingPreview = preview
        } done: { [weak self] shortcut in
            guard let self else { return }
            self.recordingShortcut = nil
            self.recordingPreview = []
            guard let shortcut else { return }
            self.apply(shortcut, to: slot)
        }
    }

    public func cancelShortcutRecording() {
        hotkey.cancelRecording()
        recordingShortcut = nil
        recordingPreview = []
    }

    public func resetShortcuts() {
        shortcutError = nil
        shortcuts = .defaults
    }

    private func apply(_ shortcut: KeyShortcut, to slot: ShortcutSlot) {
        if slot != .pasteLast, !shortcut.isUsableForHolding(keyTypesText: KeyboardLayout.typesText) {
            shortcutError = "\(shortcut.displayName) types text, so it can't be held to talk. Add ⌃, ⌥ or ⌘, or use fn or an F-key."
            return
        }
        if slot == .pasteLast, shortcut.isModifierOnly {
            shortcutError = "Paste last transcript needs a key, like ⌃⌘V."
            return
        }
        var updated = shortcuts
        switch slot {
        case .dictate: updated.dictate = shortcut
        case .command: updated.command = shortcut
        case .pasteLast: updated.pasteLast = shortcut
        }
        if let problem = updated.problems.first {
            shortcutError = problem
            return
        }
        shortcuts = updated
    }

    // MARK: - Model

    /// Switches to `model`: loads it if it's on disk, otherwise downloads it first.
    public func chooseModel(_ model: SpeechModel) async {
        if case .downloading = modelState { return }
        speechModel = model
        if model.isDownloaded {
            await loadModel()
        } else {
            await downloadModel()
        }
    }

    /// Deletes downloaded speech models other than the one in use.
    public func removeUnusedModels() {
        for model in SpeechModel.allCases where model != speechModel {
            do { try model.remove() } catch { log.error("Couldn't remove \(model.rawValue): \(error.localizedDescription)") }
        }
        objectWillChange.send()
    }

    public func downloadModel() async {
        if case .downloading = modelState { return }
        modelState = .downloading(0)
        let model = speechModel
        do {
            try await model.download { [weak self] fraction in
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
            try await engine.load(speechModel)
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
        let before = store.dictionary
        for correction in corrections {
            if let index = store.dictionary.firstIndex(where: { $0.written == correction.written }) {
                if !store.dictionary[index].heard.contains(where: { $0.lowercased() == correction.heard }) {
                    store.dictionary[index].heard.append(correction.heard)
                }
            } else {
                store.dictionary.append(DictionaryEntry(heard: [correction.heard], written: correction.written, learned: true))
            }
        }
        let after = store.dictionary
        ledger.recordLearned(corrections.count)
        log.info("Learned \(corrections.count) spelling(s)")
        // The pill is busy while you dictate; the words are saved either way.
        guard !isRecording, !isTranscribing else { return }
        overlay.notifyLearned(corrections.map(\.written)) { [weak self] in
            guard let self, self.store.dictionary == after else { return }
            self.store.dictionary = before
            self.ledger.recordLearned(-corrections.count)
        }
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
                // "Fix this" with nothing selected: there's nothing to fix, so don't invent text.
                if selection?.isEmpty ?? true, CommandOutput.needsSelection(instruction) {
                    self.overlay.flash("Select the text first, then hold fn + ⌃", for: .seconds(3))
                    return
                }
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
                    terms: store.dictionary.map(\.written), fieldBefore: fieldBefore,
                    appName: NSWorkspace.shared.frontmostApplication?.localizedName
                ),
                formattingAvailable: formatter.isAvailable && settings.editingStrength != .verbatim
            )
        )
        streaming = session
        session.start()

        // Names and terms on screen, read in the background while the user speaks.
        screenTerms = []
        Task.detached(priority: .userInitiated) {
            let terms = ScreenContext.terms()
            await MainActor.run { [weak self, weak session] in
                guard let self, !terms.isEmpty else { return }
                self.screenTerms = terms
                session?.addTerms(terms)
                self.log.debug("Screen terms: \(terms.count)")
            }
        }
    }

    /// Names and terms visible when the current dictation started.
    private var screenTerms: [String] = []

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
                let spelled = Speller.make(dictionary: Array(knownSpellings), screen: self.screenTerms).apply(result.text)
                var text = spelled.text
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
                        dictionaryFixes: dictionary.corrections(in: result.raw) + spelled.fixes
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
