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
            VStack(alignment: .leading, spacing: 28) {
                Text("Welcome back, \(Self.firstName)")
                    .font(.system(size: 30, weight: .semibold))

                HStack(alignment: .top, spacing: 18) {
                    HeroCard(controller: controller)
                    StatsCard(stats: history.stats)
                        .frame(width: 250)
                }

                historySection
            }
            .padding(36)
            .frame(maxWidth: 980, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
    }

    @ViewBuilder private var historySection: some View {
        let groups = history.groupedByDay(matching: search.query)
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Text("History").font(.title3.weight(.semibold))
                Spacer()
                HStack(spacing: 6) {
                    Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                    TextField("Search", text: $search.query).textFieldStyle(.plain)
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 6)
                .frame(width: 220)
                .background(.quaternary.opacity(0.6), in: RoundedRectangle(cornerRadius: 8))
            }

            if groups.isEmpty {
                EmptyHistory(searching: !search.query.isEmpty)
            }
            ForEach(groups, id: \.day) { group in
                Text(Self.dayTitle(group.day))
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .tracking(0.6)
                    .padding(.top, 6)
                VStack(spacing: 0) {
                    ForEach(Array(group.records.enumerated()), id: \.element.id) { index, record in
                        if index > 0 { Divider().padding(.leading, 20) }
                        HistoryRow(record: record, controller: controller, history: history)
                    }
                }
                .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 14))
                .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(.quaternary))
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

/// The midnight card with the mark and how to dictate.
private struct HeroCard: View {
    @ObservedObject var controller: DictationController

    var body: some View {
        HStack(alignment: .center, spacing: 24) {
            VStack(alignment: .leading, spacing: 10) {
                Text("Speak anywhere you write")
                    .font(.system(size: 24, weight: .semibold))
                    .foregroundStyle(.white)
                Text(controller.recordingMode.hint(for: controller.trigger) + " Press Esc to cancel.")
                    .font(.callout)
                    .foregroundStyle(.white.opacity(0.75))
                    .fixedSize(horizontal: false, vertical: true)
                HStack(spacing: 8) {
                    Label("On-device", systemImage: "lock.fill")
                    Label(formattingLabel, systemImage: "wand.and.stars")
                }
                .font(.caption.weight(.medium))
                .foregroundStyle(.white.opacity(0.85))
                .labelStyle(PillLabelStyle())
                .padding(.top, 4)
            }
            Spacer(minLength: 0)
            Image(nsImage: KuduMark.image(size: NSSize(width: 92, height: 92), template: false))
                .opacity(0.95)
        }
        .padding(26)
        .frame(maxWidth: .infinity, minHeight: 168, alignment: .leading)
        .background(
            LinearGradient(
                colors: [Color(red: 0.17, green: 0.13, blue: 0.38), Color(red: 0.05, green: 0.04, blue: 0.12)],
                startPoint: .topLeading, endPoint: .bottomTrailing
            ),
            in: RoundedRectangle(cornerRadius: 18)
        )
    }

    private var formattingLabel: String {
        guard controller.smartFormattingAvailable else { return "Rule cleanup" }
        switch controller.store.cleanup.editingStrength {
        case .verbatim: return "Verbatim"
        case .light: return "Smart formatting"
        case .polished: return "Polished"
        }
    }
}

private struct PillLabelStyle: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 5) {
            configuration.icon
            configuration.title
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 4)
        .background(.white.opacity(0.12), in: Capsule())
    }
}

private struct StatsCard: View {
    let stats: UsageStats

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            stat(Self.compact(stats.totalWords), "total words")
            Divider()
            stat(stats.wordsPerMinute > 0 ? "\(stats.wordsPerMinute)" : "–", "words per minute")
            Divider()
            stat("\(stats.dayStreak)", "day streak")
        }
        .padding(22)
        .frame(maxWidth: .infinity, minHeight: 168, alignment: .leading)
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 18))
        .overlay(RoundedRectangle(cornerRadius: 18).strokeBorder(.quaternary))
    }

    private func stat(_ value: String, _ label: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(value).font(.system(size: 26, weight: .semibold, design: .rounded)).monospacedDigit()
            Text(label).font(.callout).foregroundStyle(.secondary)
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
        HStack(alignment: .firstTextBaseline, spacing: 16) {
            Text(record.date.formatted(date: .omitted, time: .shortened))
                .font(.callout)
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .frame(width: 72, alignment: .leading)
            Text(record.text)
                .font(.body)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
            HStack(spacing: 4) {
                if record.formatted {
                    Image(systemName: "wand.and.stars")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                        .help("Smart formatting was applied")
                }
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
                .frame(width: 22)
            }
            .buttonStyle(.borderless)
            .opacity(hover.isHovering ? 1 : 0)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 14)
        .contentShape(Rectangle())
        .background(hover.isHovering ? Color.primary.opacity(0.03) : .clear)
        .onHover { hover.isHovering = $0 }
    }
}

private struct EmptyHistory: View {
    let searching: Bool

    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: searching ? "magnifyingglass" : "waveform")
                .font(.system(size: 28))
                .foregroundStyle(.tertiary)
            Text(searching ? "No dictations match your search." : "Your dictations will appear here.")
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 48)
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 14))
        .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(.quaternary))
    }
}
