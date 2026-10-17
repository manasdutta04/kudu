import AppKit
import KuduCore
import KuduKit
import SwiftUI
import UniformTypeIdentifiers

/// Which meeting is open, and which tab. ObservableObject rather than @State (see `Draft` in
/// Pages.swift).
@MainActor
final class MeetingsPageState: ObservableObject {
    @Published var selected: UUID?
    @Published var tab: MeetingTab = .notes
    @Published var confirmDelete: UUID?
    @Published var confirmDeleteAll = false
    @Published var showConsent = false
}

enum MeetingTab: String, CaseIterable, Identifiable, Hashable {
    case notes, transcript

    var id: String { rawValue }
    var title: String { self == .notes ? "Notes" : "Transcript" }
}

/// Meetings: turn notes on, take them (live card), and read past meetings' notes and
/// transcripts.
struct MeetingsPage: View {
    @ObservedObject var controller: DictationController
    @ObservedObject var meetings: MeetingRecorder
    @ObservedObject var store: MeetingStore
    @StateObject private var state = MeetingsPageState()

    var body: some View {
        Group {
            if let id = state.selected, let meeting = store.meeting(id) {
                MeetingDetail(meeting: meeting, controller: controller, meetings: meetings, store: store, state: state)
            } else {
                list
            }
        }
        .confirmationDialog("Delete this meeting?", isPresented: Binding(get: { state.confirmDelete != nil }, set: { if !$0 { state.confirmDelete = nil } })) {
            Button("Delete", role: .destructive) {
                if let id = state.confirmDelete { store.delete(id) }
                state.selected = nil
            }
        } message: {
            Text("Its notes and transcript are deleted from this Mac. This can't be undone.")
        }
        .confirmationDialog("Delete all meetings?", isPresented: $state.confirmDeleteAll) {
            Button("Delete all", role: .destructive) { store.deleteAll() }
        } message: {
            Text("Every meeting's notes and transcript are deleted from this Mac. This can't be undone.")
        }
        .sheet(isPresented: $state.showConsent) {
            ConsentSheet {
                state.showConsent = false
                controller.meetingNotesEnabled = true
            } cancel: {
                state.showConsent = false
            }
        }
    }

    private var list: some View {
        PageScroll {
            PageHeader(title: "Meetings", subtitle: "Notes and transcripts of your calls, written on this Mac.")
            if !controller.meetingNotesEnabled {
                EnableMeetingsCard { state.showConsent = true }
            } else {
                MeetingLiveCard(controller: controller, meetings: meetings)
            }
            if store.meetings.isEmpty {
                if controller.meetingNotesEnabled {
                    EmptyState(symbol: "person.2.wave.2", text: "Your meetings will appear here.")
                }
            } else {
                ForEach(days, id: \.day) { group in
                    VStack(alignment: .leading, spacing: 8) {
                        Text(group.title.uppercased())
                            .font(.system(size: 11, weight: .semibold)).foregroundStyle(.secondary).tracking(0.8)
                            .padding(.leading, 4)
                        VStack(spacing: 0) {
                            ForEach(Array(group.meetings.enumerated()), id: \.element.id) { index, meeting in
                                MeetingRow(meeting: meeting, writing: isWriting(meeting.id), divider: index < group.meetings.count - 1) {
                                    state.tab = .notes
                                    state.selected = meeting.id
                                }
                            }
                        }
                        .card(radius: 14)
                    }
                }
                HStack {
                    Spacer()
                    Button("Delete all meetings…") { state.confirmDeleteAll = true }
                        .buttonStyle(QuietButtonStyle(destructive: true))
                        .disabled(meetings.isRecording)
                }
            }
        }
    }

    private func isWriting(_ id: UUID) -> Bool {
        if case let .writing(writing, _, _) = meetings.state { return writing == id }
        return false
    }

    private var days: [(day: Date, title: String, meetings: [Meeting])] {
        let calendar = Calendar.current
        let groups = Dictionary(grouping: store.meetings.filter { $0.id != meetings.current?.id }) { calendar.startOfDay(for: $0.started) }
        return groups.keys.sorted(by: >).map { day in
            let title = calendar.isDateInToday(day) ? "Today"
                : calendar.isDateInYesterday(day) ? "Yesterday"
                : day.formatted(.dateTime.weekday(.wide).day().month(.wide))
            return (day, title, groups[day]!.sorted { $0.started > $1.started })
        }
    }
}

// MARK: - Turning it on

private struct EnableMeetingsCard: View {
    let enable: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Notes for every meeting, \(Text("written on this Mac").italic())")
                .font(Theme.display(28)).foregroundStyle(.white)
            Text("Kudu listens to your calls in Zoom, Teams, Meet or any other app, transcribes them as they happen, and writes up the summary, decisions and action items when the call ends. Nothing leaves your Mac and the audio is never saved.")
                .font(.system(size: 13)).foregroundStyle(.white.opacity(0.8))
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 8) {
                ForEach(["Summary", "Decisions", "Action items", "Full transcript"], id: \.self) { label in
                    Text(label)
                        .font(.system(size: 12.5, weight: .medium)).foregroundStyle(.white)
                        .padding(.horizontal, 12).padding(.vertical, 6)
                        .background(.white.opacity(0.14), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                }
            }
            Button("Turn on meeting notes", action: enable)
                .buttonStyle(MidnightButtonStyle())
                .padding(.top, 4)
        }
        .padding(24)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Theme.midnight, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }
}

private struct MidnightButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(Color(red: 0.08, green: 0.07, blue: 0.18))
            .padding(.horizontal, 16).padding(.vertical, 8)
            .background(.white, in: Capsule())
            .opacity(configuration.isPressed ? 0.8 : 1)
    }
}

/// Shown once, when meeting notes are turned on.
private struct ConsentSheet: View {
    let accept: () -> Void
    let cancel: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Image(systemName: "person.2.wave.2").font(.system(size: 26)).foregroundStyle(Theme.accent)
            Text("Before you take notes").font(Theme.display(24))
            VStack(alignment: .leading, spacing: 10) {
                bullet("hand.raised", "Let people know you're taking notes. In some places everyone in a call must agree to it being recorded or transcribed.")
                bullet("lock", "Everything stays on this Mac: the transcript and notes are saved in Kudu's folder, and the audio is never saved.")
                bullet("speaker.wave.2", "To hear the other people, Kudu records the Mac's sound. macOS asks you to allow this the first time.")
            }
            HStack {
                Spacer()
                Button("Not now", action: cancel).buttonStyle(QuietButtonStyle())
                Button("Turn on", action: accept).buttonStyle(QuietButtonStyle(prominent: true))
            }
            .padding(.top, 6)
        }
        .padding(28)
        .frame(width: 460)
    }

    private func bullet(_ symbol: String, _ text: String) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: symbol).frame(width: 18).foregroundStyle(.secondary)
            Text(text).font(.system(size: 13)).fixedSize(horizontal: false, vertical: true)
        }
    }
}

// MARK: - Live

/// The top card: start notes, the meeting being recorded, or notes being written.
private struct MeetingLiveCard: View {
    @ObservedObject var controller: DictationController
    @ObservedObject var meetings: MeetingRecorder

    var body: some View {
        Group {
            switch meetings.state {
            case let .recording(meeting):
                recording(meeting)
            case let .writing(_, done, total):
                HStack(spacing: 14) {
                    ProgressView().controlSize(.small)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Writing the notes…").font(.system(size: 14, weight: .semibold))
                        Text(total > 1 ? "Step \(min(done + 1, total)) of \(total)" : "This takes a few seconds.")
                            .font(.callout).foregroundStyle(.secondary)
                    }
                    Spacer()
                }
            case .idle:
                idle
            }
        }
        .padding(20)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card(radius: 16)
    }

    private var idle: some View {
        HStack(alignment: .center, spacing: 16) {
            ZStack {
                RoundedRectangle(cornerRadius: 11, style: .continuous).fill(Theme.accent.opacity(0.12)).frame(width: 44, height: 44)
                Image(systemName: "waveform.badge.mic").font(.system(size: 18, weight: .semibold)).foregroundStyle(Theme.accent)
            }
            VStack(alignment: .leading, spacing: 3) {
                Text("Take notes in your next meeting").font(.system(size: 14, weight: .semibold))
                Text(idleDetail).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 12)
            if let keys = controller.shortcuts.meeting?.keycaps { Shortcut(keys: keys) }
            Button("Start") { Task { await controller.startMeeting() } }
                .buttonStyle(QuietButtonStyle(prominent: true))
                .disabled(controller.meetingBlocker != nil)
                .help(controller.meetingBlocker ?? "Start taking notes now")
        }
    }

    private var idleDetail: String {
        if let blocker = controller.meetingBlocker { return blocker + "." }
        switch controller.meetingAutoStart {
        case .off: return "Press the shortcut or Start when the call begins."
        case .ask: return "Kudu asks when a meeting app starts using the microphone."
        case .automatic: return "Kudu starts by itself when a meeting begins, and stops when it ends."
        }
    }

    private func recording(_ meeting: Meeting) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .center, spacing: 14) {
                LiveDot()
                VStack(alignment: .leading, spacing: 2) {
                    Text(meeting.app.map { "Taking notes · \($0)" } ?? "Taking notes").font(.system(size: 14, weight: .semibold))
                    Text(meeting.heardOthers ? "Your microphone and the other people" : "Your microphone only")
                        .font(.callout).foregroundStyle(.secondary)
                }
                Spacer()
                TimelineView(.periodic(from: .now, by: 1)) { context in
                    Text(MeetingFormat.clock(meeting.duration(now: context.date)))
                        .font(.system(size: 15, weight: .medium, design: .rounded)).monospacedDigit()
                        .foregroundStyle(.secondary)
                }
                Button("Stop") { Task { await controller.stopMeeting() } }
                    .buttonStyle(QuietButtonStyle(prominent: true))
            }
            if !meetings.live.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(MeetingFormat.turns(meetings.live).suffix(3)) { turn in
                        HStack(alignment: .firstTextBaseline, spacing: 8) {
                            if meeting.heardOthers {
                                Text(turn.source.speaker).font(.caption.weight(.semibold))
                                    .foregroundStyle(turn.source == .mic ? Theme.accent : .secondary)
                                    .frame(width: 46, alignment: .leading)
                            }
                            Text(turn.text).font(.system(size: 12.5)).foregroundStyle(.secondary).lineLimit(2)
                        }
                    }
                }
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Theme.selection.opacity(0.5), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            }
        }
    }
}

private struct LiveDot: View {
    @StateObject private var pulse = HoverState()

    var body: some View {
        ZStack {
            Circle().fill(Color.red.opacity(0.18)).frame(width: 28, height: 28)
                .scaleEffect(pulse.isHovering ? 1.15 : 0.85)
            Circle().fill(Color.red).frame(width: 10, height: 10)
        }
        .frame(width: 44, height: 44)
        .onAppear {
            withAnimation(.easeInOut(duration: 1).repeatForever(autoreverses: true)) { pulse.isHovering = true }
        }
    }
}

// MARK: - List

private struct MeetingRow: View {
    let meeting: Meeting
    let writing: Bool
    let divider: Bool
    let open: () -> Void
    @StateObject private var hover = HoverState()

    var body: some View {
        VStack(spacing: 0) {
            Button(action: open) {
                HStack(alignment: .center, spacing: 14) {
                    Text(meeting.started.formatted(date: .omitted, time: .shortened))
                        .font(.system(size: 12.5)).monospacedDigit().foregroundStyle(.secondary)
                        .frame(width: 70, alignment: .leading)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(meeting.title).font(.system(size: 13.5, weight: .semibold)).lineLimit(1)
                        Text(subtitle).font(.system(size: 12.5)).foregroundStyle(.secondary).lineLimit(1)
                    }
                    Spacer(minLength: 8)
                    if writing {
                        ProgressView().controlSize(.small)
                    }
                    Text(MeetingFormat.length(meeting.duration())).font(.caption).foregroundStyle(.secondary)
                    Image(systemName: "chevron.right").font(.system(size: 10, weight: .semibold)).foregroundStyle(.tertiary)
                }
                .padding(.horizontal, 18).padding(.vertical, 12)
                .contentShape(Rectangle())
                .background(hover.isHovering ? Color.primary.opacity(0.025) : .clear)
            }
            .buttonStyle(.plain)
            .onHover { hover.isHovering = $0 }
            if divider { Divider().overlay(Theme.hairline).padding(.leading, 102) }
        }
    }

    private var subtitle: String {
        if let summary = meeting.notes?.summary, !summary.isEmpty { return summary }
        return [meeting.app, "\(meeting.words) words"].compactMap { $0 }.joined(separator: " · ")
    }
}

// MARK: - Detail

private struct MeetingDetail: View {
    let meeting: Meeting
    @ObservedObject var controller: DictationController
    @ObservedObject var meetings: MeetingRecorder
    @ObservedObject var store: MeetingStore
    @ObservedObject var state: MeetingsPageState

    var body: some View {
        let segments = store.segments(of: meeting.id)
        PageScroll {
            Button { state.selected = nil } label: {
                Label("Meetings", systemImage: "chevron.left").font(.system(size: 12.5, weight: .medium))
            }
            .buttonStyle(.plain).foregroundStyle(.secondary)

            HStack(alignment: .top, spacing: 16) {
                VStack(alignment: .leading, spacing: 8) {
                    Text(meeting.title).font(Theme.display(30)).textSelection(.enabled)
                    Text(meta).font(.system(size: 13)).foregroundStyle(.secondary)
                }
                Spacer()
                HStack(spacing: 6) {
                    Button { copy(markdown(segments)) } label: { Label("Copy", systemImage: "doc.on.doc") }
                        .buttonStyle(QuietButtonStyle())
                    Menu {
                        Button("Export as Markdown…") { export(markdown(segments)) }
                        Button("Copy transcript only") { copy(MeetingFormat.transcriptText(segments, labelled: meeting.heardOthers)) }
                        Button("Write the notes again") { Task { await meetings.regenerateNotes(meeting.id) } }
                            .disabled(meetings.state != .idle)
                        Button("Show in Finder") { NSWorkspace.shared.activateFileViewerSelecting([store.folder(meeting.id)]) }
                        Divider()
                        Button("Delete…", role: .destructive) { state.confirmDelete = meeting.id }
                    } label: {
                        Image(systemName: "ellipsis").frame(width: 26, height: 24)
                            .background(Theme.selection, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
                    }
                    .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).fixedSize()
                }
            }

            if !meeting.heardOthers {
                HStack(spacing: 8) {
                    Label(
                        meetings.systemAudioBlocked
                            ? "Only your microphone was recorded: macOS didn't let Kudu hear the other people. Allow Kudu under System Audio Recording."
                            : "Only your microphone was recorded. Turn on “Include the other people” in Settings to hear everyone.",
                        systemImage: "info.circle"
                    )
                    .font(.caption).foregroundStyle(.secondary)
                    if meetings.systemAudioBlocked {
                        Button("Open Settings") { Permissions.openSettings(.audioCapture) }.buttonStyle(QuietButtonStyle())
                    }
                }
            }

            ChipPicker(items: MeetingTab.allCases, selection: $state.tab) { $0.title }
                .frame(width: 240)

            switch state.tab {
            case .notes: notes
            case .transcript: transcript(segments)
            }
        }
    }

    private var meta: String {
        var parts = [meeting.started.formatted(date: .complete, time: .shortened), MeetingFormat.length(meeting.duration())]
        if let app = meeting.app { parts.append(app) }
        return parts.joined(separator: " · ")
    }

    @ViewBuilder private var notes: some View {
        if case let .writing(id, done, total) = meetings.state, id == meeting.id {
            HStack(spacing: 10) {
                ProgressView().controlSize(.small)
                Text(total > 1 ? "Writing the notes… step \(min(done + 1, total)) of \(total)" : "Writing the notes…").foregroundStyle(.secondary)
            }
            .padding(20).frame(maxWidth: .infinity, alignment: .leading).card(radius: 14)
        } else if let notes = meeting.notes {
            VStack(alignment: .leading, spacing: 18) {
                if !notes.summary.isEmpty {
                    NotesBlock(title: "Summary") {
                        Text(notes.summary).font(.system(size: 14)).lineSpacing(3).textSelection(.enabled)
                    }
                }
                if !notes.actions.isEmpty { NotesList(title: "Action items", items: notes.actions, symbol: "circle") }
                if !notes.decisions.isEmpty { NotesList(title: "Decisions", items: notes.decisions, symbol: "checkmark.seal") }
                if !notes.keyPoints.isEmpty { NotesList(title: "Key points", items: notes.keyPoints, symbol: "smallcircle.filled.circle") }
                Text(generatorNote(notes.generator)).font(.caption).foregroundStyle(.tertiary)
            }
        } else {
            EmptyState(symbol: "text.alignleft", text: "No notes yet.")
        }
    }

    private func generatorNote(_ generator: NotesGenerator) -> String {
        switch generator {
        case .appleIntelligence: "Written by Apple Intelligence on this Mac. Check important details against the transcript."
        case .kuduWriter: "Written by the Kudu writer model on this computer."
        case .extractive: "Basic notes picked from the transcript (Apple Intelligence isn't available on this Mac)."
        }
    }

    private func transcript(_ segments: [MeetingSegment]) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            let turns = MeetingFormat.turns(segments)
            if turns.isEmpty {
                Text("Nothing was transcribed.").foregroundStyle(.secondary).padding(20)
            }
            ForEach(Array(turns.enumerated()), id: \.element.id) { index, turn in
                HStack(alignment: .firstTextBaseline, spacing: 14) {
                    Text(MeetingFormat.clock(turn.start)).font(.system(size: 12)).monospacedDigit().foregroundStyle(.tertiary)
                        .frame(width: 52, alignment: .leading)
                    VStack(alignment: .leading, spacing: 3) {
                        if meeting.heardOthers {
                            Text(turn.source.speaker).font(.system(size: 12, weight: .semibold))
                                .foregroundStyle(turn.source == .mic ? Theme.accent : .secondary)
                        }
                        Text(turn.text).font(.system(size: 13.5)).lineSpacing(2).textSelection(.enabled)
                    }
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 18).padding(.vertical, 12)
                if index < turns.count - 1 { Divider().overlay(Theme.hairline).padding(.leading, 84) }
            }
        }
        .card(radius: 14)
    }

    private func markdown(_ segments: [MeetingSegment]) -> String {
        MeetingFormat.markdown(meeting, segments: segments, dateText: meeting.started.formatted(date: .complete, time: .shortened))
    }

    private func copy(_ text: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }

    private func export(_ text: String) {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "md") ?? .plainText]
        panel.nameFieldStringValue = meeting.title.replacingOccurrences(of: "/", with: "-") + ".md"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        try? text.write(to: url, atomically: true, encoding: .utf8)
    }
}

private struct NotesBlock<Content: View>: View {
    let title: String
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title.uppercased()).font(.system(size: 11, weight: .semibold)).foregroundStyle(.secondary).tracking(0.8)
            content()
        }
        .padding(20)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card(radius: 14)
    }
}

private struct NotesList: View {
    let title: String
    let items: [String]
    let symbol: String

    var body: some View {
        NotesBlock(title: title) {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                    HStack(alignment: .firstTextBaseline, spacing: 10) {
                        Image(systemName: symbol).font(.system(size: 10, weight: .semibold)).foregroundStyle(Theme.accent)
                        Text(item).font(.system(size: 13.5)).lineSpacing(2).textSelection(.enabled)
                    }
                }
            }
        }
    }
}

// MARK: - Settings

/// Settings → Meeting notes.
struct MeetingSettingsSection: View {
    @ObservedObject var controller: DictationController
    @ObservedObject var meetings: MeetingRecorder

    var body: some View {
        Section(
            title: "Meeting notes",
            footer: controller.meetingNotesEnabled
                ? "Let people know you're taking notes. Transcripts and notes stay on this Mac, in the Meetings page; the audio is never saved."
                : nil
        ) {
            Row(title: "Take notes in meetings", detail: "Transcribes your calls on this Mac and writes up the summary and action items.", divider: controller.meetingNotesEnabled) {
                AccentSwitch(isOn: $controller.meetingNotesEnabled)
            }
            if controller.meetingNotesEnabled {
                EditableShortcutRow(title: "Start or stop notes", detail: nil, slot: .meeting, controller: controller)
                Row(title: "When a meeting starts", detail: "Kudu notices Zoom, Teams, Webex, Slack, FaceTime and browser calls using the microphone.") {
                    ValueMenu($controller.meetingAutoStart) { $0.title }
                }
                Row(title: "Include the other people", detail: "Records the Mac's sound as well as your microphone, so everyone is in the transcript.") {
                    AccentSwitch(isOn: $controller.meetingSystemAudio)
                }
                Row(title: "Notes written by", divider: false) {
                    if meetings.notesUseAppleIntelligence {
                        Label("Apple Intelligence", systemImage: "sparkle").font(.caption).foregroundStyle(Theme.accent)
                    } else {
                        Text("Basic notes (no Apple Intelligence)").font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
        }
    }
}
