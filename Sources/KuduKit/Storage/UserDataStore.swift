import Foundation
import KuduCore
import os

/// Everything the user customizes, saved as small JSON files in
/// ~/Library/Application Support/Kudu. Nothing is synced or uploaded.
@MainActor
public final class UserDataStore: ObservableObject {
    @Published public var cleanup: CleanupSettings { didSet { save(cleanup, as: "cleanup.json") } }
    @Published public var dictionary: [DictionaryEntry] { didSet { save(dictionary, as: "dictionary.json") } }
    @Published public var snippets: [Snippet] { didSet { save(snippets, as: "snippets.json") } }

    public let directory: URL
    private let log = Logger(subsystem: "app.kudu", category: "storage")

    public init(directory: URL = UserDataStore.defaultDirectory) {
        self.directory = directory
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        cleanup = Self.load(CleanupSettings.self, from: directory, file: "cleanup.json") ?? CleanupSettings()
        dictionary = Self.load([DictionaryEntry].self, from: directory, file: "dictionary.json") ?? []
        snippets = Self.load([Snippet].self, from: directory, file: "snippets.json") ?? []
    }

    public nonisolated static var defaultDirectory: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Kudu", isDirectory: true)
    }

    private static func load<T: Decodable>(_ type: T.Type, from directory: URL, file: String) -> T? {
        guard let data = try? Data(contentsOf: directory.appendingPathComponent(file)) else { return nil }
        return try? JSONDecoder().decode(T.self, from: data)
    }

    private func save<T: Encodable>(_ value: T, as file: String) {
        do {
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            try encoder.encode(value).write(to: directory.appendingPathComponent(file), options: .atomic)
        } catch {
            log.error("Couldn't save \(file): \(error.localizedDescription)")
        }
    }
}
