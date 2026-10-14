import KuduKit
import SwiftUI

@main
struct KuduApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    var body: some Scene {
        MenuBarExtra {
            MenuContent(controller: appDelegate.controller)
        } label: {
            MenuBarIcon(controller: appDelegate.controller)
        }
        .menuBarExtraStyle(.window)

        Window("Welcome to Kudu", id: "onboarding") {
            OnboardingView(controller: appDelegate.controller)
        }
        .windowResizability(.contentSize)

        Window("Kudu Settings", id: "settings") {
            SettingsView(store: appDelegate.controller.store)
        }
        .windowResizability(.contentSize)
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let controller = DictationController()

    private var signalSources: [DispatchSourceSignal] = []

    func applicationDidFinishLaunching(_ notification: Notification) {
        restoreSystemStateOnSignals()
        controller.activate()
        if CommandLine.arguments.contains("--overlay-demo") {
            controller.runOverlayDemo()
            return
        }
        if !Permissions.accessibilityGranted || !Permissions.microphoneGranted || controller.modelState == .missing {
            OnboardingWindow.open()
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        controller.shutdown()
    }

    /// `killall Kudu`, logout and Ctrl-C send signals that skip applicationWillTerminate.
    /// Catch them so the 🌐 key's own action is always given back.
    private func restoreSystemStateOnSignals() {
        for sig in [SIGTERM, SIGINT, SIGHUP] {
            signal(sig, SIG_IGN)
            let source = DispatchSource.makeSignalSource(signal: sig, queue: .main)
            source.setEventHandler { [weak self] in
                MainActor.assumeIsolated { self?.controller.shutdown() }
                exit(0)
            }
            source.resume()
            signalSources.append(source)
        }
    }
}

/// Opens the onboarding window from AppKit code (outside a SwiftUI view).
@MainActor
enum OnboardingWindow {
    static var openAction: OpenWindowAction?

    static func open() {
        NSApp.activate(ignoringOtherApps: true)
        if let openAction {
            openAction(id: "onboarding")
        } else {
            // The SwiftUI environment isn't wired yet on first launch; retry shortly.
            Task { @MainActor in
                try? await Task.sleep(for: .milliseconds(300))
                openAction?(id: "onboarding")
            }
        }
    }
}

struct MenuBarIcon: View {
    @ObservedObject var controller: DictationController
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Image(systemName: controller.isRecording ? "waveform.circle.fill" : "waveform")
            .onAppear { OnboardingWindow.openAction = openWindow }
    }
}
