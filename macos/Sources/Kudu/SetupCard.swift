import Combine
import KuduKit
import SwiftUI

/// Live permission state, refreshed every second while the setup card is visible (macOS
/// doesn't notify apps when Accessibility is granted).
@MainActor
final class SetupState: ObservableObject {
    @Published var microphone = Permissions.microphoneGranted
    @Published var accessibility = Permissions.accessibilityGranted

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
                VStack(alignment: .leading, spacing: 16) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Finish setting up").font(.system(size: 17, weight: .semibold))
                        Text("Three quick steps, then hold \(controller.trigger.displayName) anywhere to dictate. Everything stays on your Mac.")
                            .font(.callout).foregroundStyle(.secondary)
                    }
                    SetupStep(number: 1, title: "Microphone", detail: "To hear you while you dictate.", done: state.microphone) {
                        Button("Allow") {
                            Task {
                                state.microphone = await Permissions.requestMicrophone()
                                if !state.microphone { Permissions.openSettings(.microphone) }
                                controller.activate()
                            }
                        }
                    }
                    SetupStep(
                        number: 2, title: "Accessibility",
                        detail: "To notice the dictation key and type into other apps.", done: state.accessibility
                    ) {
                        Button("Open Settings") {
                            Permissions.requestAccessibility()
                            Permissions.openSettings(.accessibility)
                        }
                    }
                    accessibilityHelp
                    SetupStep(
                        number: 3, title: "Speech model",
                        detail: "Parakeet, about 450 MB, downloaded once. Works offline after that.",
                        done: modelDone
                    ) {
                        modelAction
                    }
                }
                .padding(22)
                .card()
            }
        }
        .onReceive(poll) { _ in state.refresh(controller) }
    }

    private var modelDone: Bool {
        switch controller.modelState {
        case .ready, .loading: true
        default: false
        }
    }

    private var isComplete: Bool {
        state.microphone && state.accessibility && controller.hotkeyActive && modelDone
    }

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
                .controlSize(.small)
            }
            .padding(.leading, 40)
        } else if !controller.hotkeyActive {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text("Accessibility is on. Relaunch Kudu to start listening.")
                    .font(.caption).foregroundStyle(.secondary)
                Button("Relaunch") { Permissions.relaunch() }.controlSize(.small)
            }
            .padding(.leading, 40)
        }
    }

    @ViewBuilder private var modelAction: some View {
        switch controller.modelState {
        case .missing, .failed:
            VStack(alignment: .trailing, spacing: 4) {
                Button("Download") { Task { await controller.downloadModel() } }
                if case .failed(let message) = controller.modelState {
                    Text(message).font(.caption).foregroundStyle(.red).lineLimit(2)
                }
            }
        case .downloading(let fraction):
            ProgressView(value: fraction).frame(width: 140)
        case .loading, .ready:
            EmptyView()
        }
    }
}

private struct SetupStep<Action: View>: View {
    let number: Int
    let title: String
    let detail: String
    let done: Bool
    @ViewBuilder let action: () -> Action

    var body: some View {
        HStack(spacing: 14) {
            ZStack {
                Circle()
                    .fill(done ? Color.green : Theme.selection)
                    .frame(width: 26, height: 26)
                if done {
                    Image(systemName: "checkmark").font(.system(size: 11, weight: .bold)).foregroundStyle(.white)
                } else {
                    Text("\(number)").font(.system(size: 12, weight: .semibold))
                }
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 13.5, weight: .semibold))
                Text(detail).font(.callout).foregroundStyle(.secondary)
            }
            Spacer()
            if !done { action() }
        }
    }
}
