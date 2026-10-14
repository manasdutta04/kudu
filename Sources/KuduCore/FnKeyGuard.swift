import Foundation

/// Reads and writes one preference key in one domain, per preference host.
public protocol PreferencesBackend: Sendable {
    func value(forKey key: String, domain: String, currentHost: Bool) -> Int?
    /// `nil` removes the key.
    func setValue(_ value: Int?, forKey key: String, domain: String, currentHost: Bool)
    func synchronize(domain: String)
}

/// Where the guard remembers the user's original setting, so it survives a crash.
public protocol GuardMemory: AnyObject {
    func integer(forKey key: String) -> Int
    func bool(forKey key: String) -> Bool
    func set(_ value: Int, forKey key: String)
    func set(_ value: Bool, forKey key: String)
}

extension UserDefaults: GuardMemory {}

/// While Kudu uses fn as its dictation key, pauses the system's own action for the 🌐 key
/// (System Settings → Keyboard → "Press 🌐 key to"). Otherwise a quick tap or double-tap opens
/// the Emoji & Symbols picker, which steals focus and receives the dictated text.
///
/// The user's original choice is saved before it is changed and put back when Kudu quits or
/// stops using fn. If Kudu dies without restoring, the saved original is restored on the next
/// launch. If the user changes the setting themselves while Kudu runs, their choice is kept.
public final class FnKeyGuard {
    public static let domain = "com.apple.HIToolbox"
    public static let key = "AppleFnUsageType"
    /// "Do Nothing".
    public static let pausedValue = 0

    private let preferences: any PreferencesBackend
    private let memory: any GuardMemory

    private enum Memory {
        static let active = "fnGuard.active"
        static func value(_ host: Host) -> String { "fnGuard.\(host.name).value" }
        static func wasSet(_ host: Host) -> String { "fnGuard.\(host.name).wasSet" }
    }

    private enum Host: CaseIterable {
        case any, current
        var name: String { self == .any ? "anyHost" : "currentHost" }
        var isCurrent: Bool { self == .current }
    }

    public init(preferences: any PreferencesBackend = SystemPreferences(), memory: any GuardMemory = UserDefaults.standard) {
        self.preferences = preferences
        self.memory = memory
    }

    /// True while the system action is paused by Kudu (also after a crash, until restored).
    public var isEngaged: Bool { memory.bool(forKey: Memory.active) }

    public func engage() {
        if !isEngaged {
            for host in Host.allCases {
                let current = read(host)
                memory.set(current != nil, forKey: Memory.wasSet(host))
                memory.set(current ?? 0, forKey: Memory.value(host))
            }
            memory.set(true, forKey: Memory.active)
        }
        // Re-apply even when already engaged: after a crash, or if something reset it.
        for host in Host.allCases {
            preferences.setValue(Self.pausedValue, forKey: Self.key, domain: Self.domain, currentHost: host.isCurrent)
        }
        preferences.synchronize(domain: Self.domain)
    }

    public func disengage() {
        guard isEngaged else { return }
        for host in Host.allCases {
            // The user picked something else while we were active: keep their choice.
            guard read(host) == Self.pausedValue else { continue }
            let original = memory.bool(forKey: Memory.wasSet(host)) ? memory.integer(forKey: Memory.value(host)) : nil
            preferences.setValue(original, forKey: Self.key, domain: Self.domain, currentHost: host.isCurrent)
        }
        preferences.synchronize(domain: Self.domain)
        memory.set(false, forKey: Memory.active)
    }

    private func read(_ host: Host) -> Int? {
        preferences.value(forKey: Self.key, domain: Self.domain, currentHost: host.isCurrent)
    }
}

/// The real macOS preferences, via CFPreferences (the same store System Settings writes).
public struct SystemPreferences: PreferencesBackend {
    public init() {}

    public func value(forKey key: String, domain: String, currentHost: Bool) -> Int? {
        let value = CFPreferencesCopyValue(
            key as CFString, domain as CFString, kCFPreferencesCurrentUser,
            currentHost ? kCFPreferencesCurrentHost : kCFPreferencesAnyHost
        )
        return (value as? NSNumber)?.intValue
    }

    public func setValue(_ value: Int?, forKey key: String, domain: String, currentHost: Bool) {
        CFPreferencesSetValue(
            key as CFString, value.map { NSNumber(value: $0) }, domain as CFString, kCFPreferencesCurrentUser,
            currentHost ? kCFPreferencesCurrentHost : kCFPreferencesAnyHost
        )
    }

    public func synchronize(domain: String) {
        CFPreferencesSynchronize(domain as CFString, kCFPreferencesCurrentUser, kCFPreferencesAnyHost)
        CFPreferencesSynchronize(domain as CFString, kCFPreferencesCurrentUser, kCFPreferencesCurrentHost)
    }
}
