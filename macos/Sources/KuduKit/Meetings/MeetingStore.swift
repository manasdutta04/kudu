import Foundation
import KuduCore
import os

/// Meetings saved only on this Mac, one folder each in
/// ~/Library/Application Support/Kudu/Meetings/<id>/: `meeting.json` (title, times, notes)
/// and `transcript.jsonl` (one segment per line, appended while the meeting goes on, so
/// nothing is lost if Kudu quits). Newest first in memory. Kept until the user deletes them.
@MainActor
public final class MeetingStore: ObservableObject {
    @Published public private(set) var meetings: [Meeting] = []

    public let directory: URL
    private let log = Logger(subsystem: "app.kudu", category: "meetings")

    public init(directory: URL = UserDataStore.defaultDirectory.appendingPathComponent("Meetings")) {
        self.directory = directory
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        load()
    }

    public func folder(_ id: UUID) -> URL { directory.appendingPathComponent(id.uuidString) }

    /// Saves a meeting's details (creating its folder the first time).
    public func save(_ meeting: Meeting) {
        let folder = folder(meeting.id)
        do {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            try JSONEncoder.meetings.encode(meeting).write(to: folder.appendingPathComponent("meeting.json"), options: .atomic)
        } catch {
            log.error("Couldn't save meeting: \(error.localizedDescription, privacy: .public)")
        }
        if let index = meetings.firstIndex(where: { $0.id == meeting.id }) {
            meetings[index] = meeting
        } else {
            meetings.insert(meeting, at: 0)
            meetings.sort { $0.started > $1.started }
        }
    }

    /// Adds transcribed segments to a meeting's transcript file.
    public func append(_ segments: [MeetingSegment], to id: UUID) {
        guard !segments.isEmpty else { return }
        let url = folder(id).appendingPathComponent("transcript.jsonl")
        var data = Data()
        for segment in segments {
            guard let line = try? JSONEncoder.meetings.encode(segment) else { continue }
            data.append(line)
            data.append(0x0A)
        }
        do {
            if !FileManager.default.fileExists(atPath: url.path) {
                try data.write(to: url)
            } else {
                let handle = try FileHandle(forWritingTo: url)
                defer { try? handle.close() }
                try handle.seekToEnd()
                try handle.write(contentsOf: data)
            }
        } catch {
            log.error("Couldn't save the transcript: \(error.localizedDescription, privacy: .public)")
        }
    }

    public func segments(of id: UUID) -> [MeetingSegment] {
        let url = folder(id).appendingPathComponent("transcript.jsonl")
        guard let text = try? String(contentsOf: url, encoding: .utf8) else { return [] }
        return text.split(separator: "\n").compactMap { line in
            try? JSONDecoder.meetings.decode(MeetingSegment.self, from: Data(line.utf8))
        }
    }

    public func meeting(_ id: UUID) -> Meeting? { meetings.first { $0.id == id } }

    public func delete(_ id: UUID) {
        try? FileManager.default.removeItem(at: folder(id))
        meetings.removeAll { $0.id == id }
    }

    public func deleteAll() {
        for meeting in meetings { try? FileManager.default.removeItem(at: folder(meeting.id)) }
        meetings.removeAll()
    }

    /// Meetings that were still recording or being written up when Kudu last quit.
    public var interrupted: [Meeting] { meetings.filter { $0.status != .done } }

    private func load() {
        let folders = (try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)) ?? []
        meetings = folders.compactMap { folder in
            guard let data = try? Data(contentsOf: folder.appendingPathComponent("meeting.json")) else { return nil }
            return try? JSONDecoder.meetings.decode(Meeting.self, from: data)
        }
        .sorted { $0.started > $1.started }
    }
}

extension JSONEncoder {
    static let meetings: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()
}

extension JSONDecoder {
    static let meetings: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}
