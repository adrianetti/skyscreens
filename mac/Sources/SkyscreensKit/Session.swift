import Foundation

/// Starts and stops the displays helper, and remembers it in Application Support.
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

    static var stateDir: URL {
        let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("skyscreens")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    static var stateFile: URL { stateDir.appendingPathComponent("displays.json") }
    static var logFile: URL { stateDir.appendingPathComponent("displays.log") }

    /// The helper ships next to this executable.
    static var helperPath: String {
        URL(fileURLWithPath: CommandLine.arguments[0]).resolvingSymlinksInPath().deletingLastPathComponent().appendingPathComponent("skyscreens-displays").path
    }

    struct State: Decodable { let pid: Int32 }

    static func runningPID() -> Int32? {
        guard let data = try? Data(contentsOf: stateFile),
              let line = String(data: data, encoding: .utf8)?.split(separator: "\n").first,
              let state = try? JSONDecoder().decode(State.self, from: Data(line.utf8)),
              kill(state.pid, 0) == 0 else { return nil }
        return state.pid
    }

    /// Returns the helper's JSON description of the screens.
    public static func status() -> String? {
        runningPID() == nil ? nil : (try? String(contentsOf: stateFile, encoding: .utf8))?.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    public static func on(pixelsPerDegree: Double = 25, refreshRate: Double = 60) throws -> String {
        if let status = status() { return status }
        guard FileManager.default.isExecutableFile(atPath: helperPath) else { throw SessionError.helperMissing(helperPath) }
        try? FileManager.default.removeItem(at: stateFile)

        // Detached in its own session so it outlives this command.
        var actions: posix_spawn_file_actions_t?
        posix_spawn_file_actions_init(&actions)
        posix_spawn_file_actions_addopen(&actions, 0, "/dev/null", O_RDONLY, 0)
        posix_spawn_file_actions_addopen(&actions, 1, stateFile.path, O_WRONLY | O_CREAT | O_TRUNC, 0o644)
        posix_spawn_file_actions_addopen(&actions, 2, logFile.path, O_WRONLY | O_CREAT | O_TRUNC, 0o644)
        var attr: posix_spawnattr_t?
        posix_spawnattr_init(&attr)
        posix_spawnattr_setflags(&attr, Int16(POSIX_SPAWN_SETSID))
        defer { posix_spawn_file_actions_destroy(&actions); posix_spawnattr_destroy(&attr) }

        let argv = [helperPath, "--ppd", "\(pixelsPerDegree)", "--hz", "\(refreshRate)"]
        var cargs = argv.map { strdup($0) } + [nil]
        defer { cargs.forEach { free($0) } }
        var pid: pid_t = 0
        guard posix_spawn(&pid, helperPath, &actions, &attr, &cargs, environ) == 0 else { throw SessionError.helperFailed("spawn failed") }

        let deadline = Date().addingTimeInterval(8)
        while Date() < deadline {
            if let s = status() { return s }
            if kill(pid, 0) != 0 { break }
            usleep(50_000)
        }
        kill(pid, SIGTERM)
        throw SessionError.helperFailed((try? String(contentsOf: logFile, encoding: .utf8)) ?? "no output")
    }

    @discardableResult
    public static func off() -> Bool {
        guard let pid = runningPID() else { return false }
        kill(pid, SIGTERM)
        for _ in 0..<60 where kill(pid, 0) == 0 { usleep(50_000) }
        if kill(pid, 0) == 0 { kill(pid, SIGKILL) }
        try? FileManager.default.removeItem(at: stateFile)
        return true
    }
}
