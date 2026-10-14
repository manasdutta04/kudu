import Combine
import KuduCore
import KuduKit
import SwiftUI

/// Renders the menu-bar icon: the Kudu mark, whose three waveform bars follow your voice
/// while recording and ripple while the text is being prepared.
@MainActor
final class MenuBarIconAnimator: ObservableObject {
    @Published private(set) var image: NSImage

    private static let size = NSSize(width: 18, height: 18)
    private let controller: DictationController
    private var timer: Timer?
    private var phase: Double = 0
    private var subscription: AnyCancellable?

    init(controller: DictationController) {
        self.controller = controller
        image = KuduMark.image(size: Self.size, template: true)
        subscription = controller.$isRecording
            .combineLatest(controller.$isTranscribing)
            .sink { [weak self] recording, transcribing in
                self?.setAnimating(recording || transcribing)
            }
    }

    private func setAnimating(_ animating: Bool) {
        if animating {
            guard timer == nil else { return }
            // ~15 fps is smooth at this size and cheap for the menu bar.
            let timer = Timer(timeInterval: 1.0 / 15, repeats: true) { [weak self] _ in
                MainActor.assumeIsolated { self?.tick() }
            }
            RunLoop.main.add(timer, forMode: .common)
            self.timer = timer
        } else {
            timer?.invalidate()
            timer = nil
            image = KuduMark.image(size: Self.size, template: true)
        }
    }

    private func tick() {
        let levels: [CGFloat]
        if controller.isRecording {
            // Middle bar: newest loudness; outer bars: slightly older, so speech ripples outward.
            let history = controller.levels.history
            levels = [history[1], history[0], history[2]].map { 0.15 + 0.85 * CGFloat($0) }
        } else {
            phase += 0.45
            levels = (0..<3).map { 0.5 + 0.35 * CGFloat(sin(phase - Double($0) * 0.9)) }
        }
        image = KuduMark.image(size: Self.size, barLevels: levels, template: true)
    }
}

struct MenuBarLabel: View {
    @StateObject private var animator: MenuBarIconAnimator
    @Environment(\.openWindow) private var openWindow

    init(controller: DictationController) {
        _animator = StateObject(wrappedValue: MenuBarIconAnimator(controller: controller))
    }

    var body: some View {
        Image(nsImage: animator.image)
            .onAppear { Windows.openAction = openWindow }
    }
}

/// The menu under the menu-bar icon: a native menu, so it looks and behaves like the rest of
/// macOS (keyboard navigation, checkmarks, submenus).
struct MenuContent: View {
    @ObservedObject var controller: DictationController
    @ObservedObject var history: HistoryStore
    @ObservedObject var store: UserDataStore
    @ObservedObject var navigation: Navigation
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Text(status)

        Divider()

        Button("Open Kudu") { show(.home) }
        Button("Paste last transcript") { controller.pasteLastTranscript() }
            .keyboardShortcut(Self.menuShortcut(controller.shortcuts.pasteLast))
            .disabled(history.records.isEmpty)

        Divider()

        Picker("Dictation key", selection: $controller.dictateShortcut) {
            ForEach(Self.dictationChoices(current: controller.shortcuts.dictate), id: \.self) { Text($0.displayName).tag($0) }
        }
        Picker("Recording", selection: $controller.recordingMode) {
            ForEach(RecordingMode.allCases) { Text($0.title).tag($0) }
        }
        Picker("Microphone", selection: $controller.microphoneUID) {
            Text("System default").tag(String?.none)
            ForEach(controller.microphones) { Text($0.name).tag(Optional($0.uid)) }
        }
        Picker("Editing", selection: $store.cleanup.editingStrength) {
            ForEach(EditingStrength.allCases) { Text($0.title).tag($0) }
        }
        Toggle("Sounds", isOn: $controller.soundsEnabled)

        Divider()

        Button("Settings…") { show(.settings) }
            .keyboardShortcut(",", modifiers: .command)
        Button("Help & feedback") {
            NSWorkspace.shared.open(URL(string: "https://github.com/manasdutta04/kudu/issues")!)
        }

        Divider()

        Button("Quit Kudu") { NSApp.terminate(nil) }
            .keyboardShortcut("q", modifiers: .command)
    }

    /// The paste-last shortcut shown at the right of the menu item, when SwiftUI can show it.
    static func menuShortcut(_ shortcut: KeyShortcut?) -> KeyboardShortcut? {
        guard let shortcut, let label = shortcut.keyLabel, label.count == 1, !shortcut.contains(.fn),
              let character = label.lowercased().first
        else { return nil }
        var modifiers: EventModifiers = []
        for modifier in shortcut.modifiers {
            switch modifier.kind {
            case .control: modifiers.insert(.control)
            case .option: modifiers.insert(.option)
            case .shift: modifiers.insert(.shift)
            case .command: modifiers.insert(.command)
            case .fn: break
            }
        }
        return KeyboardShortcut(KeyEquivalent(character), modifiers: modifiers)
    }

    /// The usual dictation keys, plus a custom one recorded in Settings.
    static func dictationChoices(current: KeyShortcut) -> [KeyShortcut] {
        let presets: [KeyShortcut] = [.fn, .rightCommand, .rightOption]
        return presets.contains(current) ? presets : presets + [current]
    }

    private func show(_ page: Page) {
        navigation.page = page
        NSApp.activate(ignoringOtherApps: true)
        openWindow(id: WindowID.main)
    }

    private var status: String {
        switch controller.modelState {
        case .ready where controller.hotkeyActive:
            "Kudu is ready · hold \(controller.shortcuts.dictate.displayName) to dictate"
        case .ready: "Kudu needs Accessibility permission"
        case .downloading(let fraction): "Downloading speech model… \(Int(fraction * 100))%"
        case .loading: "Loading speech model…"
        case .missing: "Finish setting up in Kudu"
        case .failed: "Speech model error: open Kudu"
        }
    }
}

struct StatusBadge: View {
    @ObservedObject var controller: DictationController

    var body: some View {
        switch controller.modelState {
        case .ready where controller.hotkeyActive:
            Label("Ready", systemImage: "checkmark.circle.fill").foregroundStyle(.green)
        case .ready:
            Label("Needs permission", systemImage: "exclamationmark.triangle.fill").foregroundStyle(.orange)
        case .downloading(let fraction):
            Text("Downloading \(Int(fraction * 100))%").foregroundStyle(.secondary)
        case .loading:
            Text("Loading…").foregroundStyle(.secondary)
        case .missing:
            Label("No model", systemImage: "arrow.down.circle").foregroundStyle(.orange)
        case .failed:
            Label("Error", systemImage: "xmark.octagon.fill").foregroundStyle(.red)
        }
    }
}
