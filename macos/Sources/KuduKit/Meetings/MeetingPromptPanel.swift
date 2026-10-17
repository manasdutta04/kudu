import AppKit
import SwiftUI

/// "Meeting in Zoom · Take notes?" in the top-right corner when a meeting starts and the user
/// chose to be asked. It never takes focus, and goes away by itself after 20 seconds.
@MainActor
final class MeetingPromptPanel {
    private let panel: NSPanel
    private let model = MeetingPromptModel()
    private var dismissal: Task<Void, Never>?
    private let size = NSSize(width: 340, height: 92)

    init() {
        panel = NSPanel(
            contentRect: NSRect(origin: .zero, size: size),
            styleMask: [.nonactivatingPanel, .borderless],
            backing: .buffered,
            defer: false
        )
        panel.level = .statusBar
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.hidesOnDeactivate = false
        panel.isReleasedWhenClosed = false
        let host = FirstClickHostingView(rootView: MeetingPromptView(model: model))
        host.frame = NSRect(origin: .zero, size: size)
        panel.contentView = host
    }

    func show(app: String, shortcut: String?, start: @escaping () -> Void) {
        model.app = app
        model.shortcut = shortcut
        model.start = { [weak self] in
            self?.hide()
            start()
        }
        model.dismiss = { [weak self] in self?.hide() }
        if let screen = NSScreen.main {
            let frame = screen.visibleFrame
            panel.setFrameOrigin(NSPoint(x: frame.maxX - size.width - 16, y: frame.maxY - size.height - 12))
        }
        panel.orderFrontRegardless()
        dismissal?.cancel()
        dismissal = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .seconds(20))
            guard !Task.isCancelled else { return }
            self?.hide()
        }
    }

    func hide() {
        dismissal?.cancel()
        dismissal = nil
        panel.orderOut(nil)
    }
}

/// Buttons respond to the first click, without the panel having to become key first.
private final class FirstClickHostingView<Content: View>: NSHostingView<Content> {
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}

@MainActor
final class MeetingPromptModel: ObservableObject {
    @Published var app = ""
    @Published var shortcut: String?
    var start: () -> Void = {}
    var dismiss: () -> Void = {}
}

private struct MeetingPromptView: View {
    @ObservedObject var model: MeetingPromptModel

    var body: some View {
        HStack(alignment: .center, spacing: 12) {
            ZStack {
                Circle().fill(Color(red: 0.42, green: 0.33, blue: 0.95).opacity(0.18)).frame(width: 36, height: 36)
                Image(systemName: "waveform.badge.mic").font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(Color(red: 0.62, green: 0.55, blue: 1))
            }
            VStack(alignment: .leading, spacing: 2) {
                Text("Meeting in \(model.app)").font(.system(size: 13, weight: .semibold)).foregroundStyle(.white)
                Text(model.shortcut.map { "Take notes on this Mac? (\($0))" } ?? "Take notes on this Mac?")
                    .font(.system(size: 11.5)).foregroundStyle(.white.opacity(0.65)).lineLimit(1)
                HStack(spacing: 8) {
                    Button("Take notes") { model.start() }
                        .buttonStyle(PromptButtonStyle(prominent: true))
                    Button("Not now") { model.dismiss() }
                        .buttonStyle(PromptButtonStyle(prominent: false))
                }
                .padding(.top, 5)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 14)
        .frame(width: 340, height: 92)
        .background(PromptGlass())
        .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(.white.opacity(0.12), lineWidth: 1))
    }
}

private struct PromptButtonStyle: ButtonStyle {
    var prominent: Bool

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12, weight: .semibold))
            .foregroundStyle(prominent ? Color.white : Color.white.opacity(0.85))
            .padding(.horizontal, 11).padding(.vertical, 4)
            .background(
                Capsule().fill(prominent ? Color(red: 0.42, green: 0.33, blue: 0.95) : Color.white.opacity(0.12))
            )
            .opacity(configuration.isPressed ? 0.75 : 1)
    }
}

private struct PromptGlass: NSViewRepresentable {
    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = .hudWindow
        view.blendingMode = .behindWindow
        view.state = .active
        view.appearance = NSAppearance(named: .darkAqua)
        return view
    }

    func updateNSView(_ view: NSVisualEffectView, context: Context) {}
}
