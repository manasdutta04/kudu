import AppKit
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
/// The tap is active (not listen-only) so it can swallow Escape while dictating.
/// That requires Accessibility permission, which Kudu needs anyway to insert text.
@MainActor
public final class HotkeyMonitor {
    public var trigger: TriggerKey
    public var onEvent: ((HotkeyEvent) -> Void)?
    /// Set by the owner while a dictation is in progress, so Escape can be swallowed.
    public var isDictating = false

    private var tap: CFMachPort?
    private var runLoopSource: CFRunLoopSource?
    private var triggerDown = false
    private var triggerDownAt: TimeInterval = 0
    /// A second modifier joining within this window means a shortcut (e.g. fn+⌃), not dictation.
    private let chordWindow: TimeInterval = 0.3
    private let log = Logger(subsystem: "app.kudu", category: "hotkey")

    public init(trigger: TriggerKey) {
        self.trigger = trigger
    }

    /// Returns false when the tap could not be created (usually: Accessibility not granted yet).
    @discardableResult
    public func start() -> Bool {
        guard tap == nil else { return true }
        let mask: CGEventMask =
            (1 << CGEventType.flagsChanged.rawValue) | (1 << CGEventType.keyDown.rawValue)
        let refcon = Unmanaged.passUnretained(self).toOpaque()
        guard let tap = CGEvent.tapCreate(
            tap: .cgSessionEventTap,
            place: .headInsertEventTap,
            options: .defaultTap,
            eventsOfInterest: mask,
            callback: hotkeyTapCallback,
            userInfo: refcon
        ) else {
            log.error("CGEvent.tapCreate failed; Accessibility permission missing?")
            return false
        }
        self.tap = tap
        let source = CFMachPortCreateRunLoopSource(nil, tap, 0)
        CFRunLoopAddSource(CFRunLoopGetMain(), source, .commonModes)
        runLoopSource = source
        CGEvent.tapEnable(tap: tap, enable: true)
        return true
    }

    public func stop() {
        if let tap { CGEvent.tapEnable(tap: tap, enable: false) }
        if let runLoopSource { CFRunLoopRemoveSource(CFRunLoopGetMain(), runLoopSource, .commonModes) }
        tap = nil
        runLoopSource = nil
        triggerDown = false
    }

    /// Runs on the main run loop (the tap source is attached there). Returns true to swallow the event.
    fileprivate func handle(type: CGEventType, keyCode: Int64, flags: CGEventFlags) -> Bool {
        switch type {
        case .tapDisabledByTimeout, .tapDisabledByUserInput:
            if let tap { CGEvent.tapEnable(tap: tap, enable: true) }
            return false

        case .flagsChanged:
            guard keyCode == trigger.keyCode else {
                // Another modifier joined the trigger (e.g. fn+⌃): treat as a shortcut, not dictation.
                if triggerDown, ProcessInfo.processInfo.systemUptime - triggerDownAt < chordWindow {
                    cancelHold()
                }
                return false
            }
            let isDown = trigger.isDown(in: flags)
            if isDown && !triggerDown {
                triggerDown = true
                triggerDownAt = ProcessInfo.processInfo.systemUptime
                onEvent?(.pressed)
            } else if !isDown && triggerDown {
                triggerDown = false
                onEvent?(.released)
                // A lone fn release opens the emoji picker / switches input source (the
                // "Press 🌐 key to" setting). Swallow it after a dictation so that doesn't happen.
                return trigger == .fn
            }
            return false

        case .keyDown:
            if keyCode == 53 /* kVK_Escape */, isDictating {
                cancelHold()
                return true
            }
            // Any regular key while the trigger is held means the user is typing a shortcut.
            if triggerDown { cancelHold() }
            return false

        default:
            return false
        }
    }

    private func cancelHold() {
        triggerDown = false
        onEvent?(.cancelled)
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
    // The run loop source lives on the main run loop, so this is already the main thread.
    let keyCode = event.getIntegerValueField(.keyboardEventKeycode)
    let flags = event.flags
    let swallow = MainActor.assumeIsolated { monitor.handle(type: type, keyCode: keyCode, flags: flags) }
    return swallow ? nil : Unmanaged.passUnretained(event)
}
