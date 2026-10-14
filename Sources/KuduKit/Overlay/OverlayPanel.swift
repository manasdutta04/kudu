import AppKit
import SwiftUI

/// Observable state the overlay pill renders.
@MainActor
public final class OverlayModel: ObservableObject {
    public enum Phase: Equatable {
        case recording
        case transcribing
        case message(String)

        var isMessage: Bool {
            if case .message = self { return true }
            return false
        }
    }

    @Published public var phase: Phase = .recording
    /// Hands-free recording: shown as a small pulsing dot instead of requiring the key.
    @Published public var isLocked = false
    /// Smoothed loudness history (0...1), newest first. The middle bar shows the newest value
    /// and older values ripple outward, so the wave follows the voice.
    @Published public private(set) var history: [Float] = Array(repeating: 0, count: OverlayModel.historyLength)

    static let historyLength = 5
    private var smoothed: Float = 0

    public init() {}

    /// Feed the RMS of one audio buffer (~20 ms).
    public func push(rms: Float) {
        // Perceived loudness is logarithmic: map −55 dBFS (room tone) … −12 dBFS (loud speech)
        // onto 0…1, so quiet speech still moves the bars and loud speech doesn't pin them.
        let decibels = 20 * log10(max(rms, 1e-6))
        let target = min(1, max(0, (decibels + 55) / 43))
        // Fast attack, slow release: jumps with each syllable, settles smoothly.
        let rate: Float = target > smoothed ? 0.55 : 0.18
        smoothed += (target - smoothed) * rate
        history.removeLast()
        history.insert(smoothed, at: 0)
    }

    public func resetLevels() {
        smoothed = 0
        history = Array(repeating: 0, count: Self.historyLength)
    }
}

/// A small floating pill near the bottom of the screen. It's a non-activating panel, so it
/// never takes focus away from the app you're dictating into.
@MainActor
public final class OverlayPanel {
    public let model = OverlayModel()
    private let panel: NSPanel
    /// The panel is a fixed transparent canvas; the pill inside sizes itself to its content.
    private let canvas = NSSize(width: 260, height: 44)

    public init() {
        panel = NSPanel(
            contentRect: NSRect(origin: .zero, size: canvas),
            styleMask: [.nonactivatingPanel, .borderless],
            backing: .buffered,
            defer: false
        )
        panel.level = .statusBar
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false // the pill draws its own soft shadow
        panel.ignoresMouseEvents = true
        panel.hidesOnDeactivate = false
        panel.isReleasedWhenClosed = false
        panel.animationBehavior = .none
        let host = NSHostingView(rootView: OverlayView(model: model))
        host.frame = NSRect(origin: .zero, size: canvas)
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

    /// Bottom-center of the screen containing the mouse: just above the Dock when the Dock is
    /// showing, otherwise close to the bottom edge (full-screen apps, auto-hidden Dock).
    private func position() {
        let mouse = NSEvent.mouseLocation
        guard let screen = NSScreen.screens.first(where: { NSMouseInRect(mouse, $0.frame, false) }) ?? NSScreen.main
        else { return }
        let dockShowing = !ScreenPlacement.dockAutoHides && !ScreenPlacement.frontWindowIsFullScreen()
        // visibleFrame always reserves the Dock's space, even when a full-screen app hides it.
        let floor = dockShowing ? screen.visibleFrame.minY : screen.frame.minY
        let gap: CGFloat = dockShowing ? 6 : 10
        // The pill is vertically centered in the transparent canvas.
        let pillHeight: CGFloat = 26
        let y = floor + gap - (canvas.height - pillHeight) / 2
        panel.setFrameOrigin(NSPoint(x: screen.frame.midX - canvas.width / 2, y: y))
    }
}

/// Facts about the screen layout that NSScreen doesn't expose.
@MainActor
enum ScreenPlacement {
    static var dockAutoHides: Bool {
        UserDefaults(suiteName: "com.apple.dock")?.bool(forKey: "autohide") ?? false
    }

    /// Whether the focused window of the frontmost app is in full-screen mode (its own Space,
    /// Dock hidden). Uses Accessibility, with a short timeout so a busy app can't stall us.
    static func frontWindowIsFullScreen() -> Bool {
        guard let pid = NSWorkspace.shared.frontmostApplication?.processIdentifier else { return false }
        let app = AXUIElementCreateApplication(pid)
        AXUIElementSetMessagingTimeout(app, 0.1)
        var window: CFTypeRef?
        guard AXUIElementCopyAttributeValue(app, kAXFocusedWindowAttribute as CFString, &window) == .success,
              let window, CFGetTypeID(window) == AXUIElementGetTypeID()
        else { return false }
        var fullScreen: CFTypeRef?
        guard AXUIElementCopyAttributeValue(window as! AXUIElement, "AXFullScreen" as CFString, &fullScreen) == .success
        else { return false }
        return (fullScreen as? Bool) ?? false
    }
}

// MARK: - Views

struct OverlayView: View {
    @ObservedObject var model: OverlayModel

    var body: some View {
        content
            .padding(.horizontal, 12)
            .frame(height: 26)
            .background(GlassBackground())
            .clipShape(Capsule())
            .overlay(Capsule().strokeBorder(.white.opacity(0.14), lineWidth: 0.5))
            .shadow(color: .black.opacity(0.25), radius: 6, y: 2)
            .frame(maxWidth: .infinity, maxHeight: .infinity) // centered in the canvas
            .animation(.spring(response: 0.28, dampingFraction: 0.85), value: model.phase)
            .animation(.spring(response: 0.28, dampingFraction: 0.85), value: model.isLocked)
    }

    @ViewBuilder private var content: some View {
        switch model.phase {
        case .recording:
            HStack(spacing: 7) {
                if model.isLocked {
                    PulsingDot().transition(.scale.combined(with: .opacity))
                }
                Waveform(history: model.history)
            }
        case .transcribing:
            ThinkingDots()
        case .message(let text):
            Text(text)
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.white.opacity(0.92))
                .lineLimit(1)
        }
    }
}

/// Nine thin bars, mirrored around the middle. The middle bar shows the newest loudness and
/// each step outward shows a slightly older one with a gentle falloff, so speech ripples.
private struct Waveform: View {
    let history: [Float]

    private static let barCount = 9
    private static let minHeight: CGFloat = 3
    private static let maxHeight: CGFloat = 14

    var body: some View {
        HStack(spacing: 2.5) {
            ForEach(0..<Self.barCount, id: \.self) { index in
                Capsule()
                    .fill(.white.opacity(0.95))
                    .frame(width: 2.5, height: height(for: index))
            }
        }
        .frame(height: Self.maxHeight)
        .animation(.linear(duration: 0.06), value: history)
    }

    private func height(for index: Int) -> CGFloat {
        let center = Self.barCount / 2
        let distance = abs(index - center)
        let level = CGFloat(history[min(distance, history.count - 1)])
        // Cosine-squared falloff: tall in the middle, tapering to about a third at the edges,
        // so the outline stays rounded like a voice instead of a flat block.
        let x = CGFloat(distance) / CGFloat(center + 1)
        let falloff = 0.25 + 0.75 * pow(cos(x * .pi / 2), 2)
        return Self.minHeight + (Self.maxHeight - Self.minHeight) * level * falloff
    }
}

/// A small red dot that breathes: recording continues without holding the key.
private struct PulsingDot: View {
    var body: some View {
        TimelineView(.animation) { context in
            let t = context.date.timeIntervalSinceReferenceDate
            let pulse = 0.55 + 0.45 * (0.5 + 0.5 * sin(t * 2 * .pi / 1.6))
            Circle()
                .fill(Color(red: 1, green: 0.27, blue: 0.23))
                .frame(width: 6, height: 6)
                .opacity(pulse)
                .shadow(color: Color(red: 1, green: 0.27, blue: 0.23).opacity(0.6 * pulse), radius: 3)
        }
    }
}

/// Three dots that rise and fall in turn while the speech model works.
private struct ThinkingDots: View {
    var body: some View {
        TimelineView(.animation) { context in
            let t = context.date.timeIntervalSinceReferenceDate
            HStack(spacing: 4) {
                ForEach(0..<3, id: \.self) { index in
                    let phase = sin((t * 2 * .pi / 0.9) - Double(index) * 0.7)
                    Circle()
                        .fill(.white)
                        .frame(width: 4, height: 4)
                        .opacity(0.45 + 0.55 * (0.5 + 0.5 * phase))
                        .offset(y: -1.5 * phase)
                }
            }
            .frame(width: 40)
        }
    }
}

/// Frosted glass that blurs whatever is behind the panel (behind-window blending), with a
/// light dark tint so white bars stay readable on any wallpaper.
private struct GlassBackground: NSViewRepresentable {
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
