import AppKit
import SwiftUI

/// Observable state the overlay pill renders.
@MainActor
public final class OverlayModel: ObservableObject {
    public enum Phase: Equatable {
        case recording
        case transcribing
        case message(String)
    }

    @Published public var phase: Phase = .recording
    /// Hands-free recording: shows a lock instead of requiring the key to be held.
    @Published public var isLocked = false
    /// Recent input levels (0...1), newest last, for the waveform bars.
    @Published public var levels: [Float] = Array(repeating: 0, count: OverlayModel.barCount)

    static let barCount = 14

    public init() {}

    public func push(level: Float) {
        levels.removeFirst()
        levels.append(level)
    }

    public func resetLevels() {
        levels = Array(repeating: 0, count: Self.barCount)
    }
}

/// A small floating pill near the bottom of the screen. It's a non-activating panel, so it
/// never takes focus away from the app you're dictating into.
@MainActor
public final class OverlayPanel {
    public let model = OverlayModel()
    private let panel: NSPanel
    private let size = NSSize(width: 132, height: 36)

    public init() {
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
        panel.ignoresMouseEvents = true
        panel.hidesOnDeactivate = false
        panel.isReleasedWhenClosed = false
        panel.animationBehavior = .none
        let host = NSHostingView(rootView: OverlayView(model: model))
        host.frame = NSRect(origin: .zero, size: size)
        panel.contentView = host
    }

    public func show(_ phase: OverlayModel.Phase) {
        model.phase = phase
        if phase == .recording {
            model.resetLevels()
            model.isLocked = false
        }
        guard !panel.isVisible else { return }
        position()
        panel.orderFrontRegardless()
    }

    public func hide() {
        panel.orderOut(nil)
    }

    /// Shows a short message, then hides.
    public func flash(_ message: String, for duration: Duration = .seconds(2)) {
        show(.message(message))
        Task { @MainActor in
            try? await Task.sleep(for: duration)
            if case .message(message) = model.phase { hide() }
        }
    }

    /// Bottom-center of the screen containing the mouse, just above the Dock.
    private func position() {
        let mouse = NSEvent.mouseLocation
        let screen = NSScreen.screens.first { NSMouseInRect(mouse, $0.frame, false) } ?? NSScreen.main
        guard let visible = screen?.visibleFrame else { return }
        let origin = NSPoint(x: visible.midX - size.width / 2, y: visible.minY + 20)
        panel.setFrameOrigin(origin)
    }
}

struct OverlayView: View {
    @ObservedObject var model: OverlayModel

    var body: some View {
        ZStack {
            Capsule().fill(.black.opacity(0.85))
            Capsule().strokeBorder(.white.opacity(0.12), lineWidth: 1)
            content.padding(.horizontal, 14)
        }
        .frame(width: 132, height: 36)
    }

    @ViewBuilder private var content: some View {
        switch model.phase {
        case .recording:
            HStack(spacing: 3) {
                if model.isLocked {
                    Image(systemName: "lock.fill")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(.white.opacity(0.8))
                        .padding(.trailing, 3)
                }
                ForEach(Array(model.levels.enumerated()), id: \.offset) { _, level in
                    Capsule()
                        .fill(.white)
                        .frame(width: 3, height: 4 + CGFloat(level) * 18)
                }
            }
            .animation(.easeOut(duration: 0.08), value: model.levels)
        case .transcribing:
            ProgressView().controlSize(.small).tint(.white)
        case .message(let text):
            Text(text).font(.system(size: 11, weight: .medium)).foregroundStyle(.white).lineLimit(1)
        }
    }
}
