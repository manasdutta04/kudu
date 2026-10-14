import AppKit
import KuduCore
import KuduKit
import SwiftUI

/// Your dictation at a glance: speed, cleanups, words, streak heat map and where you dictate.
/// Everything is computed from daily totals stored on this Mac.
struct InsightsPage: View {
    @ObservedObject var ledger: UsageLedger

    var body: some View {
        let insights = ledger.insights
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                PageHeader(title: "Insights", subtitle: "Your dictation at a glance. Counted on this Mac, never uploaded.")
                    .padding(.bottom, 6)

                HStack(alignment: .top, spacing: 14) {
                    SpeedCard(wordsPerMinute: insights.wordsPerMinute)
                    CleanupCard(insights: insights)
                    WordsCard(insights: insights)
                }
                .fixedSize(horizontal: false, vertical: true)

                StreakCard(insights: insights)
                UsageCard(insights: insights)
            }
            .padding(.horizontal, 40)
            .padding(.vertical, 42)
            .frame(maxWidth: 900, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        // Scrolling stays (trackpad, wheel); the bar itself is hidden for a calmer page.
        .scrollIndicators(.hidden)
    }
}

// MARK: - Cards

private struct CardTitle: View {
    let text: String

    var body: some View {
        Text(text.uppercased())
            .font(.system(size: 10.5, weight: .semibold))
            .tracking(0.9)
            .foregroundStyle(.secondary)
    }
}

private struct SpeedCard: View {
    let wordsPerMinute: Int

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            CardTitle(text: "Words per minute")
            ZStack(alignment: .bottom) {
                Gauge(value: min(1, Double(wordsPerMinute) / 200))
                    .frame(width: 150, height: 82)
                VStack(spacing: 0) {
                    Text(wordsPerMinute > 0 ? "\(wordsPerMinute)" : "–").font(Theme.display(32)).monospacedDigit()
                    Text("wpm").font(.caption).foregroundStyle(.secondary)
                }
            }
            .frame(maxWidth: .infinity)
            Text(speedLine).font(.callout).foregroundStyle(.secondary)
                .frame(maxWidth: .infinity)
        }
        .padding(20)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .card(radius: 16)
    }

    private var speedLine: String {
        guard wordsPerMinute > 0 else { return "Dictate a bit to see your speed" }
        let ratio = Double(wordsPerMinute) / Insights.typingWordsPerMinute
        return String(format: "%.1f× faster than typing", ratio)
    }
}

/// A half-ring gauge.
private struct Gauge: View {
    let value: Double

    var body: some View {
        ZStack {
            Arc(progress: 1).stroke(Theme.selection, style: StrokeStyle(lineWidth: 12, lineCap: .round))
            Arc(progress: max(0.02, value))
                .stroke(
                    AngularGradient(colors: [Theme.accent.opacity(0.55), Theme.accent], center: .bottom, startAngle: .degrees(180), endAngle: .degrees(360)),
                    style: StrokeStyle(lineWidth: 12, lineCap: .round)
                )
        }
    }

    private struct Arc: Shape {
        var progress: Double

        func path(in rect: CGRect) -> Path {
            var path = Path()
            let radius = min(rect.width / 2, rect.height) - 6
            path.addArc(
                center: CGPoint(x: rect.midX, y: rect.maxY), radius: radius,
                startAngle: .degrees(180), endAngle: .degrees(180 + 180 * progress), clockwise: false
            )
            return path
        }
    }
}

private struct CleanupCard: View {
    let insights: Insights

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            CardTitle(text: "Cleaned up for you")
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(insights.cleanups.formatted()).font(Theme.display(32)).monospacedDigit()
                Text("words").font(.callout).foregroundStyle(.secondary)
            }
            Divider().overlay(Theme.hairline)
            Line(icon: "character.book.closed", text: "\(insights.dictionaryFixes.formatted()) dictionary fixes")
            Line(icon: "sparkle", text: "\(insights.learned.formatted()) words learned from you")
        }
        .padding(20)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .card(radius: 16)
    }
}

private struct WordsCard: View {
    let insights: Insights

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            CardTitle(text: "Words dictated")
            HStack(alignment: .firstTextBaseline) {
                Text(insights.totalWords.formatted()).font(Theme.display(32)).monospacedDigit()
                Spacer(minLength: 6)
                if let change = insights.monthChange {
                    Text(String(format: "%@%.0f%%", change >= 0 ? "↗ +" : "↘ ", change * 100))
                        .font(.system(size: 11, weight: .semibold))
                        .lineLimit(1)
                        .fixedSize()
                        .foregroundStyle(change >= 0 ? Color.green : Color.orange)
                        .padding(.horizontal, 7)
                        .padding(.vertical, 3)
                        .background((change >= 0 ? Color.green : Color.orange).opacity(0.12), in: Capsule())
                        .help("Compared with the same days last month")
                }
            }
            Divider().overlay(Theme.hairline)
            Line(icon: "waveform", text: "\(insights.totalDictations.formatted()) dictations")
            Line(icon: "clock", text: "\(Self.duration(insights.minutesSaved)) saved vs typing")
        }
        .padding(20)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .card(radius: 16)
    }

    static func duration(_ minutes: Int) -> String {
        minutes >= 60 ? "\(minutes / 60)h \(minutes % 60)m" : "\(minutes)m"
    }
}

private struct Line: View {
    let icon: String
    let text: String

    var body: some View {
        Label {
            Text(text).font(.system(size: 12.5))
        } icon: {
            Image(systemName: icon).foregroundStyle(.secondary).frame(width: 16)
        }
    }
}

// MARK: - Streak heat map

private struct StreakCard: View {
    let insights: Insights
    /// Largest cell size; smaller when the window is narrow.
    private let cell: CGFloat = 22
    private let gap: CGFloat = 4
    private let labelWidth: CGFloat = 28

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(alignment: .firstTextBaseline) {
                Text(insights.currentStreak == 1 ? "1 day streak" : "\(insights.currentStreak) day streak")
                    .font(Theme.display(26))
                Spacer()
                CardTitle(text: "Longest streak · \(insights.longestStreak) days")
            }

            // The grid always fills the card's width: cells are sized from it (up to `cell`).
            GeometryReader { proxy in
                let weeks = CGFloat(max(1, insights.heatMap.count))
                let size = min(cell, max(8, (proxy.size.width - labelWidth - 8 - gap * (weeks - 1)) / weeks))
                HStack(alignment: .top, spacing: 8) {
                    VStack(alignment: .trailing, spacing: gap) {
                        Color.clear.frame(height: 14) // month row
                        ForEach(0..<7, id: \.self) { row in
                            Text([1: "Mon", 3: "Wed", 5: "Fri"][row] ?? "")
                                .font(.system(size: 9.5))
                                .foregroundStyle(.secondary)
                                .frame(height: size)
                        }
                    }
                    .frame(width: labelWidth, alignment: .trailing)
                    VStack(alignment: .leading, spacing: gap) {
                        HStack(spacing: gap) {
                            ForEach(Array(insights.heatMap.enumerated()), id: \.offset) { index, week in
                                Text(monthLabel(week, index: index))
                                    .font(.system(size: 9.5))
                                    .foregroundStyle(.secondary)
                                    .fixedSize()
                                    .frame(width: size, height: 14, alignment: .leading)
                            }
                        }
                        HStack(spacing: gap) {
                            ForEach(Array(insights.heatMap.enumerated()), id: \.offset) { _, week in
                                VStack(spacing: gap) {
                                    ForEach(week) { day in
                                        RoundedRectangle(cornerRadius: size * 0.22, style: .continuous)
                                            .fill(color(day))
                                            .frame(width: size, height: size)
                                            .help(tooltip(day))
                                    }
                                }
                            }
                        }
                    }
                }
            }
            .frame(height: 14 + gap + 7 * cell + 6 * gap)

            HStack(spacing: 5) {
                Text("Less").font(.caption).foregroundStyle(.secondary)
                ForEach(0..<5, id: \.self) { level in
                    RoundedRectangle(cornerRadius: 3, style: .continuous).fill(Self.color(level)).frame(width: 11, height: 11)
                }
                Text("More").font(.caption).foregroundStyle(.secondary)
                Spacer()
                Text("Each square is a day; darker means more words.").font(.caption).foregroundStyle(.secondary)
            }
        }
        .padding(22)
        .card(radius: 16)
    }

    private func monthLabel(_ week: [Insights.Cell], index: Int) -> String {
        // Label the first week that contains the 1st–7th of a month.
        guard let first = week.first?.date else { return "" }
        let day = Calendar.current.component(.day, from: first)
        return day <= 7 || index == 0 ? first.formatted(.dateTime.month(.abbreviated)) : ""
    }

    private func color(_ day: Insights.Cell) -> Color {
        day.isFuture ? Theme.selection.opacity(0.35) : Self.color(day.level)
    }

    static func color(_ level: Int) -> Color {
        switch level {
        case 0: Theme.selection
        case 1: Theme.accent.opacity(0.28)
        case 2: Theme.accent.opacity(0.5)
        case 3: Theme.accent.opacity(0.75)
        default: Theme.accent
        }
    }

    private func tooltip(_ day: Insights.Cell) -> String {
        let date = day.date.formatted(.dateTime.weekday(.wide).month(.abbreviated).day())
        return day.words == 0 ? "No dictation · \(date)" : "\(day.words.formatted()) words · \(date)"
    }
}

// MARK: - Where you dictate

private struct UsageCard: View {
    let insights: Insights

    var body: some View {
        HStack(alignment: .top, spacing: 28) {
            VStack(alignment: .leading, spacing: 14) {
                CardTitle(text: "Where you dictate")
                if insights.categories.isEmpty {
                    Text("Nothing yet.").foregroundStyle(.secondary)
                }
                ForEach(insights.categories, id: \.category) { entry in
                    CategoryBar(
                        category: AppCategory(rawValue: entry.category) ?? .general,
                        share: Double(entry.words) / Double(max(1, insights.totalWords)),
                        words: entry.words
                    )
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            VStack(alignment: .leading, spacing: 12) {
                CardTitle(text: "Top apps")
                if insights.apps.isEmpty {
                    Text("Nothing yet.").foregroundStyle(.secondary)
                }
                ForEach(insights.apps.prefix(5), id: \.bundleID) { app in
                    AppRow(bundleID: app.bundleID, words: app.words)
                }
            }
            .frame(width: 250, alignment: .leading)
        }
        .padding(22)
        .card(radius: 16)
    }
}

private struct CategoryBar: View {
    let category: AppCategory
    let share: Double
    let words: Int

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: icon).foregroundStyle(.secondary).frame(width: 18)
            Text(title).font(.system(size: 12.5)).frame(width: 130, alignment: .leading)
            GeometryReader { proxy in
                ZStack(alignment: .leading) {
                    Capsule().fill(Theme.selection)
                    Capsule().fill(Theme.accent).frame(width: max(6, proxy.size.width * share))
                }
            }
            .frame(height: 8)
            Text(String(format: "%.0f%%", share * 100))
                .font(.system(size: 12, weight: .medium)).monospacedDigit()
                .frame(width: 38, alignment: .trailing)
        }
        .help("\(words.formatted()) words")
    }

    private var title: String {
        switch category {
        case .personalMessage: "Personal messages"
        case .workMessage: "Work messages"
        case .email: "Email"
        case .code: "Code"
        case .general: "Everything else"
        }
    }

    private var icon: String {
        switch category {
        case .personalMessage: "message"
        case .workMessage: "bubble.left.and.bubble.right"
        case .email: "envelope"
        case .code: "chevron.left.forwardslash.chevron.right"
        case .general: "doc.text"
        }
    }
}

private struct AppRow: View {
    let bundleID: String
    let words: Int

    var body: some View {
        HStack(spacing: 10) {
            Image(nsImage: icon).resizable().frame(width: 22, height: 22)
            Text(name).font(.system(size: 12.5)).lineLimit(1)
            Spacer()
            Text(words.formatted()).font(.system(size: 12)).monospacedDigit().foregroundStyle(.secondary)
        }
    }

    private var url: URL? { NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleID) }

    private var name: String {
        if let url { return FileManager.default.displayName(atPath: url.path).replacingOccurrences(of: ".app", with: "") }
        return Self.knownNames[bundleID] ?? bundleID.split(separator: ".").last.map { $0.capitalized } ?? bundleID
    }

    /// Names for apps that aren't installed on this Mac any more.
    private static let knownNames: [String: String] = [
        "com.tinyspeck.slackmacgap": "Slack", "com.microsoft.teams2": "Microsoft Teams", "com.hnc.Discord": "Discord",
        "net.whatsapp.WhatsApp": "WhatsApp", "ru.keepcoder.Telegram": "Telegram", "com.microsoft.VSCode": "Visual Studio Code",
        "com.todesktop.230313mzl4w4u92": "Cursor", "com.google.Chrome": "Google Chrome", "com.microsoft.Outlook": "Outlook",
        "company.thebrowser.Browser": "Arc", "com.brave.Browser": "Brave",
    ]

    private var icon: NSImage {
        url.map { NSWorkspace.shared.icon(forFile: $0.path) } ?? NSImage(systemSymbolName: "app", accessibilityDescription: nil)!
    }
}
