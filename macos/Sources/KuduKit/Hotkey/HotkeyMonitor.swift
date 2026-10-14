import AppKit
@preconcurrency import CoreFoundation
import CoreGraphics
import os

/// A single key that triggers dictation while held.
public enum TriggerKey: String, CaseIterable, Sendable, Codable, Identifiable {
    case fn
    case rightCommand
    case rightOption

    public var id: String { rawValue }

    public var displayName: String {
        switch self {
        case .fn: "fn (Globe)"
        case .rightCommand: "Right ⌘"
        case .rightOption: "Right ⌥"
        }
    }

    var keyCode: Int64 {
        switch self {
        case .fn: 63          // kVK_Function
        case .rightCommand: 54 // kVK_RightCommand
        case .rightOption: 61  // kVK_RightOption
        }
    }

    /// Whether this key is down according to the event's flags. Right-side modifiers use the
    /// device-dependent bits (NX_DEVICERCMDKEYMASK / NX_DEVICERALTKEYMASK) so the left key
    /// being held doesn't confuse us.
    func isDown(in flags: CGEventFlags) -> Bool {
        switch self {
        case .fn: flags.contains(.maskSecondaryFn)
        case .rightCommand: flags.rawValue & 0x10 != 0
        case .rightOption: flags.rawValue & 0x40 != 0
        }
    }
}

public enum HotkeyEvent: Sendable {
    /// Trigger key went down on its own.
    case pressed
    /// Trigger key released after a normal hold.
    case released
    /// The hold turned out to be part of a shortcut (another key was pressed), or Escape was hit.
    case cancelled
    /// Trigger + ⌃ held together: Command mode (speak an instruction for the selected text).
    case commandPressed
    /// The trigger was released after `commandPressed`.
    case commandReleased
}

/// Watches the trigger key with a CGEventTap at session level.
///
/// The tap is active (not listen-only) so it can swallow events: Escape while dictating, and
/// — when fn is the dictation key — the fn press and release themselves. macOS turns an fn
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
        var trigger: TriggerKey
        var isDictating = false
        var triggerDown = false
        var triggerDownAt: TimeInterval = 0
        /// Trigger + ⌃ is held: the hold is a Command mode instruction.
        var commandActive = false
        /// The physical fn key is down. Tracked separately from `triggerDown` because a hold can
        /// be cancelled (Escape, a shortcut) while fn is still held, and its release must still
        /// be swallowed.
        var fnDown = false
        /// Hide fn from the rest of the system while it is the dictation key.
        var suppressFn = true
        /// fn was combined with a real key (fn+arrow, fn+F…): the user wants a system shortcut,
        /// so stop hiding fn for the rest of this press.
        var fnCombo = false
        /// Events whose fn flag was removed during the current press (for diagnostics).
        var strippedCount = 0

        /// fn is physically held as a lone key and must stay invisible to the system.
        var isHidingFn: Bool { fnDown && !fnCombo && trigger == .fn && suppressFn }
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

    public var trigger: TriggerKey {
        get { state.withLock { $0.trigger } }
        set { state.withLock { $0.trigger = newValue; $0.triggerDown = false } }
    }

    /// When fn is the trigger, keep the system from seeing fn presses (no emoji picker).
    public var suppressFn: Bool {
        get { state.withLock { $0.suppressFn } }
        set { state.withLock { $0.suppressFn = newValue } }
    }

    /// Set by the owner while a dictation is in progress, so Escape can be swallowed.
    public var isDictating: Bool {
        get { state.withLock { $0.isDictating } }
        set { state.withLock { $0.isDictating = newValue } }
    }

    /// A second modifier joining within this window makes a chord: ⌃ means Command mode, the
    /// others mean a system shortcut (not dictation).
    private let chordWindow: TimeInterval = 0.3
    private static let controlKeyCodes: Set<Int64> = [59, 62] // left, right ⌃
    private let log = Logger(subsystem: "app.kudu", category: "hotkey")

    // Owned by start()/stop() on the main thread and read by the tap thread after start.
    nonisolated(unsafe) private var tap: CFMachPort?
    nonisolated(unsafe) private var tapRunLoop: CFRunLoop?

    public init(trigger: TriggerKey) {
        state = OSAllocatedUnfairLock(initialState: State(trigger: trigger))
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
        state.withLock { $0.triggerDown = false; $0.fnDown = false }
    }

    /// Runs on the tap thread for every event and must return quickly.
    fileprivate func handle(type: CGEventType, keyCode: Int64, flags: CGEventFlags) -> Disposition {
        switch type {
        case .tapDisabledByTimeout, .tapDisabledByUserInput:
            log.info("Event tap was disabled (\(type.rawValue)); re-enabling")
            if let tap { CGEvent.tapEnable(tap: tap, enable: true) }
            return .pass

        case .flagsChanged:
            return handleFlagsChanged(keyCode: keyCode, flags: flags)

        case .keyDown:
            let (disposition, events) = state.withLock { state -> (Disposition, [HotkeyEvent]) in
                if keyCode == 53 /* kVK_Escape */, state.isDictating {
                    state.triggerDown = false
                    state.commandActive = false
                    return (.swallow, [.cancelled])
                }
                var events: [HotkeyEvent] = []
                // Any regular key while the trigger is held means the user is typing a shortcut.
                if state.triggerDown {
                    state.triggerDown = false
                    state.commandActive = false
                    events.append(.cancelled)
                }
                if state.fnDown { state.fnCombo = true }
                return (.pass, events)
            }
            emit(events)
            return disposition

        default:
            // Mouse, scroll, gestures, key up…: hide the fn flag during a lone fn press.
            guard flags.contains(.maskSecondaryFn) else { return .pass }
            return state.withLock { state in
                guard state.isHidingFn else { return .pass }
                state.strippedCount += 1
                return .stripFn
            }
        }
    }

    private func handleFlagsChanged(keyCode: Int64, flags: CGEventFlags) -> Disposition {
        let isFnKey = keyCode == TriggerKey.fn.keyCode
        let (disposition, events, note) = state.withLock { state -> (Disposition, [HotkeyEvent], String?) in
            var events: [HotkeyEvent] = []
            var disposition = Disposition.pass
            var note: String?

            if isFnKey {
                let down = flags.contains(.maskSecondaryFn)
                let hiding = state.trigger == .fn && state.suppressFn
                if down && !state.fnDown {
                    state.fnDown = true
                    state.fnCombo = false
                    state.strippedCount = 0
                    if hiding { disposition = .swallow }
                } else if !down && state.fnDown {
                    state.fnDown = false
                    // After a combo the system has already seen fn (on the key event), so let the
                    // release through to keep its modifier state right; a combo is never a 🌐 tap.
                    if hiding && !state.fnCombo { disposition = .swallow }
                    note = "fn press ended: swallowed=\(disposition == .swallow) combo=\(state.fnCombo) stripped=\(state.strippedCount)"
                } else if hiding && !state.fnCombo {
                    disposition = .swallow
                }
            } else if flags.contains(.maskSecondaryFn), state.isHidingFn {
                // Another modifier changed while fn is held: keep fn out of its flags.
                state.strippedCount += 1
                disposition = .stripFn
            }

            guard keyCode == state.trigger.keyCode else {
                let justPressed = ProcessInfo.processInfo.systemUptime - state.triggerDownAt < chordWindow
                if Self.controlKeyCodes.contains(keyCode), flags.contains(.maskControl) {
                    // ⌃ joined the trigger right after it went down: Command mode.
                    if state.triggerDown, justPressed, !state.commandActive {
                        state.commandActive = true
                        events.append(.commandPressed)
                    }
                } else if state.triggerDown, justPressed, !state.commandActive, flags.contains(.maskCommand) || flags.contains(.maskShift) || flags.contains(.maskAlternate) {
                    // Another modifier joined the trigger: a system shortcut, not dictation.
                    state.triggerDown = false
                    events.append(.cancelled)
                }
                return (disposition, events, note)
            }
            let isDown = state.trigger.isDown(in: flags)
            if isDown && !state.triggerDown {
                state.triggerDown = true
                state.triggerDownAt = ProcessInfo.processInfo.systemUptime
                // ⌃ already held when the trigger goes down: Command mode straight away.
                if flags.contains(.maskControl) {
                    state.commandActive = true
                    events.append(.commandPressed)
                } else {
                    events.append(.pressed)
                }
            } else if !isDown && state.triggerDown {
                state.triggerDown = false
                if state.commandActive {
                    state.commandActive = false
                    events.append(.commandReleased)
                } else {
                    events.append(.released)
                }
            }
            return (disposition, events, note)
        }
        if let note { log.info("\(note, privacy: .public)") }
        emit(events)
        return disposition
    }

    private func emit(_ events: [HotkeyEvent]) {
        guard !events.isEmpty else { return }
        DispatchQueue.main.async { [weak self] in
            MainActor.assumeIsolated {
                for event in events { self?.onEvent?(event) }
            }
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
    switch monitor.handle(type: type, keyCode: keyCode, flags: event.flags) {
    case .pass:
        return Unmanaged.passUnretained(event)
    case .swallow:
        return nil
    case .stripFn:
        event.flags.remove(.maskSecondaryFn)
        return Unmanaged.passUnretained(event)
    }
}
