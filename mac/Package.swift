// swift-tools-version: 6.0
import Foundation
import PackageDescription

let rustLib = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("../target/release")
    .standardized.path

let strictSettings: [SwiftSetting] = [
    .enableUpcomingFeature("ExistentialAny"),
    .enableUpcomingFeature("MemberImportVisibility"),
]

let package = Package(
    name: "skyscreens",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "skyscreens", targets: ["skyscreens"]),
        .executable(name: "skyscreens-displays", targets: ["skyscreens-displays"]),
    ],
    targets: [
        .target(name: "CVirtualDisplay", linkerSettings: [.linkedFramework("CoreGraphics")]),
        .target(name: "CLink", linkerSettings: [.unsafeFlags(["-L\(rustLib)"]), .linkedLibrary("link")]),
        .target(
            name: "SkyscreensKit",
            dependencies: ["CVirtualDisplay", "CLink"],
            swiftSettings: strictSettings,
            linkerSettings: [
                .linkedFramework("AppKit"),
                .linkedFramework("ScreenCaptureKit"),
                .linkedFramework("VideoToolbox"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("CoreVideo"),
            ]
        ),
        .executableTarget(name: "skyscreens-displays", dependencies: ["SkyscreensKit"], swiftSettings: strictSettings),
        .executableTarget(name: "skyscreens", dependencies: ["SkyscreensKit"], swiftSettings: strictSettings),
        .testTarget(name: "SkyscreensKitTests", dependencies: ["SkyscreensKit"], swiftSettings: strictSettings),
    ]
)
