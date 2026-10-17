import AppKit
@preconcurrency import CoreFoundation
import CoreGraphics
import KuduCore
import os

/// What the user did with their shortcuts.
public enum HotkeyEvent: Sendable {
    /// The dictation shortcut went down.
    case pressed
    /// The dictation shortcut was released after a normal hold.
    case released
    /// The hold turned out to be part of a shortcut (another key was pressed), or Escape was hit.
    case cancelled
    /// The Command mode shortcut went down (or the dictation hold became one).
    case commandPressed
    /// The Command mode shortcut was released.
    case commandReleased
    /// The paste-last shortcut (⌃⌘V by default): paste the last transcript again.
    case pasteLast
    /// The meeting notes shortcut (⌃⌥N by default): start or stop taking notes.
    case meeting
}

/// Watches the keyboard with a CGEventTap and recognizes the user's shortcuts (`ShortcutSet`,
/// matched by `ShortcutMatcher` in KuduCore, the same logic as on Windows and Linux).
///
/// The tap is active (not listen-only) so it can swallow events: Escape while dictating, the
/// key of a key-based shortcut, everything while a new shortcut is being recorded, and — when
/// fn is part of a shortcut — the fn press and release themselves. macOS turns an fn
/// press/release pair it sees into the 🌐 action (emoji picker, input source switch). Letting
/// the press through but not the release is worse: the system is left thinking fn is held,
/// and the *next* fn event completes a "tap". So both halves are hidden from the system.
/// That requires Accessibility permission, which Kudu needs anyway to insert text.
///
/// The tap runs on its own high-priority thread. If it ran on the main thread, any moment the
/// UI is busy would make macOS time the tap out and let a fn release through to the system.
/// Decisions are made on the tap thread; events are delivered to `onEvent` on the main thread.
public final class HotkeyMonitor: @unchecked Sendable {
    private struct State {
        var matcher: ShortcutMatcher
        /// Recording a new shortcut from Settings: keys go here instead of the matcher.
        var recorder: ShortcutRecorder?
        /// The physical fn key is down. Tracked from its own key events, because other events
        /// carry the fn flag for arrows and F-keys too.
        var fnDown = false
        /// Hide fn from the rest of the system while it is part of a shortcut.
        var suppressFn = true
        /// fn was combined with a real key (fn+arrow, fn+F…): the user wants a system shortcut,
        /// so stop hiding fn for the rest of this press.
        var fnCombo = false
        /// Events whose fn flag was removed during the current press (for diagnostics).
        var strippedCount = 0

        var hidesFn: Bool { (matcher.shortcuts.usesFn && suppressFn) || recorder != nil }
        /// fn is physically held as a lone key and must stay invisible to the system.
        var isHidingFn: Bool { fnDown && !fnCombo && hidesFn }
    }

    /// What the tap callback should do with an event.
    fileprivate enum Disposition {
        case pass
        case swallow
        /// Pass the event on without the fn flag.
        case stripFn
    }

    private let state: OSAllocatedUnfairLock<State>
    /// Called on the main thread.
    @MainActor public var onEvent: ((HotkeyEvent) -> Void)?
    /// While recording: the keys held so far, then the result (nil when cancelled).
    @MainActor private var onRecordingPreview: (([String]) -> Void)?
    @MainActor private var onRecorded: ((KeyShortcut?) -> Void)?

    public var shortcuts: ShortcutSet {
        get { state.withLock { $0.matcher.shortcuts } }
        set { state.withLock { $0.matcher.shortcuts = newValue } }
    }

    /// When fn is part of a shortcut, keep the system from seeing fn presses (no emoji picker).
    public var suppressFn: Bool {
        get { state.withLock { $0.suppressFn } }
        set { state.withLock { $0.suppressFn = newValue } }
    }

    /// Set by the owner while a dictation is in progress, so Escape can be swallowed.
    public var isDictating: Bool {
        get { state.withLock { $0.matcher.isDictating } }
        set { state.withLock { $0.matcher.isDictating = newValue } }
    }

    private static let fnKeyCode: Int64 = 63
    private let log = Logger(subsystem: "app.kudu", category: "hotkey")

    // Owned by start()/stop() on the main thread and read by the tap thread after start.
    nonisolated(unsafe) private var tap: CFMachPort?
    nonisolated(unsafe) private var tapRunLoop: CFRunLoop?

    public init(shortcuts: ShortcutSet) {
        state = OSAllocatedUnfairLock(initialState: State(matcher: ShortcutMatcher(shortcuts: shortcuts)))
    }

    /// The next key combination pressed is recorded instead of matched (and hidden from other
    /// apps). `preview` shows the keys held so far; `done` gets the result, or nil if cancelled.
    @MainActor
    public func recordShortcut(preview: @escaping ([String]) -> Void, done: @escaping (KeyShortcut?) -> Void) {
        onRecordingPreview = preview
        onRecorded = done
        state.withLock { $0.recorder = ShortcutRecorder() }
    }

    @MainActor
    public func cancelRecording() {
        let wasRecording = state.withLock { state -> Bool in
            defer { state.recorder = nil }
            return state.recorder != nil
        }
        if wasRecording { finishRecording(nil) }
    }

    @MainActor
    private func finishRecording(_ shortcut: KeyShortcut?) {
        var shortcut = shortcut
        if let code = shortcut?.keyCode { shortcut?.keyLabel = KeyboardLayout.label(for: code) }
        let done = onRecorded
        onRecorded = nil
        onRecordingPreview = nil
        done?(shortcut)
    }

    /// Returns false when the tap could not be created (usually: Accessibility not granted yet).
    @MainActor @discardableResult
    public func start() -> Bool {
        guard tap == nil else { return true }
        // Every event type: keyboard, mouse, scroll and trackpad gestures all carry modifier
        // flags, and any of them can tell the system fn went down and up (a 🌐 tap).
        let mask = CGEventMask.max
        let refcon = Unmanaged.passUnretained(self).toOpaque()
        // Prefer the HID-level tap: it sees fn before the rest of the system does. Fall back to
        // the session tap if the HID tap can't be created.
        let created = [CGEventTapLocation.cghidEventTap, .cgSessionEventTap].lazy.compactMap { location in
            CGEvent.tapCreate(
                tap: location,
                place: .headInsertEventTap,
                options: .defaultTap,
                eventsOfInterest: mask,
                callback: hotkeyTapCallback,
                userInfo: refcon
            )
        }.first
        guard let tap = created else {
            log.error("CGEvent.tapCreate failed; Accessibility permission missing?")
            return false
        }
        self.tap = tap

        let ready = DispatchSemaphore(value: 0)
        let thread = Thread { [weak self] in
            guard let self, let source = CFMachPortCreateRunLoopSource(nil, tap, 0) else {
                ready.signal()
                return
            }
            let runLoop = CFRunLoopGetCurrent()
            self.tapRunLoop = runLoop
            CFRunLoopAddSource(runLoop, source, .commonModes)
            CGEvent.tapEnable(tap: tap, enable: true)
            ready.signal()
            CFRunLoopRun()
        }
        thread.name = "app.kudu.hotkey"
        thread.qualityOfService = .userInteractive
        thread.start()
        ready.wait()
        return true
    }

    @MainActor
    public func stop() {
        if let tap {
            CGEvent.tapEnable(tap: tap, enable: false)
            CFMachPortInvalidate(tap)
        }
        if let tapRunLoop { CFRunLoopStop(tapRunLoop) }
        tap = nil
        tapRunLoop = nil
        state.withLock { state in
            state.fnDown = false
            state.matcher.shortcuts = state.matcher.shortcuts // forgets any hold in progress
        }
    }

    /// The physical modifier keys held, with their sides, from an event's flags. The
    /// device-dependent bits tell left from right; fn comes from its own key events.
    static func heldModifiers(_ flags: CGEventFlags, fnDown: Bool) -> [ModifierKey] {
        let raw = flags.rawValue
        var held: [ModifierKey] = fnDown ? [ModifierKey(.fn)] : []
        let pairs: [(ModifierKind, CGEventFlags, UInt64, UInt64)] = [
            (.control, .maskControl, 0x0001, 0x2000),
            (.option, .maskAlternate, 0x0020, 0x0040),
            (.shift, .maskShift, 0x0002, 0x0004),
            (.command, .maskCommand, 0x0008, 0x0010),
        ]
        for (kind, mask, leftBit, rightBit) in pairs where flags.contains(mask) {
            let left = raw & leftBit != 0
            let right = raw & rightBit != 0
            if left || !right { held.append(ModifierKey(kind, side: .left)) }
            if right { held.append(ModifierKey(kind, side: .right)) }
        }
        return held
    }

    /// Runs on the tap thread for every event and must return quickly.
    fileprivate func handle(type: CGEventType, keyCode: Int64, flags: CGEventFlags, isRepeat: Bool) -> Disposition {
        let now = ProcessInfo.processInfo.systemUptime
        switch type {
        case .tapDisabledByTimeout, .tapDisabledByUserInput:
            log.info("Event tap was disabled (\(type.rawValue)); re-enabling")
            if let tap { CGEvent.tapEnable(tap: tap, enable: true) }
            return .pass

        case .flagsChanged:
            return handleFlagsChanged(keyCode: keyCode, flags: flags, now: now)

        case .keyDown:
            let (disposition, events, recorded) = state.withLock { state -> (Disposition, [ShortcutEvent], ShortcutRecorder.Result?) in
                if state.fnDown { state.fnCombo = true }
                if var recorder = state.recorder {
                    let result = recorder.keyDown(Int(keyCode), label: nil)
                    state.recorder = nil
                    return (.swallow, [], result)
                }
                let outcome = state.matcher.keyDown(Int(keyCode), isRepeat: isRepeat, at: now)
                return (outcome.swallow ? .swallow : .pass, outcome.events, nil)
            }
            emit(events)
            if let recorded { emitRecording(recorded, preview: nil) }
            return disposition

        case .keyUp:
            let disposition = state.withLock { state -> Disposition in
                if state.recorder != nil { return .swallow }
                if state.matcher.keyUp(Int(keyCode), at: now).swallow { return .swallow }
                guard flags.contains(.maskSecondaryFn), state.isHidingFn else { return .pass }
                state.strippedCount += 1
                return .stripFn
            }
            return disposition

        default:
            // Mouse, scroll, gestures…: hide the fn flag during a lone fn press.
            guard flags.contains(.maskSecondaryFn) else { return .pass }
            return state.withLock { state in
                guard state.isHidingFn else { return .pass }
                state.strippedCount += 1
                return .stripFn
            }
        }
    }

    private func handleFlagsChanged(keyCode: Int64, flags: CGEventFlags, now: TimeInterval) -> Disposition {
        let isFnKey = keyCode == Self.fnKeyCode
        let (disposition, events, recorded, preview, note) = state.withLock {
            state -> (Disposition, [ShortcutEvent], ShortcutRecorder.Result?, [String]?, String?) in
            var disposition = Disposition.pass
            var note: String?

            if isFnKey {
                let down = flags.contains(.maskSecondaryFn)
                if down && !state.fnDown {
                    state.fnDown = true
                    state.fnCombo = false
                    state.strippedCount = 0
                    if state.hidesFn { disposition = .swallow }
                } else if !down && state.fnDown {
                    state.fnDown = false
                    // After a combo the system has already seen fn (on the key event), so let the
                    // release through to keep its modifier state right; a combo is never a 🌐 tap.
                    if state.hidesFn && !state.fnCombo { disposition = .swallow }
                    note = "fn press ended: swallowed=\(disposition == .swallow) combo=\(state.fnCombo) stripped=\(state.strippedCount)"
                } else if state.hidesFn && !state.fnCombo {
                    disposition = .swallow
                }
            } else if flags.contains(.maskSecondaryFn), state.isHidingFn {
                // Another modifier changed while fn is held: keep fn out of its flags.
                state.strippedCount += 1
                disposition = .stripFn
            }

            let held = Self.heldModifiers(flags, fnDown: state.fnDown)
            if var recorder = state.recorder {
                let result = recorder.modifiersChanged(held)
                state.recorder = result == .pending ? recorder : nil
                return (disposition, [], result == .pending ? nil : result, recorder.preview, note)
            }
            let outcome = state.matcher.modifiersChanged(held, at: now)
            return (disposition, outcome.events, nil, nil, note)
        }
        if let note { log.info("\(note, privacy: .public)") }
        emit(events)
        if recorded != nil || preview != nil { emitRecording(recorded, preview: preview) }
        return disposition
    }

    private func emit(_ events: [ShortcutEvent]) {
        guard !events.isEmpty else { return }
        DispatchQueue.main.async { [weak self] in
            MainActor.assumeIsolated {
                for event in events { self?.onEvent?(HotkeyEvent(event)) }
            }
        }
    }

    private func emitRecording(_ result: ShortcutRecorder.Result?, preview: [String]?) {
        DispatchQueue.main.async { [weak self] in
            MainActor.assumeIsolated {
                guard let self else { return }
                if let preview { self.onRecordingPreview?(preview) }
                switch result {
                case .done(let shortcut): self.finishRecording(shortcut)
                case .cancelled: self.finishRecording(nil)
                case .pending, nil: break
                }
            }
        }
    }
}

extension HotkeyEvent {
    init(_ event: ShortcutEvent) {
        switch event {
        case .pressed: self = .pressed
        case .released: self = .released
        case .cancelled: self = .cancelled
        case .commandPressed: self = .commandPressed
        case .commandReleased: self = .commandReleased
        case .pasteLast: self = .pasteLast
        case .meeting: self = .meeting
        }
    }
}

private func hotkeyTapCallback(
    proxy: CGEventTapProxy,
    type: CGEventType,
    event: CGEvent,
    refcon: UnsafeMutableRawPointer?
) -> Unmanaged<CGEvent>? {
    guard let refcon else { return Unmanaged.passUnretained(event) }
    let monitor = Unmanaged<HotkeyMonitor>.fromOpaque(refcon).takeUnretainedValue()
    let keyCode = event.getIntegerValueField(.keyboardEventKeycode)
    let isRepeat = event.getIntegerValueField(.keyboardEventAutorepeat) != 0
    switch monitor.handle(type: type, keyCode: keyCode, flags: event.flags, isRepeat: isRepeat) {
    case .pass:
        return Unmanaged.passUnretained(event)
    case .swallow:
        return nil
    case .stripFn:
        event.flags.remove(.maskSecondaryFn)
        return Unmanaged.passUnretained(event)
    }
}
