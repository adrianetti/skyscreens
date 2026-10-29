// swift-tools-version: 5.10
import PackageDescription

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
        .target(
            name: "SkyscreensKit",
            dependencies: ["CVirtualDisplay"],
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
