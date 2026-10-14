import FluidAudio
import Foundation
import os

/// NVIDIA Parakeet TDT 0.6B v2 (English) running on the Apple Neural Engine via FluidAudio.
public actor ParakeetEngine: TranscriptionEngine {
    public nonisolated let name = "Parakeet TDT 0.6B v2"
    static let version: AsrModelVersion = .v2

    private var manager: AsrManager?
    private let log = Logger(subsystem: "app.kudu", category: "asr")

    public init() {}

    public static var modelDirectory: URL { AsrModels.defaultCacheDirectory(for: version) }

    public static var isDownloaded: Bool { AsrModels.modelsExist(at: modelDirectory, version: version) }

    /// Downloads the model if it isn't on disk yet. `progress` receives 0...1.
    public static func download(progress: @escaping @Sendable (Double) -> Void) async throws {
        try await AsrModels.download(version: version) { update in
            progress(update.fractionCompleted)
        }
    }

    /// Loads the model and runs one dummy inference so the first real dictation doesn't pay
    /// the Neural Engine compilation cost.
    public func load() async throws {
        guard manager == nil else { return }
        let start = ContinuousClock.now
        let models = try await AsrModels.load(from: Self.modelDirectory, version: Self.version)
        let manager = AsrManager(config: .default)
        try await manager.loadModels(models)
        self.manager = manager

        var state = TdtDecoderState.make(decoderLayers: Self.version.decoderLayers)
        _ = try? await manager.transcribe([Float](repeating: 0, count: 16_000), decoderState: &state)
        log.info("Parakeet loaded and warm in \(ContinuousClock.now - start)")
    }

    public var isLoaded: Bool { manager != nil }

    public func unload() async {
        await manager?.cleanup()
        manager = nil
    }

    /// 200 ms of silence around each clip. Transducer models tend to drop or garble the first
    /// and last word when speech starts or stops right at the edge of the audio.
    private static let padding = [Float](repeating: 0, count: 3_200)

    public func transcribe(_ samples: [Float]) async throws -> String {
        guard let manager else { throw TranscriptionError.notReady }
        var state = TdtDecoderState.make(decoderLayers: Self.version.decoderLayers)
        let padded = Self.padding + samples + Self.padding
        let result = try await manager.transcribe(padded, decoderState: &state)
        return result.text
    }
}
