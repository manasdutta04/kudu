import KuduKit
import SwiftUI

/// First-run setup: the two permissions, then the one-time model download.
struct OnboardingView: View {
    @ObservedObject var controller: DictationController
    // ObservableObject rather than @State: @State is a macro on newer SDKs, and the
    // Command Line Tools ship without the SwiftUI macro plugin.
    @StateObject private var permissions = PermissionState()

    private let poll = Timer.publish(every: 1, on: .main, in: .common).autoconnect()

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Welcome to Kudu").font(.title2.bold())
                Text("Private dictation that runs entirely on your Mac. Nothing you say leaves this computer.")
                    .foregroundStyle(.secondary)
            }

            Step(
                number: 1, title: "Microphone", detail: "To hear you while you hold the dictation key.",
                done: permissions.mic
            ) {
                Button("Allow") {
                    Task {
                        permissions.mic = await Permissions.requestMicrophone()
                        if !permissions.mic { Permissions.openSettings(.microphone) }
                        controller.activate()
                    }
                }
            }

            Step(
                number: 2, title: "Accessibility",
                detail: "To notice the dictation key and type the text into other apps.",
                done: permissions.accessibility
            ) {
                Button("Open Settings") {
                    Permissions.requestAccessibility()
                    Permissions.openSettings(.accessibility)
                }
            }

            Step(
                number: 3, title: "Speech model",
                detail: "Parakeet (about 450 MB), downloaded once. After that Kudu works offline.",
                done: controller.modelState == .ready
            ) {
                modelAction
            }

            HStack {
                Spacer()
                Text("Hold \(controller.trigger.displayName) anywhere to dictate.")
                    .font(.callout).foregroundStyle(.secondary)
            }
        }
        .padding(24)
        .frame(width: 460)
        .onReceive(poll) { _ in
            let ax = Permissions.accessibilityGranted
            permissions.mic = Permissions.microphoneGranted
            if ax != permissions.accessibility || (ax && !controller.hotkeyActive) {
                permissions.accessibility = ax
                controller.activate()
            }
        }
    }

    @ViewBuilder private var modelAction: some View {
        switch controller.modelState {
        case .missing, .failed:
            VStack(alignment: .trailing) {
                Button("Download") { Task { await controller.downloadModel() } }
                if case .failed(let message) = controller.modelState {
                    Text(message).font(.caption).foregroundStyle(.red).lineLimit(2)
                }
            }
        case .downloading(let fraction):
            ProgressView(value: fraction).frame(width: 120)
        case .loading:
            ProgressView().controlSize(.small)
        case .ready:
            EmptyView()
        }
    }
}

@MainActor
private final class PermissionState: ObservableObject {
    @Published var mic = Permissions.microphoneGranted
    @Published var accessibility = Permissions.accessibilityGranted
}

private struct Step<Action: View>: View {
    let number: Int
    let title: String
    let detail: String
    let done: Bool
    @ViewBuilder let action: () -> Action

    var body: some View {
        HStack(alignment: .center, spacing: 12) {
            ZStack {
                Circle().fill(done ? Color.green : Color.secondary.opacity(0.2)).frame(width: 26, height: 26)
                if done {
                    Image(systemName: "checkmark").font(.caption.bold()).foregroundStyle(.white)
                } else {
                    Text("\(number)").font(.caption.bold())
                }
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.headline)
                Text(detail).font(.callout).foregroundStyle(.secondary)
            }
            Spacer()
            if !done { action() }
        }
    }
}
