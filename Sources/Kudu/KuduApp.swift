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

    func applicationDidFinishLaunching(_ notification: Notification) {
        controller.activate()
        if !Permissions.accessibilityGranted || !Permissions.microphoneGranted || controller.modelState == .missing {
            OnboardingWindow.open()
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
