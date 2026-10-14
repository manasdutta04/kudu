import Foundation
import ServiceManagement
import os

/// Registers Kudu as a login item through ServiceManagement (no helper app needed).
@MainActor
public final class LaunchAtLogin: ObservableObject {
    @Published public private(set) var isEnabled = SMAppService.mainApp.status == .enabled
    private let log = Logger(subsystem: "app.kudu", category: "login-item")

    public init() {}

    public func set(_ enabled: Bool) {
        do {
            if enabled {
                try SMAppService.mainApp.register()
            } else {
                try SMAppService.mainApp.unregister()
            }
        } catch {
            log.error("Login item change failed: \(error.localizedDescription)")
        }
        isEnabled = SMAppService.mainApp.status == .enabled
    }
}
