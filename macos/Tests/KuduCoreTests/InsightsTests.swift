import Foundation
import Testing
@testable import KuduCore

@Suite struct InsightsTests {
    var calendar: Calendar {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        return calendar
    }
    // Wednesday 14 October 2026, 15:00 UTC
    let now = Date(timeIntervalSince1970: 1_791_990_000)

    func day(_ offset: Int) -> Date {
        calendar.date(byAdding: .day, value: offset, to: calendar.startOfDay(for: now))!
    }

    func total(_ words: Int, seconds: Double = 60, category: String = "general", app: String = "com.apple.Notes") -> DayTotal {
        var total = DayTotal()
        total.words = words
        total.dictations = 1
        total.audioSeconds = seconds
        total.categories = [category: words]
        total.apps = [app: words]
        return total
    }

    @Test func streaksAcrossAGap() {
        var days: [Date: DayTotal] = [:]
        for offset in [-9, -8, -7, -6, -5] { days[day(offset)] = total(50) } // 5-day run
        for offset in [-2, -1, 0] { days[day(offset)] = total(50) } // current 3-day run
        let insights = Insights.compute(days: days, now: now, calendar: calendar)
        #expect(insights.longestStreak == 5)
        #expect(insights.currentStreak == 3)
    }

    @Test func heatMapLevelsAndShape() {
        var days: [Date: DayTotal] = [:]
        days[day(0)] = total(400)
        days[day(-1)] = total(100)
        let insights = Insights.compute(days: days, now: now, weeks: 4, calendar: calendar)
        #expect(insights.heatMap.count == 4)
        #expect(insights.heatMap.allSatisfy { $0.count == 7 })
        let cells = insights.heatMap.flatMap { $0 }
        #expect(cells.first { $0.date == day(0) }?.level == 4)
        #expect(cells.first { $0.date == day(-1) }?.level == 1)
        #expect(cells.first { $0.date == day(-2) }?.level == 0)
        #expect(cells.contains { $0.isFuture }) // the rest of this week
        #expect(calendar.component(.weekday, from: insights.heatMap[0][0].date) == 1) // starts on Sunday
    }

    @Test func speedSharesAndTimeSaved() {
        var days: [Date: DayTotal] = [:]
        days[day(0)] = total(240, seconds: 120, category: "email", app: "com.apple.mail") // 120 wpm
        days[day(-1)] = total(120, seconds: 60, category: "workMessage", app: "com.tinyspeck.slackmacgap")
        let insights = Insights.compute(days: days, now: now, calendar: calendar)
        #expect(insights.wordsPerMinute == 120)
        #expect(insights.categories.first?.category == "email")
        #expect(insights.apps.first?.bundleID == "com.apple.mail")
        #expect(insights.minutesSaved == 6) // 360 words: 9 min typing − 3 min speaking
    }

    @Test func monthOverMonthChange() {
        var days: [Date: DayTotal] = [:]
        days[calendar.date(from: DateComponents(year: 2026, month: 10, day: 3))!] = total(110)
        days[calendar.date(from: DateComponents(year: 2026, month: 9, day: 3))!] = total(100)
        days[calendar.date(from: DateComponents(year: 2026, month: 9, day: 20))!] = total(999) // after the same period
        let change = Insights.compute(days: days, now: now, calendar: calendar).monthChange
        #expect(change.map { abs($0 - 0.1) < 0.0001 } == true)
    }
}
