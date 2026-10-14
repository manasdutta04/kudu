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

/// The small popover under the menu-bar icon: status, the last dictation, and a way into the app.
struct MenuContent: View {
    @ObservedObject var controller: DictationController
    @ObservedObject var history: HistoryStore
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                Image(nsImage: KuduMark.image(size: NSSize(width: 18, height: 18), template: true))
                Text("Kudu").font(.headline)
                Spacer()
                StatusBadge(controller: controller)
            }

            if let last = history.records.first {
                VStack(alignment: .leading, spacing: 6) {
                    Text(last.text)
                        .font(.callout)
                        .lineLimit(4)
                        .textSelection(.enabled)
                    HStack {
                        Text("\(last.latencyMs) ms after release").font(.caption).foregroundStyle(.secondary)
                        Spacer()
                        Button("Copy") { controller.copyToClipboard(last) }.controlSize(.small)
                    }
                }
            } else {
                Text(controller.recordingMode.hint(for: controller.trigger))
                    .font(.callout).foregroundStyle(.secondary)
            }

            Divider()

            HStack {
                Button("Open Kudu") {
                    NSApp.activate(ignoringOtherApps: true)
                    openWindow(id: WindowID.main)
                }
                .keyboardShortcut(.defaultAction)
                Spacer()
                Button("Quit") { NSApp.terminate(nil) }
                    .keyboardShortcut("q")
            }
        }
        .padding(14)
        .frame(width: 290)
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
