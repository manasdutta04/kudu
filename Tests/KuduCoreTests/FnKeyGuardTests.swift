import Foundation
import Testing
@testable import KuduCore

/// In-memory stand-ins so tests never touch the real system setting.
final class FakePreferences: PreferencesBackend, @unchecked Sendable {
    var anyHost: Int?
    var currentHost: Int?
    var syncCount = 0

    init(anyHost: Int?, currentHost: Int?) {
        self.anyHost = anyHost
        self.currentHost = currentHost
    }

    func value(forKey key: String, domain: String, currentHost: Bool) -> Int? {
        currentHost ? self.currentHost : anyHost
    }

    func setValue(_ value: Int?, forKey key: String, domain: String, currentHost: Bool) {
        if currentHost { self.currentHost = value } else { anyHost = value }
    }

    func synchronize(domain: String) { syncCount += 1 }
}

final class FakeMemory: GuardMemory {
    var store: [String: Any] = [:]
    func integer(forKey key: String) -> Int { store[key] as? Int ?? 0 }
    func bool(forKey key: String) -> Bool { store[key] as? Bool ?? false }
    func set(_ value: Int, forKey key: String) { store[key] = value }
    func set(_ value: Bool, forKey key: String) { store[key] = value }
}

@Suite struct FnKeyGuardTests {
    @Test func engagePausesAndDisengageRestores() {
        let prefs = FakePreferences(anyHost: 2, currentHost: nil)
        let guardian = FnKeyGuard(preferences: prefs, memory: FakeMemory())

        guardian.engage()
        #expect(prefs.anyHost == 0)
        #expect(prefs.currentHost == 0)
        #expect(guardian.isEngaged)

        guardian.disengage()
        #expect(prefs.anyHost == 2)
        #expect(prefs.currentHost == nil, "a key that wasn't set before is removed again")
        #expect(!guardian.isEngaged)
    }

    @Test func crashRecoveryKeepsTheOriginalValue() {
        let prefs = FakePreferences(anyHost: 2, currentHost: nil)
        let memory = FakeMemory()
        FnKeyGuard(preferences: prefs, memory: memory).engage()

        // Kudu was killed without restoring; a new launch engages again.
        let relaunched = FnKeyGuard(preferences: prefs, memory: memory)
        relaunched.engage()
        relaunched.disengage()
        #expect(prefs.anyHost == 2, "the paused 0 must never be saved as the original")
    }

    @Test func userChoiceMadeWhileEngagedWins() {
        let prefs = FakePreferences(anyHost: 2, currentHost: nil)
        let guardian = FnKeyGuard(preferences: prefs, memory: FakeMemory())
        guardian.engage()
        prefs.anyHost = 1 // user picked "Change Input Source" in System Settings
        guardian.disengage()
        #expect(prefs.anyHost == 1)
    }

    @Test func disengageWithoutEngageDoesNothing() {
        let prefs = FakePreferences(anyHost: 2, currentHost: 3)
        FnKeyGuard(preferences: prefs, memory: FakeMemory()).disengage()
        #expect(prefs.anyHost == 2)
        #expect(prefs.currentHost == 3)
        #expect(prefs.syncCount == 0)
    }

    @Test func alreadyDoNothingStaysDoNothing() {
        let prefs = FakePreferences(anyHost: 0, currentHost: nil)
        let guardian = FnKeyGuard(preferences: prefs, memory: FakeMemory())
        guardian.engage()
        guardian.disengage()
        #expect(prefs.anyHost == 0)
        #expect(prefs.currentHost == nil)
    }
}
