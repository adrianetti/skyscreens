import CoreGraphics
import Foundation

public enum Session {
    public enum SessionError: Error, CustomStringConvertible {
        case helperMissing(String)
        case helperFailed(String)

        public var description: String {
            switch self {
            case .helperMissing(let path): "displays helper not found at \(path)"
            case .helperFailed(let log): "displays helper failed: \(log)"
            }
        }
    }

    public struct Screen: Decodable {
        public let id: CGDirectDisplayID
        public let spec: ScreenSpec
    }

    struct State: Decodable { let pid: Int32 }

    static let startTimeout: TimeInterval = 8
    static let pollInterval: useconds_t = 50_000

    static var stateDir: URL {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        let dir = support.appendingPathComponent("skyscreens")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    static var stateFile: URL { stateDir.appendingPathComponent("displays.json") }
    static var logFile: URL { stateDir.appendingPathComponent("displays.log") }

    static var helperPath: String {
        URL(fileURLWithPath: CommandLine.arguments[0]).resolvingSymlinksInPath().deletingLastPathComponent()
            .appendingPathComponent("skyscreens-displays").path
    }

    public static func screens() -> [Screen] {
        struct Info: Decodable { let screens: [Screen] }
        guard let s = status(), let info = try? JSONDecoder().decode(Info.self, from: Data(s.utf8)) else { return [] }
        return info.screens
    }

    static func runningPID() -> Int32? {
        guard let data = try? Data(contentsOf: stateFile),
            let line = String(decoding: data, as: UTF8.self).split(separator: "\n").first,
            let state = try? JSONDecoder().decode(State.self, from: Data(line.utf8)),
            kill(state.pid, 0) == 0
        else { return nil }
        return state.pid
    }

    public static func status() -> String? {
        guard runningPID() != nil else { return nil }
        return (try? String(contentsOf: stateFile, encoding: .utf8))?.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    public static func on(pixelsPerDegree: Double = 25, refreshRate: Double = 60, count: Int = 4) throws -> String {
        if let status = status() { return status }
        guard FileManager.default.isExecutableFile(atPath: helperPath) else {
            throw SessionError.helperMissing(helperPath)
        }
        try? FileManager.default.removeItem(at: stateFile)
        let argv = [helperPath, "--ppd", "\(pixelsPerDegree)", "--hz", "\(refreshRate)", "--count", "\(count)"]
        let pid = try spawnDetached(argv)
        let deadline = Date().addingTimeInterval(startTimeout)
        while Date() < deadline {
            if let s = status() { return s }
            if kill(pid, 0) != 0 { break }
            usleep(pollInterval)
        }
        kill(pid, SIGTERM)
        throw SessionError.helperFailed((try? String(contentsOf: logFile, encoding: .utf8)) ?? "no output")
    }

    static func spawnDetached(_ argv: [String]) throws -> pid_t {
        var actions: posix_spawn_file_actions_t?
        posix_spawn_file_actions_init(&actions)
        posix_spawn_file_actions_addopen(&actions, 0, "/dev/null", O_RDONLY, 0)
        posix_spawn_file_actions_addopen(&actions, 1, stateFile.path, O_WRONLY | O_CREAT | O_TRUNC, 0o644)
        posix_spawn_file_actions_addopen(&actions, 2, logFile.path, O_WRONLY | O_CREAT | O_TRUNC, 0o644)
        var attr: posix_spawnattr_t?
        posix_spawnattr_init(&attr)
        posix_spawnattr_setflags(&attr, Int16(POSIX_SPAWN_SETSID))
        defer {
            posix_spawn_file_actions_destroy(&actions)
            posix_spawnattr_destroy(&attr)
        }
        var cargs = argv.map { strdup($0) } + [nil]
        defer {
            for arg in cargs { free(arg) }
        }
        var pid: pid_t = 0
        guard posix_spawn(&pid, argv[0], &actions, &attr, &cargs, environ) == 0 else {
            throw SessionError.helperFailed("spawn failed")
        }
        return pid
    }

    @discardableResult
    public static func off() -> Bool {
        guard let pid = runningPID() else { return false }
        kill(pid, SIGTERM)
        for _ in 0..<60 where kill(pid, 0) == 0 { usleep(pollInterval) }
        if kill(pid, 0) == 0 { kill(pid, SIGKILL) }
        try? FileManager.default.removeItem(at: stateFile)
        return true
    }
}
