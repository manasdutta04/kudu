import Foundation
import Testing
@testable import KuduCore

@Suite struct WordCountTests {
    @Test(arguments: [
        ("Hello there, world.", 3),
        ("Ship it — today", 3),
        ("  ", 0),
        ("Call me at 10:30 on 3 May", 7),
    ])
    func counts(text: String, expected: Int) {
        #expect(WordCount.count(text) == expected)
    }
}

@Suite struct UsageStatsTests {
    var calendar: Calendar {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        return calendar
    }

    func day(_ offset: Int, hour: Int = 12, from now: Date) -> Date {
        let start = calendar.startOfDay(for: now)
        return calendar.date(byAdding: .day, value: offset, to: start)!.addingTimeInterval(Double(hour) * 3600)
    }

    func record(_ date: Date, words: Int, seconds: Double) -> DictationRecord {
        DictationRecord(
            date: date, raw: "", text: Array(repeating: "word", count: words).joined(separator: " "),
            audioSeconds: seconds, latencyMs: 100
        )
    }

    let now = Date(timeIntervalSince1970: 1_800_000_000) // a fixed afternoon (UTC)

    @Test func emptyHistory() {
        #expect(UsageStats.compute([], now: now, calendar: calendar) == UsageStats(totalWords: 0, wordsPerMinute: 0, dayStreak: 0))
    }

    @Test func totalsAndSpeed() {
        let records = [
            record(day(0, from: now), words: 30, seconds: 15), // 120 wpm
            record(day(0, from: now), words: 20, seconds: 10), // 120 wpm
            record(day(0, from: now), words: 2, seconds: 0.8), // too short to time
        ]
        let stats = UsageStats.compute(records, now: now, calendar: calendar)
        #expect(stats.totalWords == 52)
        #expect(stats.wordsPerMinute == 120)
    }

    @Test func streakCountsConsecutiveDaysEndingToday() {
        let records = [0, -1, -2, -4].map { record(day($0, from: now), words: 5, seconds: 3) }
        #expect(UsageStats.compute(records, now: now, calendar: calendar).dayStreak == 3)
    }

    @Test func streakStillCountsBeforeTodaysFirstDictation() {
        let records = [-1, -2].map { record(day($0, from: now), words: 5, seconds: 3) }
        #expect(UsageStats.compute(records, now: now, calendar: calendar).dayStreak == 2)
    }

    @Test func streakResetsAfterAMissedDay() {
        let records = [-2, -3].map { record(day($0, from: now), words: 5, seconds: 3) }
        #expect(UsageStats.compute(records, now: now, calendar: calendar).dayStreak == 0)
    }
}
