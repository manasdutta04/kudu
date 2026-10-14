import Foundation

/// One day's usage. Totals only, no text, so it can be kept for good.
public struct DayTotal: Codable, Sendable, Equatable {
    public var words = 0
    public var dictations = 0
    public var audioSeconds = 0.0
    /// Words changed by cleanup: fillers, corrections, dictionary spellings.
    public var cleanups = 0
    public var dictionaryFixes = 0
    public var learned = 0
    /// Words per app category (`AppCategory.rawValue`).
    public var categories: [String: Int] = [:]
    /// Words per app bundle ID.
    public var apps: [String: Int] = [:]

    public init() {}

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        words = try c.decodeIfPresent(Int.self, forKey: .words) ?? 0
        dictations = try c.decodeIfPresent(Int.self, forKey: .dictations) ?? 0
        audioSeconds = try c.decodeIfPresent(Double.self, forKey: .audioSeconds) ?? 0
        cleanups = try c.decodeIfPresent(Int.self, forKey: .cleanups) ?? 0
        dictionaryFixes = try c.decodeIfPresent(Int.self, forKey: .dictionaryFixes) ?? 0
        learned = try c.decodeIfPresent(Int.self, forKey: .learned) ?? 0
        categories = try c.decodeIfPresent([String: Int].self, forKey: .categories) ?? [:]
        apps = try c.decodeIfPresent([String: Int].self, forKey: .apps) ?? [:]
    }
}

/// Everything the Insights page shows, computed from daily totals.
public struct Insights: Equatable, Sendable {
    public struct Cell: Equatable, Sendable, Identifiable {
        public var date: Date
        public var words: Int
        /// 0 (none) … 4 (one of your busiest days).
        public var level: Int
        /// Days after today, drawn empty.
        public var isFuture: Bool
        public var id: Date { date }
    }

    public var totalWords: Int
    public var totalDictations: Int
    public var wordsPerMinute: Int
    public var cleanups: Int
    public var dictionaryFixes: Int
    public var learned: Int
    public var currentStreak: Int
    public var longestStreak: Int
    /// Change in words this month vs. the same days last month, as a fraction (0.093 = +9.3%).
    public var monthChange: Double?
    /// Minutes saved against typing at `typingWordsPerMinute`.
    public var minutesSaved: Int
    /// Share of words per category, largest first.
    public var categories: [(category: String, words: Int)]
    /// Words per app bundle ID, largest first.
    public var apps: [(bundleID: String, words: Int)]
    /// Weeks (oldest first) of 7 days, Sunday first.
    public var heatMap: [[Cell]]

    public static let typingWordsPerMinute = 40.0

    public static func == (lhs: Insights, rhs: Insights) -> Bool {
        lhs.totalWords == rhs.totalWords && lhs.heatMap == rhs.heatMap && lhs.longestStreak == rhs.longestStreak
            && lhs.currentStreak == rhs.currentStreak && lhs.monthChange == rhs.monthChange
    }

    /// - Parameter days: totals keyed by the start of each day.
    public static func compute(
        days: [Date: DayTotal], now: Date = .now, weeks: Int = 26, calendar: Calendar = .current
    ) -> Insights {
        let totals = days.values
        let totalWords = totals.reduce(0) { $0 + $1.words }
        let seconds = totals.reduce(0) { $0 + $1.audioSeconds }
        let wordsPerMinute = seconds >= 30 ? Int((Double(totalWords) / (seconds / 60)).rounded()) : 0

        let active = Set(days.filter { $0.value.words > 0 }.keys.map { calendar.startOfDay(for: $0) })
        let today = calendar.startOfDay(for: now)

        var categories: [String: Int] = [:]
        var apps: [String: Int] = [:]
        for total in totals {
            categories.merge(total.categories, uniquingKeysWith: +)
            apps.merge(total.apps, uniquingKeysWith: +)
        }

        let typingMinutes = Double(totalWords) / typingWordsPerMinute
        let minutesSaved = max(0, Int((typingMinutes - seconds / 60).rounded()))

        return Insights(
            totalWords: totalWords,
            totalDictations: totals.reduce(0) { $0 + $1.dictations },
            wordsPerMinute: wordsPerMinute,
            cleanups: totals.reduce(0) { $0 + $1.cleanups },
            dictionaryFixes: totals.reduce(0) { $0 + $1.dictionaryFixes },
            learned: totals.reduce(0) { $0 + $1.learned },
            currentStreak: currentStreak(active, today: today, calendar: calendar),
            longestStreak: longestStreak(active, calendar: calendar),
            monthChange: monthChange(days, today: today, calendar: calendar),
            minutesSaved: minutesSaved,
            categories: categories.filter { $0.value > 0 }.sorted { $0.value > $1.value }.map { ($0.key, $0.value) },
            apps: apps.filter { $0.value > 0 }.sorted { $0.value > $1.value }.map { ($0.key, $0.value) },
            heatMap: heatMap(days, today: today, weeks: weeks, calendar: calendar)
        )
    }

    static func currentStreak(_ active: Set<Date>, today: Date, calendar: Calendar) -> Int {
        var day = today
        if !active.contains(day), let yesterday = calendar.date(byAdding: .day, value: -1, to: day) { day = yesterday }
        var streak = 0
        while active.contains(day) {
            streak += 1
            guard let previous = calendar.date(byAdding: .day, value: -1, to: day) else { break }
            day = previous
        }
        return streak
    }

    static func longestStreak(_ active: Set<Date>, calendar: Calendar) -> Int {
        var longest = 0
        for day in active {
            // Count only from the first day of each run.
            guard let previous = calendar.date(byAdding: .day, value: -1, to: day), !active.contains(previous) else { continue }
            var length = 0
            var cursor = day
            while active.contains(cursor) {
                length += 1
                guard let next = calendar.date(byAdding: .day, value: 1, to: cursor) else { break }
                cursor = next
            }
            longest = max(longest, length)
        }
        return longest
    }

    /// This month so far vs. the same number of days at the start of last month.
    static func monthChange(_ days: [Date: DayTotal], today: Date, calendar: Calendar) -> Double? {
        guard let thisMonth = calendar.dateInterval(of: .month, for: today),
              let lastMonthStart = calendar.date(byAdding: .month, value: -1, to: thisMonth.start)
        else { return nil }
        let elapsed = (calendar.dateComponents([.day], from: thisMonth.start, to: today).day ?? 0) + 1
        guard let lastMonthEnd = calendar.date(byAdding: .day, value: elapsed, to: lastMonthStart) else { return nil }
        func words(_ from: Date, _ to: Date) -> Int {
            days.filter { $0.key >= from && $0.key < to }.reduce(0) { $0 + $1.value.words }
        }
        let current = words(thisMonth.start, calendar.date(byAdding: .day, value: 1, to: today) ?? today)
        let previous = words(lastMonthStart, min(lastMonthEnd, thisMonth.start))
        guard previous > 0 else { return nil }
        return Double(current - previous) / Double(previous)
    }

    static func heatMap(_ days: [Date: DayTotal], today: Date, weeks: Int, calendar: Calendar) -> [[Cell]] {
        // Levels are relative to your own busy days (the 90th percentile), so one huge day
        // doesn't wash everything else out.
        let counts = days.values.map(\.words).filter { $0 > 0 }.sorted()
        let reference = counts.isEmpty ? 1 : max(1, counts[min(counts.count - 1, Int(Double(counts.count) * 0.9))])

        let weekday = calendar.component(.weekday, from: today) // 1 = Sunday
        guard let thisSunday = calendar.date(byAdding: .day, value: -(weekday - 1), to: today),
              let firstSunday = calendar.date(byAdding: .weekOfYear, value: -(weeks - 1), to: thisSunday)
        else { return [] }

        return (0..<weeks).map { week in
            (0..<7).map { offset in
                let date = calendar.date(byAdding: .day, value: week * 7 + offset, to: firstSunday)!
                let words = days[date]?.words ?? 0
                let level = words == 0 ? 0 : min(4, 1 + Int(Double(words) / Double(reference) * 3.999))
                return Cell(date: date, words: words, level: level, isFuture: date > today)
            }
        }
    }
}
