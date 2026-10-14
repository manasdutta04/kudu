// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "Kudu",
    platforms: [.macOS(.v14)],
    targets: [
        // Pure, dependency-free logic (text cleanup, settings models). Fully unit-tested.
        .target(name: "KuduCore"),
    ]
)
