import KuduCore
import KuduKit
import SwiftUI

/// Text fields for a new entry. ObservableObject rather than @State: @State is a macro on
/// newer SDKs and the Command Line Tools ship without the SwiftUI macro plugin.
@MainActor
private final class Draft: ObservableObject {
    @Published var first = ""
    @Published var second = ""

    func clear() {
        first = ""
        second = ""
    }
}

/// A rounded list card used by the Dictionary and Snippets pages.
private struct Card<Content: View>: View {
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(spacing: 0, content: content)
            .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 14))
            .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(.quaternary))
    }
}

private struct PageScroll<Content: View>: View {
    @ViewBuilder let content: () -> Content

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22, content: content)
                .padding(36)
                .frame(maxWidth: 820, alignment: .leading)
                .frame(maxWidth: .infinity)
        }
    }
}

// MARK: - Dictionary

struct DictionaryPage: View {
    @ObservedObject var store: UserDataStore
    @StateObject private var draft = Draft()

    var body: some View {
        PageScroll {
            PageHeader(
                title: "Dictionary",
                subtitle: "Names, products and jargon Kudu should spell your way. Add how it tends to mishear them."
            )
            HStack(spacing: 10) {
                TextField("Spelling, e.g. Kudu", text: $draft.first)
                TextField("Heard as, e.g. kudoo, coo doo", text: $draft.second)
                Button("Add") {
                    let heard = draft.second.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }
                    store.dictionary.append(DictionaryEntry(heard: heard, written: draft.first))
                    draft.clear()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(draft.first.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            .textFieldStyle(.roundedBorder)

            if store.dictionary.isEmpty {
                Text("No words yet.").foregroundStyle(.secondary)
            } else {
                Card {
                    ForEach(Array(store.dictionary.enumerated()), id: \.element.id) { index, entry in
                        if index > 0 { Divider().padding(.leading, 18) }
                        HStack {
                            Text(entry.written).fontWeight(.medium)
                            Text(entry.heard.joined(separator: ", ")).foregroundStyle(.secondary)
                            Spacer()
                            Button(role: .destructive) {
                                store.dictionary.removeAll { $0.id == entry.id }
                            } label: { Image(systemName: "trash") }
                                .buttonStyle(.borderless)
                        }
                        .padding(.horizontal, 18)
                        .padding(.vertical, 12)
                    }
                }
            }
        }
    }
}

// MARK: - Snippets

struct SnippetsPage: View {
    @ObservedObject var store: UserDataStore
    @StateObject private var draft = Draft()

    var body: some View {
        PageScroll {
            PageHeader(
                title: "Snippets",
                subtitle: "Say a trigger on its own to insert saved text. Placeholders: {date}, {time}, {clipboard}."
            )
            HStack(spacing: 10) {
                TextField("Trigger, e.g. my email", text: $draft.first)
                TextField("Inserts", text: $draft.second)
                Button("Add") {
                    store.snippets.append(Snippet(trigger: draft.first, expansion: draft.second))
                    draft.clear()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(draft.first.isEmpty || draft.second.isEmpty)
            }
            .textFieldStyle(.roundedBorder)

            if store.snippets.isEmpty {
                Text("No snippets yet.").foregroundStyle(.secondary)
            } else {
                Card {
                    ForEach(Array(store.snippets.enumerated()), id: \.element.id) { index, snippet in
                        if index > 0 { Divider().padding(.leading, 18) }
                        HStack(alignment: .top) {
                            Text("“\(snippet.trigger)”").fontWeight(.medium).frame(width: 160, alignment: .leading)
                            Text(snippet.expansion).foregroundStyle(.secondary).lineLimit(3)
                            Spacer()
                            Button(role: .destructive) {
                                store.snippets.removeAll { $0.id == snippet.id }
                            } label: { Image(systemName: "trash") }
                                .buttonStyle(.borderless)
                        }
                        .padding(.horizontal, 18)
                        .padding(.vertical, 12)
                    }
                }
            }
        }
    }
}

// MARK: - Style

struct StylePage: View {
    @ObservedObject var controller: DictationController
    @ObservedObject var store: UserDataStore

    var body: some View {
        PageScroll {
            PageHeader(title: "Style", subtitle: "How Kudu turns speech into clean, well-formatted text.")

            Form {
                Section {
                    Picker("Smart formatting", selection: $store.cleanup.smartFormatting) {
                        ForEach(SmartFormatting.allCases) { Text($0.title).tag($0) }
                    }
                    .pickerStyle(.segmented)
                    .disabled(!controller.smartFormattingAvailable)
                    Text(formattingNote)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                } header: {
                    Text("Formatting")
                }

                Section {
                    Toggle("Remove filler words (um, uh)", isOn: $store.cleanup.removeFillers)
                    Toggle("Apply spoken corrections (\"Tuesday, no, Wednesday\")", isOn: $store.cleanup.applySelfCorrections)
                    Toggle("\"New line\" and \"new paragraph\" commands", isOn: $store.cleanup.spokenCommands)
                    Toggle("Write times as 10:30", isOn: $store.cleanup.normalizeTimes)
                } header: {
                    Text("Cleanup")
                } footer: {
                    Text("Cleanup rules run instantly on every dictation.").font(.caption).foregroundStyle(.secondary)
                }
            }
            .formStyle(.grouped)
            .scrollDisabled(true)
            .frame(minHeight: 380)
        }
    }

    private var formattingNote: String {
        guard controller.smartFormattingAvailable else {
            return "Needs macOS 26 or later with Apple Intelligence turned on. Kudu uses the rule cleanup below instead."
        }
        return "Uses Apple's on-device model to add punctuation, paragraphs, lists and written numbers, without changing your words. Longer dictations (\(SmartFormatting.minimumWords)+ words) take about a second extra; short phrases stay instant. Nothing leaves your Mac."
    }
}

// MARK: - Settings

@MainActor
private final class ConfirmState: ObservableObject {
    @Published var clearHistory = false
}

struct SettingsPage: View {
    @ObservedObject var controller: DictationController
    @ObservedObject var history: HistoryStore
    @StateObject private var launchAtLogin = LaunchAtLogin()
    @StateObject private var confirm = ConfirmState()

    var body: some View {
        PageScroll {
            PageHeader(title: "Settings", subtitle: "Keys, sounds, microphone and privacy.")

            Form {
                Section("Dictation") {
                    Picker("Dictation key", selection: $controller.trigger) {
                        ForEach(TriggerKey.allCases) { Text($0.displayName).tag($0) }
                    }
                    Picker("Recording", selection: $controller.recordingMode) {
                        ForEach(RecordingMode.allCases) { Text($0.title).tag($0) }
                    }
                    if controller.trigger == .fn {
                        Toggle("Stop fn from opening the emoji picker", isOn: $controller.preventEmojiPicker)
                    }
                }

                Section("Feedback & microphone") {
                    Toggle("Sounds", isOn: $controller.soundsEnabled)
                    Toggle("Keep microphone always warm", isOn: $controller.keepMicWarm)
                    Text("The microphone stays warm for a minute after each dictation so the next one starts instantly. Audio is only ever held in memory.")
                        .font(.caption).foregroundStyle(.secondary)
                }

                Section("General") {
                    Toggle("Open Kudu at login", isOn: Binding(get: { launchAtLogin.isEnabled }, set: { launchAtLogin.set($0) }))
                }

                Section("History") {
                    Picker("Keep history", selection: $history.retention) {
                        ForEach(HistoryRetention.allCases) { Text($0.title).tag($0) }
                    }
                    HStack {
                        Text("\(history.records.count) dictations saved on this Mac")
                            .foregroundStyle(.secondary)
                        Spacer()
                        Button("Clear history…", role: .destructive) { confirm.clearHistory = true }
                            .disabled(history.records.isEmpty)
                    }
                }

                Section("Permissions") {
                    PermissionRow(title: "Microphone", granted: Permissions.microphoneGranted) {
                        Permissions.openSettings(.microphone)
                    }
                    PermissionRow(title: "Accessibility", granted: Permissions.accessibilityGranted) {
                        Permissions.openSettings(.accessibility)
                    }
                }

                Section("About") {
                    LabeledContent("Version", value: Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "–")
                    LabeledContent("Speech model", value: "NVIDIA Parakeet TDT 0.6B v2 (on device)")
                    LabeledContent("Formatting", value: controller.smartFormattingAvailable ? "Apple on-device model" : "Rules only")
                }
            }
            .formStyle(.grouped)
            .scrollDisabled(true)
            .frame(minHeight: 820)
        }
        .confirmationDialog("Clear all dictation history?", isPresented: $confirm.clearHistory) {
            Button("Clear history", role: .destructive) { history.clear() }
        } message: {
            Text("This deletes every saved dictation from this Mac. It can't be undone.")
        }
    }
}

private struct PermissionRow: View {
    let title: String
    let granted: Bool
    let open: () -> Void

    var body: some View {
        HStack {
            Text(title)
            Spacer()
            if granted {
                Label("Allowed", systemImage: "checkmark.circle.fill").foregroundStyle(.green)
            } else {
                Button("Open Settings", action: open)
            }
        }
    }
}
