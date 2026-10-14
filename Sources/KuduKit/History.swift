import Foundation

/// One finished dictation. History lives in memory only and is gone when Kudu quits.
public struct HistoryItem: Identifiable, Sendable, Equatable {
    public let id = UUID()
    public let date: Date
    /// Exactly what the speech model returned, before cleanup.
    public let raw: String
    /// What was inserted.
    public let text: String
    public let latencyMs: Int
}
