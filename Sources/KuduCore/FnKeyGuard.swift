import Foundation

/// The system's "Press 🌐 key to" setting.
public protocol FnUsageBackend: Sendable {
    /// 0 Do Nothing, 1 Change Input Source, 2 Show Emoji & Symbols, 3 Start Dictation.
    func current() -> Int?
    func set(_ value: Int)
}

/// Where the guard remembers the user's original setting, so it survives a crash.
public protocol GuardMemory: AnyObject {
    func integer(forKey key: String) -> Int
    func bool(forKey key: String) -> Bool
    func set(_ value: Int, forKey key: String)
    func set(_ value: Bool, forKey key: String)
}

extension UserDefaults: GuardMemory {}

/// While Kudu uses fn as its dictation key, pauses the system's own action for the 🌐 key.
/// Otherwise a quick tap or double-tap opens the Emoji & Symbols picker, which steals focus and
/// receives the dictated text. The 🌐 action is decided below the level where an app's event
/// tap can intercept keys, so changing the setting is the only reliable way.
///
/// The user's original choice is saved before it is changed and put back when Kudu quits or
/// stops using fn. If Kudu dies without restoring, the saved original is restored on the next
/// launch. If the user changes the setting themselves while Kudu runs, their choice is kept.
public final class FnKeyGuard {
    /// "Do Nothing".
    public static let pausedValue = 0

    private let backend: any FnUsageBackend
    private let memory: any GuardMemory

    private enum Memory {
        static let active = "fnGuard.active"
        static let original = "fnGuard.original"
    }

    public init(backend: any FnUsageBackend = SystemFnUsage(), memory: any GuardMemory = UserDefaults.standard) {
        self.backend = backend
        self.memory = memory
    }

    /// True while the system action is paused by Kudu (also after a crash, until restored).
    public var isEngaged: Bool { memory.bool(forKey: Memory.active) }

    public func engage() {
        if !isEngaged {
            guard let original = backend.current() else { return }
            memory.set(original, forKey: Memory.original)
            memory.set(true, forKey: Memory.active)
        }
        if backend.current() != Self.pausedValue {
            backend.set(Self.pausedValue)
        }
    }

    public func disengage() {
        guard isEngaged else { return }
        // The user picked something else while we were active: keep their choice.
        if backend.current() == Self.pausedValue {
            let original = memory.integer(forKey: Memory.original)
            if original != Self.pausedValue { backend.set(original) }
        }
        memory.set(false, forKey: Memory.active)
    }
}

/// The live system setting, changed the same way System Settings does: through HIToolbox's
/// `TISUpdateFnUsageType`, which saves the value and tells every process to drop its cached
/// copy. (Writing the preference directly only takes effect after logging out.)
public struct SystemFnUsage: FnUsageBackend {
    private typealias GetFunction = @convention(c) () -> Int32
    private typealias UpdateFunction = @convention(c) (Int32) -> Int32

    nonisolated(unsafe) private static let carbon = dlopen("/System/Library/Frameworks/Carbon.framework/Carbon", RTLD_LAZY)
    nonisolated(unsafe) private static let getFunction: GetFunction? =
        dlsym(carbon, "TISGetFnUsageType").map { unsafeBitCast($0, to: GetFunction.self) }
    nonisolated(unsafe) private static let updateFunction: UpdateFunction? =
        dlsym(carbon, "TISUpdateFnUsageType").map { unsafeBitCast($0, to: UpdateFunction.self) }

    public init() {}

    /// Whether this macOS offers the live setter.
    public static var isAvailable: Bool { getFunction != nil && updateFunction != nil }

    public func current() -> Int? {
        Self.getFunction.map { Int($0()) }
    }

    public func set(_ value: Int) {
        _ = Self.updateFunction?(Int32(value))
    }
}
