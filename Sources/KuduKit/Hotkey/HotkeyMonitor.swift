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
}

/// Watches the trigger key with a CGEventTap at session level.
///
/// The tap is active (not listen-only) so it can swallow events: Escape while dictating, and
/// every fn release, which macOS would otherwise turn into the emoji picker ("Press 🌐 key to").
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
        /// The physical fn key is down. Tracked separately from `triggerDown` because a hold can
        /// be cancelled (Escape, a shortcut) while fn is still held, and its release must still
        /// be swallowed.
        var fnDown = false
    }

    private let state: OSAllocatedUnfairLock<State>
    /// Called on the main thread.
    @MainActor public var onEvent: ((HotkeyEvent) -> Void)?

    public var trigger: TriggerKey {
        get { state.withLock { $0.trigger } }
        set { state.withLock { $0.trigger = newValue; $0.triggerDown = false } }
    }

    /// Set by the owner while a dictation is in progress, so Escape can be swallowed.
    public var isDictating: Bool {
        get { state.withLock { $0.isDictating } }
        set { state.withLock { $0.isDictating = newValue } }
    }

    /// A second modifier joining within this window means a shortcut (e.g. fn+⌃), not dictation.
    private let chordWindow: TimeInterval = 0.3
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
        let mask: CGEventMask =
            (1 << CGEventType.flagsChanged.rawValue) | (1 << CGEventType.keyDown.rawValue)
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

    /// Runs on the tap thread and must return quickly. Returns true to swallow the event.
    fileprivate func handle(type: CGEventType, keyCode: Int64, flags: CGEventFlags) -> Bool {
        switch type {
        case .tapDisabledByTimeout, .tapDisabledByUserInput:
            log.info("Event tap was disabled (\(type.rawValue)); re-enabling")
            if let tap { CGEvent.tapEnable(tap: tap, enable: true) }
            return false

        case .flagsChanged:
            let (swallow, events) = state.withLock { state -> (Bool, [HotkeyEvent]) in
                var events: [HotkeyEvent] = []
                var swallow = false
                if keyCode == TriggerKey.fn.keyCode {
                    let down = flags.contains(.maskSecondaryFn)
                    // While fn is the dictation key, macOS must never see a fn release on its own.
                    if !down && state.fnDown && state.trigger == .fn { swallow = true }
                    state.fnDown = down
                }

                guard keyCode == state.trigger.keyCode else {
                    // Another modifier joined the trigger (e.g. fn+⌃): a shortcut, not dictation.
                    if state.triggerDown,
                       ProcessInfo.processInfo.systemUptime - state.triggerDownAt < chordWindow {
                        state.triggerDown = false
                        events.append(.cancelled)
                    }
                    return (swallow, events)
                }
                let isDown = state.trigger.isDown(in: flags)
                if isDown && !state.triggerDown {
                    state.triggerDown = true
                    state.triggerDownAt = ProcessInfo.processInfo.systemUptime
                    events.append(.pressed)
                } else if !isDown && state.triggerDown {
                    state.triggerDown = false
                    events.append(.released)
                }
                return (swallow, events)
            }
            emit(events)
            return swallow

        case .keyDown:
            let (swallow, events) = state.withLock { state -> (Bool, [HotkeyEvent]) in
                if keyCode == 53 /* kVK_Escape */, state.isDictating {
                    state.triggerDown = false
                    return (true, [.cancelled])
                }
                // Any regular key while the trigger is held means the user is typing a shortcut.
                if state.triggerDown {
                    state.triggerDown = false
                    return (false, [.cancelled])
                }
                return (false, [])
            }
            emit(events)
            return swallow

        default:
            return false
        }
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
    let swallow = monitor.handle(type: type, keyCode: keyCode, flags: event.flags)
    return swallow ? nil : Unmanaged.passUnretained(event)
}
