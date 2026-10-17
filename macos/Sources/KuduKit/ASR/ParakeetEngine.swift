import FluidAudio
import Foundation
import os

/// The speech models Kudu offers on a Mac, by their Kudu names. Both run on the Apple Neural
/// Engine through FluidAudio and are downloaded once; formatting uses Apple Intelligence, so
/// a Mac never downloads a writer model.
public enum SpeechModel: String, CaseIterable, Identifiable, Sendable {
    /// Smaller and faster (~230 MB): the 110M-parameter English model.
    case tiny = "kudu-tiny"
    /// The most accurate (~450 MB): the 0.6B-parameter English model. Recommended.
    case base = "kudu-base"

    public var id: String { rawValue }

    /// The name people see.
    public var name: String {
        switch self {
        case .tiny: "Kudu Tiny"
        case .base: "Kudu Base"
        }
    }

    public var summary: String {
        switch self {
        case .tiny: "Smaller and faster. A good fit for Macs with 8 GB of memory."
        case .base: "The most accurate. Recommended."
        }
    }

    /// Approximate download size, in megabytes.
    public var downloadMB: Int {
        switch self {
        case .tiny: 230
        case .base: 450
        }
    }

    /// The open model it is, and its license (shown in the credits, not in the UI).
    public var credit: String {
        switch self {
        case .tiny: "NVIDIA Parakeet TDT-CTC 110M, CC-BY-4.0"
        case .base: "NVIDIA Parakeet TDT 0.6B v2, CC-BY-4.0"
        }
    }

    var version: AsrModelVersion {
        switch self {
        case .tiny: .tdtCtc110m
        case .base: .v2
        }
    }

    public var directory: URL { AsrModels.defaultCacheDirectory(for: version) }

    public var isDownloaded: Bool { AsrModels.modelsExist(at: directory, version: version) }

    /// Downloads the model if it isn't on disk yet. `progress` receives 0...1.
    public func download(progress: @escaping @Sendable (Double) -> Void) async throws {
        try await AsrModels.download(version: version) { update in
            progress(update.fractionCompleted)
        }
    }

    /// Deletes the downloaded files to free space.
    public func remove() throws {
        if FileManager.default.fileExists(atPath: directory.path) {
            try FileManager.default.removeItem(at: directory)
        }
    }
}

/// Speech recognition on the Apple Neural Engine via FluidAudio, with the chosen
/// [`SpeechModel`].
public actor ParakeetEngine: TranscriptionEngine {
    public nonisolated let name = "Kudu speech"

    private var manager: AsrManager?
    private var loaded: SpeechModel?
    private let log = Logger(subsystem: "app.kudu", category: "asr")

    public init() {}

    /// Loads `model` (replacing another one) and runs one dummy inference so the first real
    /// dictation doesn't pay the Neural Engine compilation cost.
    public func load(_ model: SpeechModel) async throws {
        if manager != nil, loaded == model { return }
        await unload()
        let start = ContinuousClock.now
        let models = try await AsrModels.load(from: model.directory, version: model.version)
        let manager = AsrManager(config: .default)
        try await manager.loadModels(models)
        self.manager = manager
        self.loaded = model

        var state = TdtDecoderState.make(decoderLayers: model.version.decoderLayers)
        _ = try? await manager.transcribe([Float](repeating: 0, count: 16_000), decoderState: &state)
        log.info("Speech model \(model.rawValue) loaded and warm in \(ContinuousClock.now - start)")
    }

    public var isLoaded: Bool { manager != nil }

    public var model: SpeechModel? { loaded }

    public func unload() async {
        await manager?.cleanup()
        manager = nil
        loaded = nil
    }

    /// 200 ms of silence around each clip. Transducer models tend to drop or garble the first
    /// and last word when speech starts or stops right at the edge of the audio.
    private static let padding = [Float](repeating: 0, count: 3_200)

    public func transcribe(_ samples: [Float]) async throws -> String {
        guard let manager, let loaded else { throw TranscriptionError.notReady }
        var state = TdtDecoderState.make(decoderLayers: loaded.version.decoderLayers)
        let padded = Self.padding + samples + Self.padding
        let result = try await manager.transcribe(padded, decoderState: &state)
        return result.text
    }
}
