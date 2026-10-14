import KuduCore
import KuduKit
import SwiftUI

struct SettingsView: View {
    @ObservedObject var store: UserDataStore

    var body: some View {
        TabView {
            CleanupTab(store: store).tabItem { Label("Cleanup", systemImage: "wand.and.stars") }
            DictionaryTab(store: store).tabItem { Label("Dictionary", systemImage: "character.book.closed") }
            SnippetsTab(store: store).tabItem { Label("Snippets", systemImage: "text.badge.plus") }
        }
        .padding(20)
        .frame(width: 520, height: 420)
    }
}

private struct CleanupTab: View {
    @ObservedObject var store: UserDataStore

    var body: some View {
        Form {
            Toggle("Remove filler words (um, uh)", isOn: $store.cleanup.removeFillers)
            Toggle("Apply spoken corrections (\"Tuesday, no, Wednesday\")", isOn: $store.cleanup.applySelfCorrections)
            Toggle("\"New line\" and \"new paragraph\" commands", isOn: $store.cleanup.spokenCommands)
            Toggle("Write times as 10:30", isOn: $store.cleanup.normalizeTimes)
            Text("Cleanup runs on your Mac in under a millisecond.")
                .font(.caption).foregroundStyle(.secondary)
        }
    }
}

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

private struct DictionaryTab: View {
    @ObservedObject var store: UserDataStore
    @StateObject private var draft = Draft()

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Words Kudu should spell your way. List how it mishears them, separated by commas.")
                .font(.callout).foregroundStyle(.secondary)
            List {
                ForEach(store.dictionary) { entry in
                    HStack {
                        Text(entry.written).bold()
                        Text(entry.heard.joined(separator: ", ")).foregroundStyle(.secondary)
                        Spacer()
                        Button(role: .destructive) {
                            store.dictionary.removeAll { $0.id == entry.id }
                        } label: { Image(systemName: "trash") }
                            .buttonStyle(.borderless)
                    }
                }
            }
            HStack {
                TextField("Spelling (e.g. Kudu)", text: $draft.first)
                TextField("Heard as (e.g. kudoo, coo doo)", text: $draft.second)
                Button("Add") {
                    let heard = draft.second.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }
                    store.dictionary.append(DictionaryEntry(heard: heard, written: draft.first))
                    draft.clear()
                }
                .disabled(draft.first.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
    }
}

private struct SnippetsTab: View {
    @ObservedObject var store: UserDataStore
    @StateObject private var draft = Draft()

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Say the trigger on its own to insert the text. Placeholders: {date}, {time}, {clipboard}.")
                .font(.callout).foregroundStyle(.secondary)
            List {
                ForEach(store.snippets) { snippet in
                    HStack(alignment: .top) {
                        Text(snippet.trigger).bold()
                        Text(snippet.expansion).foregroundStyle(.secondary).lineLimit(2)
                        Spacer()
                        Button(role: .destructive) {
                            store.snippets.removeAll { $0.id == snippet.id }
                        } label: { Image(systemName: "trash") }
                            .buttonStyle(.borderless)
                    }
                }
            }
            HStack {
                TextField("Trigger (e.g. my email)", text: $draft.first)
                TextField("Inserts", text: $draft.second)
                Button("Add") {
                    store.snippets.append(Snippet(trigger: draft.first, expansion: draft.second))
                    draft.clear()
                }
                .disabled(draft.first.isEmpty || draft.second.isEmpty)
            }
        }
    }
}
