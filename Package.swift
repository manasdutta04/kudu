// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "Kudu",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "Kudu", targets: ["Kudu"]),
    ],
    dependencies: [
        // Parakeet ASR + Silero VAD on the Neural Engine. Apache-2.0.
        // `traits: []` drops the ~8 MB NeMo text-normalization binary; Kudu does its own cleanup.
        .package(url: "https://github.com/FluidInference/FluidAudio.git", exact: "0.17.7", traits: []),
    ],
    targets: [
        // Pure, dependency-free logic (text cleanup, settings models). Fully unit-tested.
        .target(name: "KuduCore"),
        // System integration: hotkey, audio, ASR, insertion, overlay, permissions.
        .target(
            name: "KuduKit",
            dependencies: [
                "KuduCore",
                .product(name: "FluidAudio", package: "FluidAudio"),
            ]
        ),
        // The menu-bar app itself.
        .executableTarget(
            name: "Kudu",
            dependencies: ["KuduKit"],
            exclude: ["Info.plist", "Kudu.entitlements"]
        ),
        .testTarget(name: "KuduCoreTests", dependencies: ["KuduCore"]),
    ]
)
