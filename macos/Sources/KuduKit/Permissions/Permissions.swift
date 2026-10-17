import AVFoundation
import AppKit
import ApplicationServices

/// The three macOS permissions Kudu needs.
@MainActor
public enum Permissions {
    public static var microphoneGranted: Bool {
        AVCaptureDevice.authorizationStatus(for: .audio) == .authorized
    }

    public static func requestMicrophone() async -> Bool {
        await AVCaptureDevice.requestAccess(for: .audio)
    }

    /// Needed to read the hotkey with an active event tap and to insert text.
    public static var accessibilityGranted: Bool {
        AXIsProcessTrusted()
    }

    /// Shows the system prompt that deep-links to Privacy & Security → Accessibility.
    public static func requestAccessibility() {
        let options = ["AXTrustedCheckOptionPrompt": true] as CFDictionary
        _ = AXIsProcessTrustedWithOptions(options)
    }

    /// Removes Kudu's Accessibility entry so it can be granted fresh. Needed when the entry in
    /// System Settings belongs to an older build with a different code signature: the switch
    /// shows "on" but macOS doesn't trust this copy of the app.
    public static func resetAccessibility() {
        guard let bundleID = Bundle.main.bundleIdentifier else { return }
        let task = Process()
        task.executableURL = URL(fileURLWithPath: "/usr/bin/tccutil")
        task.arguments = ["reset", "Accessibility", bundleID]
        try? task.run()
        task.waitUntilExit()
        requestAccessibility()
    }

    /// Starts a fresh copy of Kudu and quits this one. macOS applies a new Accessibility
    /// grant to the keyboard event tap reliably only after a relaunch.
    public static func relaunch() {
        let path = Bundle.main.bundlePath
        let task = Process()
        task.executableURL = URL(fileURLWithPath: "/bin/sh")
        task.arguments = ["-c", "sleep 0.5; /usr/bin/open \"$0\"", path]
        try? task.run()
        NSApp.terminate(nil)
    }

    public static func openSettings(_ pane: Pane) {
        let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?\(pane.rawValue)")!
        NSWorkspace.shared.open(url)
    }

    public enum Pane: String {
        case microphone = "Privacy_Microphone"
        case accessibility = "Privacy_Accessibility"
        /// "Screen & System Audio Recording": the Mac's sound, for meeting notes.
        case audioCapture = "Privacy_AudioCapture"
    }
}
