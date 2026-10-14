import Foundation
import KuduCore
import os

/// How long dictation history is kept on disk.
public enum HistoryRetention: String, CaseIterable, Identifiable, Sendable {
    case days30, days90, forever

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .days30: "30 days"
        case .days90: "90 days"
        case .forever: "Forever"
        }
    }

    var interval: TimeInterval? {
        switch self {
        case .days30: 30 * 86_400
        case .days90: 90 * 86_400
        case .forever: nil
        }
    }
}

/// Dictation history, saved only on this Mac as JSON Lines in
/// ~/Library/Application Support/Kudu/history.jsonl. Newest first in memory.
@MainActor
public final class HistoryStore: ObservableObject {
    @Published public private(set) var records: [DictationRecord] = []
    @Published public private(set) var stats = UsageStats.empty
    @Published public var retention: HistoryRetention {
        didSet {
            UserDefaults.standard.set(retention.rawValue, forKey: Self.retentionKey)
            prune()
        }
    }

    private let fileURL: URL
    private let log = Logger(subsystem: "app.kudu", category: "history")
    private static let retentionKey = "historyRetention"

    public init(directory: URL = UserDataStore.defaultDirectory) {
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        fileURL = directory.appendingPathComponent("history.jsonl")
        retention = UserDefaults.standard.string(forKey: Self.retentionKey)
            .flatMap(HistoryRetention.init(rawValue:)) ?? .days90
        load()
        prune()
    }

    public func append(_ record: DictationRecord) {
        let index = records.firstIndex { $0.date <= record.date } ?? records.endIndex
        records.insert(record, at: index)
        refreshStats()
        do {
            var line = try JSONEncoder.history.encode(record)
            line.append(0x0A)
            if let handle = try? FileHandle(forWritingTo: fileURL) {
                defer { try? handle.close() }
                try handle.seekToEnd()
                try handle.write(contentsOf: line)
            } else {
                try line.write(to: fileURL, options: .atomic)
            }
        } catch {
            log.error("Couldn't save history: \(error.localizedDescription)")
        }
    }

    public func delete(_ id: DictationRecord.ID) {
        records.removeAll { $0.id == id }
        rewrite()
    }

    public func clear() {
        records.removeAll()
        rewrite()
    }

    /// Records grouped by calendar day, newest day first, optionally filtered by a search.
    public func groupedByDay(matching query: String = "") -> [(day: Date, records: [DictationRecord])] {
        let calendar = Calendar.current
        let filtered = query.isEmpty ? records : records.filter {
            $0.text.localizedCaseInsensitiveContains(query)
        }
        let groups = Dictionary(grouping: filtered) { calendar.startOfDay(for: $0.date) }
        return groups.keys.sorted(by: >).map { day in
            (day: day, records: groups[day]!.sorted { $0.date > $1.date })
        }
    }

    // MARK: - Private

    private func load() {
        guard let data = try? Data(contentsOf: fileURL) else { return }
        var loaded: [DictationRecord] = []
        for line in data.split(separator: 0x0A) where !line.isEmpty {
            if let record = try? JSONDecoder.history.decode(DictationRecord.self, from: Data(line)) {
                loaded.append(record)
            }
        }
        records = loaded.sorted { $0.date > $1.date }
        refreshStats()
    }

    private func prune() {
        guard let interval = retention.interval else { return }
        let cutoff = Date.now.addingTimeInterval(-interval)
        let kept = records.filter { $0.date >= cutoff }
        guard kept.count != records.count else { return }
        records = kept
        rewrite()
    }

    private func rewrite() {
        refreshStats()
        do {
            var data = Data()
            for record in records.reversed() {
                data.append(try JSONEncoder.history.encode(record))
                data.append(0x0A)
            }
            try data.write(to: fileURL, options: .atomic)
        } catch {
            log.error("Couldn't rewrite history: \(error.localizedDescription)")
        }
    }

    private func refreshStats() {
        stats = UsageStats.compute(records)
    }
}

private extension JSONEncoder {
    static let history: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()
}

private extension JSONDecoder {
    static let history: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}
