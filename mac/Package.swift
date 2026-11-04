// swift-tools-version: 5.10
import Foundation
import PackageDescription

// The Rust link crate, built by scripts/bundle.sh as a static library.
let rustLib = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("../target/release").standardized.path

let package = Package(
    name: "skyscreens",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "skyscreens", targets: ["skyscreens"]),
        .executable(name: "skyscreens-displays", targets: ["skyscreens-displays"]),
    ],
    targets: [
        // Declarations for CoreGraphics' private CGVirtualDisplay API.
        .target(name: "CVirtualDisplay", linkerSettings: [.linkedFramework("CoreGraphics")]),
        .target(name: "CLink", linkerSettings: [.unsafeFlags(["-L\(rustLib)"]), .linkedLibrary("link")]),
        .target(
            name: "SkyscreensKit",
            dependencies: ["CVirtualDisplay", "CLink"],
            linkerSettings: [
                .linkedFramework("AppKit"),
                .linkedFramework("ScreenCaptureKit"),
                .linkedFramework("VideoToolbox"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("CoreVideo"),
            ]
        ),
        .executableTarget(name: "skyscreens-displays", dependencies: ["SkyscreensKit"]),
        .executableTarget(name: "skyscreens", dependencies: ["SkyscreensKit"]),
    ]
)
