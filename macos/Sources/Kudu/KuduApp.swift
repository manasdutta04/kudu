import KuduKit
import SwiftUI

@main
struct KuduApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    var body: some Scene {
        Window("Kudu", id: WindowID.main) {
            MainWindow(controller: appDelegate.controller, navigation: appDelegate.navigation)
        }
        .windowStyle(.hiddenTitleBar)
        .defaultSize(width: 1080, height: 720)

        MenuBarExtra {
            MenuContent(
                controller: appDelegate.controller, meetings: appDelegate.controller.meetings, history: appDelegate.controller.history,
                store: appDelegate.controller.store, navigation: appDelegate.navigation
            )
        } label: {
            MenuBarLabel(controller: appDelegate.controller)
        }
        .menuBarExtraStyle(.menu)
    }
}

enum WindowID {
    static let main = "main"
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let controller = DictationController()
    let navigation = Navigation()

    private var signalSources: [DispatchSourceSignal] = []

    func applicationDidFinishLaunching(_ notification: Notification) {
        restoreSystemStateOnSignals()
        controller.activate()
        if CommandLine.arguments.contains("--overlay-demo") {
            controller.runOverlayDemo()
        }
        // Setup, if anything is missing, lives on the Home page of the main window.
    }

    /// Kudu keeps listening for the dictation key after its window is closed.
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }

    /// Clicking the Dock icon brings the main window back.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        if !flag { Windows.open(WindowID.main) }
        return true
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

/// Opens SwiftUI windows from AppKit code (the app delegate), which has no `openWindow`.
@MainActor
enum Windows {
    static var openAction: OpenWindowAction?

    static func open(_ id: String) {
        NSApp.activate(ignoringOtherApps: true)
        if let openAction {
            openAction(id: id)
        } else {
            // The SwiftUI environment isn't wired yet right after launch; retry shortly.
            Task { @MainActor in
                try? await Task.sleep(for: .milliseconds(300))
                openAction?(id: id)
            }
        }
    }
}
