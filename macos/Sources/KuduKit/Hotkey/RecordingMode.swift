import Foundation

/// How the dictation key starts and stops recording.
public enum RecordingMode: String, CaseIterable, Sendable, Codable, Identifiable {
    /// Hold to talk; double-tap to record hands-free, tap again to finish.
    case holdOrDoubleTap
    /// Tap once to start, tap again to finish. Holding still works as push-to-talk.
    case tapToToggle
    /// Hold to talk only.
    case holdOnly

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .holdOrDoubleTap: "Hold, or double-tap for hands-free"
        case .tapToToggle: "Tap to start, tap to stop"
        case .holdOnly: "Hold to talk only"
        }
    }

    public func hint(for key: TriggerKey) -> String {
        switch self {
        case .holdOrDoubleTap: "Hold \(key.displayName) and speak, or double-tap it to talk hands-free."
        case .tapToToggle: "Tap \(key.displayName) to start, tap again to finish."
        case .holdOnly: "Hold \(key.displayName), speak, release."
        }
    }
}
