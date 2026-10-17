import Combine
import KuduKit
import SwiftUI

/// Live permission state, refreshed every second while the setup card is visible (macOS
/// doesn't notify apps when Accessibility is granted).
@MainActor
final class SetupState: ObservableObject {
    @Published var microphone = Permissions.microphoneGranted
    @Published var accessibility = Permissions.accessibilityGranted
    /// The speech model picked on the card (until then, the one in settings).
    @Published var choice: SpeechModel?

    func refresh(_ controller: DictationController) {
        microphone = Permissions.microphoneGranted
        let granted = Permissions.accessibilityGranted
        if granted != accessibility || (granted && !controller.hotkeyActive) {
            accessibility = granted
            controller.activate()
        }
    }
}

/// "Finish setting up" on the Home page: microphone, Accessibility and the speech model,
/// each with its action and live status. Hidden once everything is ready.
struct SetupCard: View {
    @ObservedObject var controller: DictationController
    @StateObject private var state = SetupState()
    private let poll = Timer.publish(every: 1, on: .main, in: .common).autoconnect()

    var body: some View {
        Group {
            if !isComplete {
                VStack(alignment: .leading, spacing: 18) {
                    header
                    VStack(spacing: 0) {
                        SetupStep(
                            icon: "mic.fill", title: "Microphone", detail: "To hear you while you dictate.",
                            done: state.microphone
                        ) {
                            Button("Allow") {
                                Task {
                                    state.microphone = await Permissions.requestMicrophone()
                                    if !state.microphone { Permissions.openSettings(.microphone) }
                                    controller.activate()
                                }
                            }
                        }
                        Divider().padding(.leading, 52)
                        SetupStep(
                            icon: "accessibility", title: "Accessibility",
                            detail: "To notice the dictation key and type into other apps.", done: state.accessibility
                        ) {
                            Button("Open Settings") {
                                Permissions.requestAccessibility()
                                Permissions.openSettings(.accessibility)
                            }
                        }
                        accessibilityHelp
                        Divider().padding(.leading, 52)
                        SetupStep(icon: "waveform", title: "Speech model", detail: modelDetail, done: modelDone) {
                            EmptyView()
                        }
                        if !modelDone { modelChooser.padding(.leading, 52).padding(.bottom, 4) }
                    }
                }
                .padding(22)
                .card()
            }
        }
        .onReceive(poll) { _ in state.refresh(controller) }
    }

    // MARK: Header

    private var stepsDone: Int {
        [state.microphone, state.accessibility && controller.hotkeyActive, modelDone].filter { $0 }.count
    }

    private var header: some View {
        HStack(alignment: .top, spacing: 16) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Finish setting up").font(.system(size: 17, weight: .semibold))
                Text("Three quick steps, then hold \(controller.shortcuts.dictate.displayName) anywhere to dictate. Everything stays on your Mac.")
                    .font(.callout).foregroundStyle(.secondary)
            }
            Spacer(minLength: 12)
            VStack(alignment: .trailing, spacing: 6) {
                Text("\(stepsDone) of 3 done").font(.caption.weight(.medium)).foregroundStyle(.secondary)
                HStack(spacing: 4) {
                    ForEach(0..<3, id: \.self) { index in
                        Capsule()
                            .fill(index < stepsDone ? Theme.accent : Theme.selection)
                            .frame(width: 22, height: 5)
                    }
                }
                .animation(.easeOut(duration: 0.3), value: stepsDone)
            }
        }
    }

    // MARK: Speech model

    private var selected: SpeechModel { state.choice ?? controller.speechModel }

    private var modelDone: Bool {
        switch controller.modelState {
        case .ready, .loading: true
        default: false
        }
    }

    private var modelDetail: String {
        switch controller.modelState {
        case .ready: "\(controller.speechModel.name) is ready. Works offline."
        case .loading: "\(controller.speechModel.name) is downloaded. Getting it ready…"
        default: "Choose a size. It's downloaded once and works offline after that. Formatting uses Apple Intelligence, so there's nothing else to download."
        }
    }

    private var isComplete: Bool {
        state.microphone && state.accessibility && controller.hotkeyActive && modelDone
    }

    private var modelChooser: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 10) {
                ForEach(SpeechModel.allCases) { model in
                    ModelOption(model: model, selected: model == selected, recommended: model == .base) {
                        if !isDownloading { state.choice = model }
                    }
                }
            }
            switch controller.modelState {
            case .downloading(let fraction):
                VStack(alignment: .leading, spacing: 6) {
                    ProgressView(value: fraction).tint(Theme.accent)
                    Text("Downloading \(controller.speechModel.name)… \(Int(fraction * 100))% · \(Int(fraction * Double(controller.speechModel.downloadMB))) of \(controller.speechModel.downloadMB) MB")
                        .font(.caption).foregroundStyle(.secondary).monospacedDigit()
                }
            case .failed(let message):
                HStack(spacing: 10) {
                    downloadButton(title: "Try again")
                    Text(message).font(.caption).foregroundStyle(.red).lineLimit(2)
                }
            default:
                downloadButton(title: "Download \(selected.name) · \(selected.downloadMB) MB")
            }
        }
    }

    private var isDownloading: Bool {
        if case .downloading = controller.modelState { return true }
        return false
    }

    private func downloadButton(title: String) -> some View {
        Button(title) { Task { await controller.chooseModel(selected) } }
            .buttonStyle(QuietButtonStyle(prominent: true))
    }

    // MARK: Accessibility help

    /// Kudu is switched on in System Settings but macOS still doesn't trust this copy: the
    /// entry belongs to an older build. Or it was just granted and needs a relaunch.
    @ViewBuilder private var accessibilityHelp: some View {
        if !state.accessibility {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text("Already switched on but not detected? Reset it, then switch Kudu on again.")
                    .font(.caption).foregroundStyle(.secondary)
                Button("Reset permission") {
                    Permissions.resetAccessibility()
                    Permissions.openSettings(.accessibility)
                }
                .buttonStyle(QuietButtonStyle())
            }
            .padding(.leading, 52).padding(.bottom, 10)
        } else if !controller.hotkeyActive {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text("Accessibility is on. Relaunch Kudu to start listening.")
                    .font(.caption).foregroundStyle(.secondary)
                Button("Relaunch") { Permissions.relaunch() }.buttonStyle(QuietButtonStyle())
            }
            .padding(.leading, 52).padding(.bottom, 10)
        }
    }
}

/// One step of setup: an icon tile, what it's for, and its action (or a "Done" mark).
private struct SetupStep<Action: View>: View {
    let icon: String
    let title: String
    let detail: String
    let done: Bool
    @ViewBuilder let action: () -> Action

    var body: some View {
        HStack(spacing: 14) {
            ZStack {
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .fill(done ? Color.green.opacity(0.16) : Theme.selection)
                    .frame(width: 38, height: 38)
                Image(systemName: done ? "checkmark" : icon)
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(done ? Color.green : Color.primary)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 13.5, weight: .semibold))
                Text(detail).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 12)
            if done {
                Text("Done").font(.caption.weight(.semibold)).foregroundStyle(.green)
            } else {
                action().buttonStyle(QuietButtonStyle(prominent: true))
            }
        }
        .padding(.vertical, 10)
        .animation(.easeOut(duration: 0.25), value: done)
    }
}

/// A selectable speech model: its Kudu name, size and what it's good for.
struct ModelOption: View {
    let model: SpeechModel
    let selected: Bool
    var recommended = false
    let action: () -> Void
    @StateObject private var hover = HoverState()

    var body: some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    ZStack {
                        Circle().strokeBorder(selected ? Theme.accent : Color.secondary.opacity(0.5), lineWidth: 1.5)
                            .frame(width: 16, height: 16)
                        if selected { Circle().fill(Theme.accent).frame(width: 8, height: 8) }
                    }
                    Text(model.name).font(.system(size: 13.5, weight: .semibold))
                    Spacer(minLength: 4)
                    Text("\(model.downloadMB) MB").font(.caption.weight(.medium)).foregroundStyle(.secondary).monospacedDigit()
                }
                Text(model.summary).font(.caption).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                if recommended {
                    Text("Recommended").font(.caption2.weight(.semibold)).foregroundStyle(Theme.accent)
                        .padding(.horizontal, 7).padding(.vertical, 2)
                        .background(Capsule().fill(Theme.accent.opacity(0.12)))
                }
            }
            .padding(12)
            .frame(maxWidth: .infinity, minHeight: 92, alignment: .topLeading)
            .background(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(selected ? Theme.accent.opacity(0.08) : (hover.isHovering ? Theme.selection.opacity(0.6) : Color.clear))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .strokeBorder(selected ? Theme.accent : Theme.hairline, lineWidth: selected ? 1.5 : 1)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hover.isHovering = $0 }
        .animation(.easeOut(duration: 0.15), value: selected)
    }
}
