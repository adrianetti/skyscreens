import Foundation
import os

final class LinkHelper: Sendable {
    enum LaunchError: Error, CustomStringConvertible {
        case missing(String)
        case portBusy(UInt16)
        case exited(Int32)
        case silent

        var description: String {
            switch self {
            case .missing(let path): "link helper not found at \(path)"
            case .portBusy(let port): "port \(port) is busy"
            case .exited(let status): "link helper exited with status \(status)"
            case .silent: "link helper did not start listening"
            }
        }
    }

    static let executableName = "skyscreens-link"
    static let portBusyStatus: Int32 = 3
    static let startTimeout: DispatchTimeInterval = .seconds(5)

    private let process: Process
    private let input: FileHandle
    private let writeLock = OSAllocatedUnfairLock()
    private let events = OSAllocatedUnfairLock(initialState: EventStream())
    private let connected = OSAllocatedUnfairLock(initialState: false)

    static var executablePath: String {
        let executable = URL(fileURLWithPath: CommandLine.arguments[0]).resolvingSymlinksInPath()
        return executable.deletingLastPathComponent().appendingPathComponent(executableName).path
    }

    init(port: UInt16, onEvent: @escaping @Sendable (LinkEvent) -> Void) throws {
        let path = Self.executablePath
        guard FileManager.default.isExecutableFile(atPath: path) else { throw LaunchError.missing(path) }
        signal(SIGPIPE, SIG_IGN)
        let stdin = Pipe()
        let stdout = Pipe()
        process = Process()
        process.executableURL = URL(fileURLWithPath: path)
        process.arguments = ["--port", "\(port)"]
        process.standardInput = stdin
        process.standardOutput = stdout
        input = stdin.fileHandleForWriting
        let listening = DispatchSemaphore(value: 0)
        stdout.fileHandleForReading.readabilityHandler = { [events, connected] handle in
            for event in events.withLock({ $0.append(handle.availableData) }) {
                switch event {
                case .listening: listening.signal()
                case .connected: connected.withLock { $0 = true }
                case .disconnected: connected.withLock { $0 = false }
                case .keyframeRequest, .pong: break
                }
                onEvent(event)
            }
        }
        try process.run()
        guard listening.wait(timeout: .now() + Self.startTimeout) == .success else {
            throw process.isRunning ? LaunchError.silent : Self.launchError(process.terminationStatus, port: port)
        }
    }

    private static func launchError(_ status: Int32, port: UInt16) -> LaunchError {
        status == portBusyStatus ? .portBusy(port) : .exited(status)
    }

    var isConnected: Bool { connected.withLock { $0 } }

    func announce(_ specs: [ScreenSpec]) -> Bool {
        write(BridgeCodec.screensRecord(specs))
    }

    func sendVideo(_ annexB: Data, isKey: Bool, capturedMicros: UInt64) -> Bool {
        guard isConnected else { return false }
        return write(BridgeCodec.videoRecord(screen: 0, isKey: isKey, capturedMicros: capturedMicros, annexB: annexB))
    }

    func stop() {
        writeLock.withLock { input.closeFile() }
        process.waitUntilExit()
    }

    private func write(_ record: Data) -> Bool {
        writeLock.withLock {
            do {
                try input.write(contentsOf: record)
                return true
            } catch {
                return false
            }
        }
    }
}
