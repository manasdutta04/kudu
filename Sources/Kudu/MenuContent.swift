import KuduKit
import SwiftUI

struct MenuContent: View {
    @ObservedObject var controller: DictationController
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Kudu").font(.headline)
                Spacer()
                StatusBadge(controller: controller)
            }

            if let last = controller.history.first {
                VStack(alignment: .leading, spacing: 6) {
                    Text(last.text)
                        .font(.callout)
                        .lineLimit(4)
                        .textSelection(.enabled)
                    HStack {
                        Text("\(last.latencyMs) ms after release").font(.caption).foregroundStyle(.secondary)
                        Spacer()
                        Button("Copy") { controller.copyToClipboard(last) }
                        Button("Copy raw") { controller.copyToClipboard(last, raw: true) }
                            .help("The transcript before cleanup")
                    }
                    .controlSize(.small)
                }
                if controller.history.count > 1 {
                    DisclosureGroup("Recent (\(controller.history.count - 1))") {
                        ForEach(controller.history.dropFirst()) { item in
                            Button { controller.copyToClipboard(item) } label: {
                                Text(item.text).lineLimit(1).frame(maxWidth: .infinity, alignment: .leading)
                            }
                            .buttonStyle(.plain)
                            .help("Click to copy")
                        }
                        Button("Clear history") { controller.clearHistory() }.controlSize(.small)
                    }
                    .font(.callout)
                }
            } else {
                Text("Hold \(controller.trigger.displayName), speak, release.")
                    .font(.callout).foregroundStyle(.secondary)
            }

            Divider()

            Picker("Dictation key", selection: $controller.trigger) {
                ForEach(TriggerKey.allCases) { key in Text(key.displayName).tag(key) }
            }
            Toggle("Keep microphone warm", isOn: $controller.keepMicWarm)
                .help("Removes start-up delay and keeps the first syllable. The mic indicator stays on.")

            Divider()

            HStack {
                Button("Setup…") {
                    NSApp.activate(ignoringOtherApps: true)
                    openWindow(id: "onboarding")
                }
                Spacer()
                Button("Quit Kudu") { NSApp.terminate(nil) }
                    .keyboardShortcut("q")
            }
        }
        .padding(14)
        .frame(width: 300)
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
