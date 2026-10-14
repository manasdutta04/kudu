import Foundation

/// A speech-to-text backend. Implementations must be safe to call from any task.
public protocol TranscriptionEngine: Sendable {
    var name: String { get }
    /// Transcribes 16 kHz mono Float32 samples.
    func transcribe(_ samples: [Float]) async throws -> String
}

public enum TranscriptionError: LocalizedError {
    case notReady

    public var errorDescription: String? {
        switch self {
        case .notReady: "The speech model isn't loaded yet."
        }
    }
}
