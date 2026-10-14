import KuduCore
import KuduKit
import SwiftUI

@MainActor
final class HistorySearch: ObservableObject {
    @Published var query = ""
}

struct HomeView: View {
    @ObservedObject var controller: DictationController
    @ObservedObject var history: HistoryStore
    @StateObject private var search = HistorySearch()

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 26) {
                Text("Welcome back, \(Self.firstName)")
                    .font(Theme.display(32))
                    .padding(.top, 8)

                SetupCard(controller: controller)

                HStack(alignment: .top, spacing: 16) {
                    HeroCard(controller: controller)
                    StatsCard(stats: history.stats)
                        .frame(width: 236)
                }

                historySection
            }
            .padding(.horizontal, 40)
            .padding(.vertical, 34)
            .frame(maxWidth: 980, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        // Scrolling stays (trackpad, wheel); the bar itself is hidden for a calmer page.
        .scrollIndicators(.hidden)
    }

    @ViewBuilder private var historySection: some View {
        let groups = history.groupedByDay(matching: search.query)
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .center) {
                Text("History").font(.system(size: 17, weight: .semibold))
                Spacer()
                SearchField(text: $search.query)
            }

            if groups.isEmpty {
                EmptyHistory(searching: !search.query.isEmpty, hint: controller.recordingMode.hint(for: controller.shortcuts.dictate))
            }
            ForEach(groups, id: \.day) { group in
                Text(Self.dayTitle(group.day))
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.secondary)
                    .tracking(0.8)
                    .padding(.top, 10)
                VStack(spacing: 0) {
                    ForEach(Array(group.records.enumerated()), id: \.element.id) { index, record in
                        if index > 0 { Divider().overlay(Theme.hairline).padding(.leading, 112) }
                        HistoryRow(record: record, controller: controller, history: history)
                    }
                }
                .card(radius: 14)
            }
        }
    }

    static var firstName: String {
        NSFullUserName().split(separator: " ").first.map(String.init) ?? NSUserName()
    }

    static func dayTitle(_ day: Date) -> String {
        let calendar = Calendar.current
        if calendar.isDateInToday(day) { return "TODAY" }
        if calendar.isDateInYesterday(day) { return "YESTERDAY" }
        return day.formatted(.dateTime.month(.wide).day().year()).uppercased()
    }
}

private struct SearchField: View {
    @Binding var text: String

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "magnifyingglass").font(.system(size: 12)).foregroundStyle(.secondary)
            TextField("Search", text: $text)
                .textFieldStyle(.plain)
                .font(.system(size: 13))
            if !text.isEmpty {
                Button { text = "" } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(.tertiary) }
                    .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .frame(width: 230)
        .background(Theme.card, in: Capsule())
        .overlay(Capsule().strokeBorder(Theme.hairline))
    }
}

/// The midnight card: what Kudu does and the three gestures.
private struct HeroCard: View {
    @ObservedObject var controller: DictationController

    var body: some View {
        HStack(alignment: .center, spacing: 20) {
            VStack(alignment: .leading, spacing: 12) {
                Text("Speak anywhere you write")
                    .font(Theme.display(25))
                    .foregroundStyle(.white)
                VStack(alignment: .leading, spacing: 7) {
                    GestureHint(keys: "Hold \(controller.shortcuts.dictate.displayName)", text: "dictate")
                    if controller.recordingMode != .holdOnly {
                        GestureHint(
                            keys: controller.recordingMode == .tapToToggle
                                ? "Tap \(controller.shortcuts.dictate.displayName)" : "Double-tap \(controller.shortcuts.dictate.displayName)",
                            text: "hands-free"
                        )
                    }
                    if let command = controller.shortcuts.command {
                        GestureHint(keys: command.displayName, text: "command: rewrite, translate, draft")
                    }
                }
            }
            Spacer(minLength: 0)
            Image(nsImage: KuduMark.image(size: NSSize(width: 84, height: 84), template: false))
                .opacity(0.92)
                .padding(.trailing, 6)
        }
        .padding(24)
        .frame(maxWidth: .infinity, minHeight: 176, alignment: .leading)
        .background(Theme.midnight, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }
}

private struct GestureHint: View {
    let keys: String
    let text: String

    var body: some View {
        HStack(spacing: 8) {
            Text(keys)
                .font(.system(size: 11.5, weight: .semibold))
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .background(.white.opacity(0.14), in: RoundedRectangle(cornerRadius: 6, style: .continuous))
            Text(text).font(.system(size: 12.5))
        }
        .foregroundStyle(.white.opacity(0.88))
    }
}

private struct StatsCard: View {
    let stats: UsageStats

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            stat(Self.compact(stats.totalWords), "total words")
            Divider().overlay(Theme.hairline).padding(.vertical, 12)
            stat(stats.wordsPerMinute > 0 ? "\(stats.wordsPerMinute)" : "–", "wpm")
            Divider().overlay(Theme.hairline).padding(.vertical, 12)
            stat("\(stats.dayStreak)", "day streak")
        }
        .padding(22)
        .frame(maxWidth: .infinity, minHeight: 176, alignment: .leading)
        .card(radius: 18)
    }

    private func stat(_ value: String, _ label: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(value).font(Theme.display(30)).monospacedDigit()
            Text(label).font(.system(size: 13)).foregroundStyle(.secondary)
        }
    }

    static func compact(_ number: Int) -> String {
        switch number {
        case 1_000_000...: String(format: "%.1fM", Double(number) / 1_000_000)
        case 10_000...: String(format: "%.0fK", Double(number) / 1_000)
        case 1_000...: String(format: "%.1fK", Double(number) / 1_000)
        default: "\(number)"
        }
    }
}

@MainActor
private final class HoverState: ObservableObject {
    @Published var isHovering = false
}

private struct HistoryRow: View {
    let record: DictationRecord
    let controller: DictationController
    let history: HistoryStore
    @StateObject private var hover = HoverState()

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 0) {
            Text(record.date.formatted(date: .omitted, time: .shortened))
                .font(.system(size: 12.5))
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .frame(width: 92, alignment: .leading)
            VStack(alignment: .leading, spacing: 4) {
                if record.isCommand {
                    Label(record.raw, systemImage: "sparkle")
                        .font(.system(size: 11.5))
                        .foregroundStyle(Theme.accent)
                        .lineLimit(1)
                }
                Text(record.text)
                    .font(.system(size: 13.5))
                    .textSelection(.enabled)
                    .lineSpacing(2)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            HStack(spacing: 2) {
                Button { controller.copyToClipboard(record) } label: { Image(systemName: "doc.on.doc") }
                    .help("Copy")
                Menu {
                    Button("Copy raw transcript") { controller.copyToClipboard(record, raw: true) }
                    Button("Type again at cursor") { controller.reinsert(record) }
                    Divider()
                    Button("Delete", role: .destructive) { history.delete(record.id) }
                } label: {
                    Image(systemName: "ellipsis")
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .frame(width: 24)
            }
            .buttonStyle(.borderless)
            .foregroundStyle(.secondary)
            .opacity(hover.isHovering ? 1 : 0)
            .padding(.leading, 12)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 14)
        .contentShape(Rectangle())
        .background(hover.isHovering ? Color.primary.opacity(0.025) : .clear)
        .onHover { hover.isHovering = $0 }
    }
}

private struct EmptyHistory: View {
    let searching: Bool
    let hint: String

    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: searching ? "magnifyingglass" : "waveform")
                .font(.system(size: 26))
                .foregroundStyle(.tertiary)
            Text(searching ? "No dictations match your search." : "Your dictations will appear here.")
                .font(.system(size: 13.5, weight: .medium))
            if !searching {
                Text(hint).font(.callout).foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 48)
        .card(radius: 14)
    }
}
