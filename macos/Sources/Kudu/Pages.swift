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
        // Scrolling stays (trackpad, wheel); the bar itself is hidden for a calmer page.
        .scrollIndicators(.hidden)
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

/// Add-form visibility and search text. ObservableObject rather than @State (see `Draft`).
@MainActor
private final class ListState: ObservableObject {
    @Published var adding = false
    @Published var query = ""
}

/// Header with a title and the prominent "Add new" button.
private struct ListHeader: View {
    let title: String
    let subtitle: String
    let add: () -> Void

    var body: some View {
        HStack(alignment: .top) {
            PageHeader(title: title, subtitle: subtitle)
            Spacer()
            Button(action: add) { Label("Add new", systemImage: "plus") }
                .buttonStyle(QuietButtonStyle(prominent: true))
        }
    }
}

/// The dismissible intro card on Dictionary and Snippets.
private struct IntroCard<Chips: View>: View {
    let title: Text
    let detail: String
    @Binding var dismissed: Bool
    @ViewBuilder let chips: () -> Chips

    var body: some View {
        if !dismissed {
            VStack(alignment: .leading, spacing: 14) {
                HStack(alignment: .top) {
                    title.font(Theme.display(28)).foregroundStyle(.white)
                    Spacer()
                    Button { dismissed = true } label: {
                        Image(systemName: "xmark").font(.system(size: 10, weight: .bold))
                            .frame(width: 22, height: 22)
                            .background(.white.opacity(0.15), in: Circle())
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(.white)
                }
                Text(detail).font(.system(size: 13)).foregroundStyle(.white.opacity(0.8))
                    .fixedSize(horizontal: false, vertical: true)
                chips()
            }
            .padding(24)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Theme.midnight, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        }
    }
}

private struct Chip: View {
    let text: String
    var italic = false

    var body: some View {
        Text(text)
            .font(.system(size: 12.5, weight: .medium))
            .italic(italic)
            .foregroundStyle(.white)
            .lineLimit(1)
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .background(.white.opacity(0.14), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
    }
}

private struct ListSearch: View {
    @Binding var text: String

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "magnifyingglass").font(.system(size: 12)).foregroundStyle(.secondary)
            TextField("Search", text: $text).textFieldStyle(.plain).font(.system(size: 13))
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .frame(width: 220)
        .background(Theme.card, in: Capsule())
        .overlay(Capsule().strokeBorder(Theme.hairline))
    }
}

struct DictionaryPage: View {
    @ObservedObject var store: UserDataStore
    @StateObject private var draft = Draft()
    @StateObject private var list = ListState()
    @AppStorage("dismissedDictionaryIntro") private var dismissedIntro = false

    var body: some View {
        PageScroll {
            ListHeader(
                title: "Dictionary",
                subtitle: "Names, products and jargon, spelled your way. Kudu also learns them when you fix a word it got wrong."
            ) { list.adding.toggle() }

            IntroCard(
                title: Text("Kudu spells the way ") + Text("you").italic() + Text(" do."),
                detail: "Correct a spelling once after dictating, or add it here, and your names and terms come out right from then on.",
                dismissed: $dismissedIntro
            ) {
                HStack(spacing: 8) {
                    Button { list.adding = true } label: { Chip(text: "Add new word") }.buttonStyle(.plain)
                    ForEach(store.dictionary.prefix(4)) { Chip(text: $0.written) }
                }
            }

            if list.adding {
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
            }

            let entries = store.dictionary.reversed().filter {
                list.query.isEmpty || $0.written.localizedCaseInsensitiveContains(list.query)
                    || $0.heard.contains { $0.localizedCaseInsensitiveContains(list.query) }
            }
            HStack {
                Text(store.dictionary.count == 1 ? "1 word" : "\(store.dictionary.count) words").font(.system(size: 13, weight: .semibold))
                Spacer()
                ListSearch(text: $list.query)
            }
            if store.dictionary.isEmpty {
                EmptyState(symbol: "character.book.closed", text: "No words yet. Add names and terms, or fix a word after dictating.")
            } else {
                VStack(spacing: 0) {
                    ForEach(Array(entries.enumerated()), id: \.element.id) { index, entry in
                        if index > 0 { Divider().overlay(Theme.hairline).padding(.leading, 18) }
                        EntryRow {
                            HStack(spacing: 6) {
                                Text(entry.written).font(.system(size: 13.5, weight: .medium))
                                if entry.isLearned {
                                    Image(systemName: "sparkle").font(.system(size: 10)).foregroundStyle(Theme.accent)
                                        .help("Learned from a correction you made")
                                }
                            }
                            if !entry.heard.isEmpty {
                                Text("heard as " + entry.heard.joined(separator: ", "))
                                    .font(.system(size: 12)).foregroundStyle(.secondary).lineLimit(1)
                            }
                        } onDelete: {
                            store.dictionary.removeAll { $0.id == entry.id }
                        }
                    }
                }
                .card(radius: 14)
            }
        }
    }
}

/// A list row whose delete button appears on hover.
private struct EntryRow<Content: View>: View {
    @ViewBuilder let content: () -> Content
    let onDelete: () -> Void
    @StateObject private var hover = RowHover()

    var body: some View {
        HStack(spacing: 14) {
            content()
            Spacer()
            Button(role: .destructive, action: onDelete) { Image(systemName: "trash").font(.system(size: 12)) }
                .buttonStyle(.borderless)
                .foregroundStyle(.secondary)
                .opacity(hover.isHovering ? 1 : 0)
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 13)
        .contentShape(Rectangle())
        .background(hover.isHovering ? Color.primary.opacity(0.025) : .clear)
        .onHover { hover.isHovering = $0 }
    }
}

@MainActor
private final class RowHover: ObservableObject {
    @Published var isHovering = false
}

// MARK: - Snippets

struct SnippetsPage: View {
    @ObservedObject var store: UserDataStore
    @StateObject private var draft = Draft()
    @StateObject private var list = ListState()
    @AppStorage("dismissedSnippetsIntro") private var dismissedIntro = false

    var body: some View {
        PageScroll {
            ListHeader(
                title: "Snippets",
                subtitle: "Say a trigger on its own to insert saved text. Placeholders: {date}, {time}, {clipboard}."
            ) { list.adding.toggle() }

            IntroCard(
                title: Text("The stuff ") + Text("you").italic() + Text(" shouldn't have to retype."),
                detail: "Save text you type often: your email, a link, a sign-off. Then say its trigger to drop it in.",
                dismissed: $dismissedIntro
            ) {
                VStack(alignment: .leading, spacing: 8) {
                    ExampleSnippet(trigger: "my email", expansion: "you@example.com")
                    ExampleSnippet(trigger: "my calendar", expansion: "https://cal.com/you")
                    Button { list.adding = true } label: { Chip(text: "Add new snippet") }.buttonStyle(.plain)
                        .padding(.top, 4)
                }
            }

            if list.adding {
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
            }

            let snippets = store.snippets.reversed().filter {
                list.query.isEmpty || $0.trigger.localizedCaseInsensitiveContains(list.query)
                    || $0.expansion.localizedCaseInsensitiveContains(list.query)
            }
            HStack {
                Text(store.snippets.count == 1 ? "1 snippet" : "\(store.snippets.count) snippets").font(.system(size: 13, weight: .semibold))
                Spacer()
                ListSearch(text: $list.query)
            }
            if store.snippets.isEmpty {
                EmptyState(symbol: "text.badge.plus", text: "No snippets yet. Try “my email” → your address.")
            } else {
                VStack(spacing: 0) {
                    ForEach(Array(snippets.enumerated()), id: \.element.id) { index, snippet in
                        if index > 0 { Divider().overlay(Theme.hairline).padding(.leading, 18) }
                        EntryRow {
                            Text("“\(snippet.trigger)”").font(.system(size: 13.5, weight: .medium))
                            Image(systemName: "arrow.right").font(.system(size: 10)).foregroundStyle(.tertiary)
                            Text(snippet.expansion).font(.system(size: 13)).foregroundStyle(.secondary).lineLimit(1)
                        } onDelete: {
                            store.snippets.removeAll { $0.id == snippet.id }
                        }
                    }
                }
                .card(radius: 14)
            }
        }
    }
}

private struct ExampleSnippet: View {
    let trigger: String
    let expansion: String

    var body: some View {
        HStack(spacing: 8) {
            Chip(text: "“\(trigger)”", italic: true)
            Image(systemName: "arrow.right").font(.system(size: 10, weight: .semibold)).foregroundStyle(.white.opacity(0.7))
            Chip(text: expansion)
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
                Row(title: "Recording", detail: controller.recordingMode.hint(for: controller.shortcuts.dictate)) {
                    ValueMenu($controller.recordingMode) { $0.title }
                }
                Row(title: "Microphone", detail: "Kudu opens it only while you dictate.", divider: controller.shortcuts.usesFn) {
                    ValueMenu(
                        items: [MicrophoneChoice(uid: nil, name: "System default")]
                            + controller.microphones.map { MicrophoneChoice(uid: $0.uid, name: $0.name) },
                        selection: $controller.microphoneUID, value: \.uid, title: \.name
                    )
                }
                if controller.shortcuts.usesFn {
                    Row(title: "Stop fn from opening the emoji picker", detail: "Only while Kudu is running.", divider: false) {
                        AccentSwitch(isOn: $controller.preventEmojiPicker)
                    }
                }
            }

            Section(
                title: "Shortcuts",
                footer: "Click Change, then press the keys you want. Modifier keys alone (fn, Right ⌥, ⌃⌥) are held to talk; or use a key with modifiers, like ⌥Space."
            ) {
                EditableShortcutRow(title: "Dictate", detail: "Hold to talk", slot: .dictate, controller: controller)
                if controller.recordingMode != .holdOnly {
                    ShortcutRow(
                        title: "Hands-free",
                        keys: controller.shortcuts.dictate.keycaps,
                        prefix: controller.recordingMode == .tapToToggle ? "Tap" : "Double-tap"
                    )
                }
                EditableShortcutRow(
                    title: "Command mode", detail: "Hold and say what to do with the selected text", slot: .command,
                    controller: controller
                )
                EditableShortcutRow(title: "Paste last transcript", detail: nil, slot: .pasteLast, controller: controller)
                ShortcutRow(title: "Cancel", keys: ["esc"], prefix: "While dictating", divider: controller.shortcuts != .defaults || controller.shortcutError != nil)
                if let error = controller.shortcutError {
                    Row(title: error, divider: controller.shortcuts != .defaults) {
                        Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
                    }
                }
                if controller.shortcuts != .defaults {
                    Row(title: "Restore the default shortcuts", detail: "fn to dictate, fn ⌃ for Command mode, ⌃⌘V to paste the last transcript.", divider: false) {
                        Button("Restore") { controller.resetShortcuts() }.buttonStyle(QuietButtonStyle())
                    }
                }
            }

            Section(title: "Memory", footer: "After a dictation Kudu watches the field for up to 60 seconds. If you fix a name or term it misspelled, it remembers it. Only the word is saved, on this Mac.") {
                Row(title: "Learn from my corrections", detail: "Learned words appear in the Dictionary with ✦.", divider: false) {
                    AccentSwitch(isOn: $controller.learnFromCorrections)
                }
            }

            Section(title: "Command mode") {
                Row(
                    title: "Select text, hold \(controller.shortcuts.command?.displayName ?? "the Command shortcut") and say what to do",
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

/// A shortcut with a Change button. While recording, it shows the keys held so far.
private struct EditableShortcutRow: View {
    let title: String
    let detail: String?
    let slot: ShortcutSlot
    @ObservedObject var controller: DictationController

    private var shortcut: KeyShortcut? {
        switch slot {
        case .dictate: controller.shortcuts.dictate
        case .command: controller.shortcuts.command
        case .pasteLast: controller.shortcuts.pasteLast
        }
    }

    var body: some View {
        Row(title: title, detail: detail) {
            HStack(spacing: 10) {
                if controller.recordingShortcut == slot {
                    HStack(spacing: 4) {
                        if controller.recordingPreview.isEmpty {
                            Text("Press keys…").font(.system(size: 12.5)).foregroundStyle(Theme.accent)
                        } else {
                            ForEach(Array(controller.recordingPreview.enumerated()), id: \.offset) { _, key in KeyCap(label: key) }
                        }
                    }
                    .padding(.horizontal, 10)
                    .frame(minHeight: 28)
                    .overlay(Capsule().strokeBorder(Theme.accent.opacity(0.7), lineWidth: 1))
                    Button("Cancel") { controller.cancelShortcutRecording() }.buttonStyle(QuietButtonStyle())
                } else {
                    if let shortcut {
                        Shortcut(keys: shortcut.keycaps)
                    } else {
                        Text("Off").font(.system(size: 12.5)).foregroundStyle(.secondary)
                    }
                    Button("Change") { controller.recordShortcut(slot) }.buttonStyle(QuietButtonStyle())
                }
            }
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
