// swift-tools-version: 6.2
import Foundation
import PackageDescription

let strictSettings: [SwiftSetting] = [
    .enableUpcomingFeature("ExistentialAny"),
    .enableUpcomingFeature("MemberImportVisibility"),
    .strictMemorySafety(),
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
        .target(
            name: "SkyscreensKit",
            dependencies: ["CVirtualDisplay"],
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
