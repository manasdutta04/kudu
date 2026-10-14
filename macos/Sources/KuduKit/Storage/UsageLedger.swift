import Foundation
import KuduCore
import os

/// Daily usage totals for the Insights page: words, dictations, time, cleanups and where you
/// dictate. Totals only (never what was said), so they are kept even when the history
/// retention deletes old dictations. Saved in ~/Library/Application Support/Kudu/usage.json.
@MainActor
public final class UsageLedger: ObservableObject {
    @Published public private(set) var days: [Date: DayTotal] = [:]

    private let fileURL: URL
    private let calendar = Calendar.current
    private let log = Logger(subsystem: "app.kudu", category: "usage")
    private static let dayFormat: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter
    }()

    public init(directory: URL = UserDataStore.defaultDirectory, backfillFrom history: HistoryStore? = nil) {
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        fileURL = directory.appendingPathComponent("usage.json")
        if let data = try? Data(contentsOf: fileURL),
           let stored = try? JSONDecoder().decode([String: DayTotal].self, from: data) {
            days = Dictionary(uniqueKeysWithValues: stored.compactMap { key, value in
                Self.dayFormat.date(from: key).map { (calendar.startOfDay(for: $0), value) }
            })
        } else if let history, !history.records.isEmpty {
            // First run with Insights: start from the history that's already saved.
            for record in history.records { add(record) }
            save()
        }
    }

    public var insights: Insights { Insights.compute(days: days) }

    public func record(_ record: DictationRecord) {
        add(record)
        save()
    }

    public func recordLearned(_ count: Int) {
        let day = calendar.startOfDay(for: .now)
        days[day, default: DayTotal()].learned = max(0, days[day, default: DayTotal()].learned + count)
        save()
    }

    private func add(_ record: DictationRecord) {
        let day = calendar.startOfDay(for: record.date)
        var total = days[day] ?? DayTotal()
        total.words += record.words
        total.dictations += 1
        total.audioSeconds += record.audioSeconds
        total.cleanups += record.cleanups ?? 0
        total.dictionaryFixes += record.dictionaryFixes ?? 0
        let category = AppContext.category(for: record.bundleID).rawValue
        total.categories[category, default: 0] += record.words
        if let bundleID = record.bundleID { total.apps[bundleID, default: 0] += record.words }
        days[day] = total
    }

    private func save() {
        let stored = Dictionary(uniqueKeysWithValues: days.map { (Self.dayFormat.string(from: $0.key), $0.value) })
        do {
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys]
            try encoder.encode(stored).write(to: fileURL, options: .atomic)
        } catch {
            log.error("Couldn't save usage: \(error.localizedDescription)")
        }
    }
}
