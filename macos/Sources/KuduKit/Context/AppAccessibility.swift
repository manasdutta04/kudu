import AppKit
import ApplicationServices
import os

/// Apps built on Electron or Chromium (Slack, VS Code, Notion, Discord, chat apps, browsers)
/// keep their text hidden from Accessibility until an assistive app asks for it, so Kudu
/// couldn't read the selection for Command mode, the names on screen, or your corrections.
/// Setting `AXManualAccessibility` on the app is the documented way to switch it on.
///
/// Done once per app process, when the app becomes active, so it's ready before you dictate.
@MainActor
enum AppAccessibility {
    private static var handled: Set<pid_t> = []
    private static var observer: NSObjectProtocol?
    private static let log = Logger(subsystem: "app.kudu", category: "accessibility")

    static func start() {
        guard observer == nil else { return }
        observer = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main
        ) { notification in
            let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication
            MainActor.assumeIsolated { if let app { enable(app) } }
        }
        if let app = NSWorkspace.shared.frontmostApplication { enable(app) }
    }

    static func enable(_ app: NSRunningApplication) {
        let pid = app.processIdentifier
        guard !handled.contains(pid), pid != ProcessInfo.processInfo.processIdentifier else { return }
        handled.insert(pid)
        guard isChromiumBased(app) else { return }
        let element = AXUIElementCreateApplication(pid)
        AXUIElementSetMessagingTimeout(element, 0.2)
        let result = AXUIElementSetAttributeValue(element, "AXManualAccessibility" as CFString, kCFBooleanTrue)
        log.info("Accessibility on for \(app.bundleIdentifier ?? "?", privacy: .public): \(result == .success)")
    }

    /// Electron and Chromium apps ship their engine as a framework inside the app bundle.
    private static func isChromiumBased(_ app: NSRunningApplication) -> Bool {
        guard let url = app.bundleURL?.appendingPathComponent("Contents/Frameworks"),
              let frameworks = try? FileManager.default.contentsOfDirectory(atPath: url.path)
        else { return false }
        return frameworks.contains { name in
            ["Electron", "Chromium", "Chrome", "Edge", "Brave", "Vivaldi", "Opera", "Arc", "CEF"].contains { name.contains($0) }
        }
    }
}
