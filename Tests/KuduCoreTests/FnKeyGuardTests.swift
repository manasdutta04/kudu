import Foundation
import Testing
@testable import KuduCore

/// In-memory stand-ins so tests never touch the real system setting.
final class FakeFnUsage: FnUsageBackend, @unchecked Sendable {
    var value: Int?
    var writes = 0

    init(_ value: Int?) { self.value = value }

    func current() -> Int? { value }
    func set(_ value: Int) {
        self.value = value
        writes += 1
    }
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
        let system = FakeFnUsage(2)
        let guardian = FnKeyGuard(backend: system, memory: FakeMemory())

        guardian.engage()
        #expect(system.value == 0)
        #expect(guardian.isEngaged)

        guardian.disengage()
        #expect(system.value == 2)
        #expect(!guardian.isEngaged)
    }

    @Test func crashRecoveryKeepsTheOriginalValue() {
        let system = FakeFnUsage(2)
        let memory = FakeMemory()
        FnKeyGuard(backend: system, memory: memory).engage()

        // Kudu was killed without restoring; a new launch engages again.
        let relaunched = FnKeyGuard(backend: system, memory: memory)
        relaunched.engage()
        relaunched.disengage()
        #expect(system.value == 2, "the paused 0 must never be saved as the original")
    }

    @Test func userChoiceMadeWhileEngagedWins() {
        let system = FakeFnUsage(2)
        let guardian = FnKeyGuard(backend: system, memory: FakeMemory())
        guardian.engage()
        system.value = 1 // user picked "Change Input Source" in System Settings
        guardian.disengage()
        #expect(system.value == 1)
    }

    @Test func disengageWithoutEngageDoesNothing() {
        let system = FakeFnUsage(3)
        FnKeyGuard(backend: system, memory: FakeMemory()).disengage()
        #expect(system.value == 3)
        #expect(system.writes == 0)
    }

    @Test func alreadyDoNothingIsLeftAlone() {
        let system = FakeFnUsage(0)
        let guardian = FnKeyGuard(backend: system, memory: FakeMemory())
        guardian.engage()
        guardian.disengage()
        #expect(system.value == 0)
        #expect(system.writes == 0)
    }

    @Test func unavailableSystemSettingIsNotEngaged() {
        let guardian = FnKeyGuard(backend: FakeFnUsage(nil), memory: FakeMemory())
        guardian.engage()
        #expect(!guardian.isEngaged)
    }
}
