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

/// Scrollable page body with the shared margins.
private struct PageScroll<Content: View>: View {
    @ViewBuilder let content: () -> Content

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24, content: content)
                .padding(.horizontal, 40)
                .padding(.vertical, 42)
                .frame(maxWidth: 820, alignment: .leading)
                .frame(maxWidth: .infinity)
        }
    }
}

/// A titled group of rows inside a card, like a grouped settings section.
private struct Section<Content: View>: View {
    let title: String
    var footer: String?
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title.uppercased())
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.secondary)
                .tracking(0.8)
                .padding(.leading, 4)
            VStack(alignment: .leading, spacing: 0, content: content)
                .card(radius: 14)
            if let footer {
                Text(footer).font(.caption).foregroundStyle(.secondary).padding(.horizontal, 4)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

/// One row in a `Section`: a label (with optional detail) and a control.
private struct Row<Control: View>: View {
    let title: String
    var detail: String?
    var divider = true
    @ViewBuilder let control: () -> Control

    var body: some View {
        VStack(spacing: 0) {
            HStack(alignment: .center, spacing: 16) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.system(size: 13.5))
                    if let detail {
                        Text(detail).font(.caption).foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                Spacer(minLength: 12)
                control()
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 12)
            if divider { Divider().overlay(Theme.hairline).padding(.leading, 18) }
        }
    }
}

/// A switch in the app's accent color.
private struct AccentSwitch: View {
    @Binding var isOn: Bool

    var body: some View {
        Toggle("", isOn: $isOn).labelsHidden().toggleStyle(.switch).tint(Theme.accent).controlSize(.small)
    }
}

/// A soft text field that matches the cards.
private struct Field: View {
    let prompt: String
    @Binding var text: String

    var body: some View {
        TextField(prompt, text: $text)
            .textFieldStyle(.plain)
            .font(.system(size: 13))
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(Theme.card, in: RoundedRectangle(cornerRadius: 9, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 9, style: .continuous).strokeBorder(Theme.hairline))
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
                Field(prompt: "Spelling, e.g. Kudu", text: $draft.first)
                Field(prompt: "Heard as, e.g. kudoo, coo doo", text: $draft.second)
                Button("Add") {
                    let heard = draft.second.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }
                    store.dictionary.append(DictionaryEntry(heard: heard, written: draft.first))
                    draft.clear()
                }
                .keyboardShortcut(.defaultAction)
                .buttonStyle(QuietButtonStyle(prominent: true))
                .disabled(draft.first.trimmingCharacters(in: .whitespaces).isEmpty)
            }

            if store.dictionary.isEmpty {
                EmptyState(symbol: "character.book.closed", text: "No words yet. Add names and terms Kudu gets wrong.")
            } else {
                VStack(spacing: 0) {
                    ForEach(Array(store.dictionary.enumerated()), id: \.element.id) { index, entry in
                        if index > 0 { Divider().overlay(Theme.hairline).padding(.leading, 18) }
                        HStack(spacing: 14) {
                            Text(entry.written).font(.system(size: 13.5, weight: .medium))
                            Text(entry.heard.joined(separator: ", ")).foregroundStyle(.secondary)
                            Spacer()
                            Button(role: .destructive) {
                                store.dictionary.removeAll { $0.id == entry.id }
                            } label: { Image(systemName: "trash").font(.system(size: 12)) }
                                .buttonStyle(.borderless)
                                .foregroundStyle(.secondary)
                        }
                        .padding(.horizontal, 18)
                        .padding(.vertical, 12)
                    }
                }
                .card(radius: 14)
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
                Field(prompt: "Trigger, e.g. my email", text: $draft.first)
                Field(prompt: "Inserts", text: $draft.second)
                Button("Add") {
                    store.snippets.append(Snippet(trigger: draft.first, expansion: draft.second))
                    draft.clear()
                }
                .keyboardShortcut(.defaultAction)
                .buttonStyle(QuietButtonStyle(prominent: true))
                .disabled(draft.first.isEmpty || draft.second.isEmpty)
            }

            if store.snippets.isEmpty {
                EmptyState(symbol: "text.badge.plus", text: "No snippets yet. Try “my email” → your address.")
            } else {
                VStack(spacing: 0) {
                    ForEach(Array(store.snippets.enumerated()), id: \.element.id) { index, snippet in
                        if index > 0 { Divider().overlay(Theme.hairline).padding(.leading, 18) }
                        HStack(alignment: .top, spacing: 14) {
                            Text("“\(snippet.trigger)”").font(.system(size: 13.5, weight: .medium))
                                .frame(width: 170, alignment: .leading)
                            Text(snippet.expansion).foregroundStyle(.secondary).lineLimit(3)
                            Spacer()
                            Button(role: .destructive) {
                                store.snippets.removeAll { $0.id == snippet.id }
                            } label: { Image(systemName: "trash").font(.system(size: 12)) }
                                .buttonStyle(.borderless)
                                .foregroundStyle(.secondary)
                        }
                        .padding(.horizontal, 18)
                        .padding(.vertical, 12)
                    }
                }
                .card(radius: 14)
            }
        }
    }
}

private struct EmptyState: View {
    let symbol: String
    let text: String

    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: symbol).font(.system(size: 24)).foregroundStyle(.tertiary)
            Text(text).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 40)
        .card(radius: 14)
    }
}

// MARK: - Style

struct StylePage: View {
    @ObservedObject var controller: DictationController
    @ObservedObject var store: UserDataStore

    var body: some View {
        PageScroll {
            PageHeader(title: "Style", subtitle: "How Kudu turns what you say into clean, well-formatted text.")

            Section(title: "Editing", footer: editingFooter) {
                VStack(alignment: .leading, spacing: 10) {
                    ChipPicker(items: EditingStrength.allCases, selection: $store.cleanup.editingStrength) { $0.title }
                    Text(store.cleanup.editingStrength.summary).font(.callout).foregroundStyle(.secondary)
                }
                .padding(18)
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            Section(title: "Style by app", footer: "Kudu picks the style from the app you're typing in.") {
                StyleRow(title: "Personal messages", apps: "Messages, WhatsApp, Telegram", style: $store.cleanup.styles.personalMessages)
                StyleRow(title: "Work messages", apps: "Slack, Teams, Discord", style: $store.cleanup.styles.workMessages)
                StyleRow(title: "Email", apps: "Mail, Outlook", style: $store.cleanup.styles.email)
                StyleRow(title: "Everything else", apps: "Notes, docs, browsers", style: $store.cleanup.styles.other, divider: false)
            }

            Section(title: "Cleanup", footer: "These rules run instantly on every dictation.") {
                Row(title: "Remove filler words", detail: "um, uh, erm") {
                    AccentSwitch(isOn: $store.cleanup.removeFillers)
                }
                Row(title: "Apply spoken corrections", detail: "“Tuesday, no, Wednesday” → “Wednesday”") {
                    AccentSwitch(isOn: $store.cleanup.applySelfCorrections)
                }
                Row(title: "Line break commands", detail: "Say “new line” or “new paragraph”") {
                    AccentSwitch(isOn: $store.cleanup.spokenCommands)
                }
                Row(title: "Write times as 10:30", divider: false) {
                    AccentSwitch(isOn: $store.cleanup.normalizeTimes)
                }
            }
        }
    }

    private var editingFooter: String {
        controller.smartFormattingAvailable
            ? "Lists, numbers and long passages are formatted by Apple's on-device model while you speak, so text still lands almost instantly. Nothing leaves your Mac."
            : "Smart formatting needs macOS 26 or later with Apple Intelligence; until then Kudu uses its instant cleanup rules."
    }
}

private struct StyleRow: View {
    let title: String
    let apps: String
    @Binding var style: WritingStyle
    var divider = true

    var body: some View {
        VStack(spacing: 0) {
            HStack(alignment: .center, spacing: 16) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(title).font(.system(size: 13.5, weight: .medium))
                    Text(apps).font(.caption).foregroundStyle(.secondary)
                }
                .frame(width: 170, alignment: .leading)
                Text(style.example)
                    .font(.system(size: 12.5))
                    .italic()
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                Spacer(minLength: 12)
                ChipPicker(items: WritingStyle.allCases, selection: $style) { $0.title }
                    .frame(width: 300)
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 12)
            if divider { Divider().overlay(Theme.hairline).padding(.leading, 18) }
        }
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
            PageHeader(title: "Settings", subtitle: "Keys, sounds, privacy and Command mode.")

            Section(title: "Dictation") {
                Row(title: "Dictation key") {
                    ValueMenu($controller.trigger) { $0.displayName }
                }
                Row(title: "Recording", detail: controller.recordingMode.hint(for: controller.trigger)) {
                    ValueMenu($controller.recordingMode) { $0.title }
                }
                Row(title: "Microphone", detail: "Kudu opens it only while you dictate.") {
                    ValueMenu(
                        items: [MicrophoneChoice(uid: nil, name: "System default")]
                            + controller.microphones.map { MicrophoneChoice(uid: $0.uid, name: $0.name) },
                        selection: $controller.microphoneUID, value: \.uid, title: \.name
                    )
                }
                if controller.trigger == .fn {
                    Row(title: "Stop fn from opening the emoji picker", detail: "Only while Kudu is running.", divider: false) {
                        AccentSwitch(isOn: $controller.preventEmojiPicker)
                    }
                }
            }

            Section(title: "Shortcuts") {
                ShortcutRow(title: "Dictate", keys: [controller.trigger.shortName], prefix: "Hold")
                if controller.recordingMode != .holdOnly {
                    ShortcutRow(
                        title: "Hands-free",
                        keys: [controller.trigger.shortName],
                        prefix: controller.recordingMode == .tapToToggle ? "Tap" : "Double-tap"
                    )
                }
                if controller.trigger == .fn {
                    ShortcutRow(title: "Command mode", keys: ["fn", "⌃"], prefix: "Hold")
                }
                ShortcutRow(title: "Paste last transcript", keys: ["⌃", "⌘", "V"])
                ShortcutRow(title: "Cancel", keys: ["esc"], prefix: "While dictating", divider: false)
            }

            Section(title: "Command mode") {
                Row(
                    title: "Select text, hold fn + ⌃ and say what to do",
                    detail: "“Make this more formal” · “Translate to Hindi” · “Fix the grammar” · “Summarize”. With nothing selected, Kudu writes it: “Write a polite reply saying I'll be late.”",
                    divider: false
                ) {
                    if controller.commandModeAvailable {
                        Label("On device", systemImage: "sparkle").font(.caption).foregroundStyle(Theme.accent)
                    } else {
                        Text("Needs Apple Intelligence").font(.caption).foregroundStyle(.secondary)
                    }
                }
            }

            Section(title: "General") {
                Row(title: "Sounds") {
                    AccentSwitch(isOn: $controller.soundsEnabled)
                }
                Row(title: "Open Kudu at login", divider: false) {
                    AccentSwitch(isOn: Binding(get: { launchAtLogin.isEnabled }, set: { launchAtLogin.set($0) }))
                }
            }

            Section(title: "Privacy", footer: "The microphone is on only while you dictate. Audio is never saved; history stays on this Mac.") {
                Row(title: "Keep history") {
                    ValueMenu($history.retention) { $0.title }
                }
                Row(title: "\(history.records.count) dictations saved on this Mac", divider: false) {
                    Button("Clear history…") { confirm.clearHistory = true }
                        .buttonStyle(QuietButtonStyle(destructive: true))
                        .disabled(history.records.isEmpty)
                }
            }

            Section(title: "Permissions") {
                PermissionRow(title: "Microphone", granted: Permissions.microphoneGranted) {
                    Permissions.openSettings(.microphone)
                }
                PermissionRow(title: "Accessibility", granted: Permissions.accessibilityGranted, divider: false) {
                    Permissions.openSettings(.accessibility)
                }
            }

            Section(title: "About") {
                Row(title: "Version") {
                    Text(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "–")
                        .foregroundStyle(.secondary)
                }
                Row(title: "Speech model") {
                    Text("NVIDIA Parakeet TDT 0.6B v2, on device").foregroundStyle(.secondary)
                }
                Row(title: "Formatting & commands", divider: false) {
                    Text(controller.smartFormattingAvailable ? "Apple on-device model" : "Rules only").foregroundStyle(.secondary)
                }
            }
        }
        .confirmationDialog("Clear all dictation history?", isPresented: $confirm.clearHistory) {
            Button("Clear history", role: .destructive) { history.clear() }
        } message: {
            Text("This deletes every saved dictation from this Mac. It can't be undone.")
        }
    }
}

private struct MicrophoneChoice: Identifiable {
    let uid: String?
    let name: String
    var id: String { uid ?? "default" }
}

private struct ShortcutRow: View {
    let title: String
    let keys: [String]
    var prefix: String?
    var divider = true

    var body: some View {
        Row(title: title, divider: divider) {
            Shortcut(keys: keys, prefix: prefix)
        }
    }
}

private struct PermissionRow: View {
    let title: String
    let granted: Bool
    var divider = true
    let open: () -> Void

    var body: some View {
        Row(title: title, divider: divider) {
            if granted {
                Label("Allowed", systemImage: "checkmark.circle.fill").font(.system(size: 12.5)).foregroundStyle(.green)
            } else {
                Button("Open Settings", action: open).buttonStyle(QuietButtonStyle())
            }
        }
    }
}
