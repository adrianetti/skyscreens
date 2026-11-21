import Foundation

enum Adb {
    static var path: String? {
        let env = ProcessInfo.processInfo.environment
        let candidates = [
            env["ANDROID_HOME"].map { "\($0)/platform-tools/adb" },
            "/opt/homebrew/share/android-commandlinetools/platform-tools/adb",
            "/opt/homebrew/bin/adb",
            "/usr/local/bin/adb",
        ]
        return candidates.compactMap { $0 }.first { FileManager.default.isExecutableFile(atPath: $0) }
    }

    @discardableResult
    static func run(_ args: [String]) -> Bool {
        guard let path else { return false }
        let p = Process()
        p.executableURL = URL(fileURLWithPath: path)
        p.arguments = args
        p.standardOutput = FileHandle.nullDevice
        p.standardError = FileHandle.nullDevice
        do { try p.run() } catch { return false }
        p.waitUntilExit()
        return p.terminationStatus == 0
    }
}
