import CLink
import CoreMedia
import Foundation
import QuartzCore

/// `skyscreens stream`: Phase 1. One screen over USB, sent as low-latency H.264.
///
/// Prints a line per second with the measured round trip from capture on the Mac to the
/// frame being decoded on the headset (plus one USB hop back).
public final class Streamer {
    static let port: UInt16 = 7243
    static let questApp = "io.github.adrianetti.skyscreens/android.app.NativeActivity"

    private let server: OpaquePointer
    private let screen: Session.Screen
    private let capture = ScreenCapture()
    private let encoder: VideoEncoder
    private let lock = NSLock()
    private var lastFrame: CVPixelBuffer?
    private var needKey = true
    private var frameIndex: Int64 = 0
    private var stats = Stats()
    private var wasConnected = false

    struct Stats {
        var frames = 0, bytes = 0, encodeMs: [Double] = [], roundTripMs: [Double] = []
    }

    public init() throws {
        _ = try Session.on(count: 1)
        guard let screen = Session.screens().first else { throw Session.SessionError.helperFailed("no screens") }
        guard let server = sk_server_start(Self.port) else { throw Session.SessionError.helperFailed("port \(Self.port) is busy") }
        self.server = server
        self.screen = screen
        encoder = try VideoEncoder(width: screen.spec.width, height: screen.spec.height, fps: 60, bitrate: 15_000_000)

        var ids: [UInt8] = [0], roles: [UInt8] = [0], widths = [UInt16(screen.spec.width)], heights = [UInt16(screen.spec.height)]
        sk_set_screens(server, 1, &ids, &roles, &widths, &heights)
        sk_server_on_message(server, { ctx, kind, _, value in
            Unmanaged<Streamer>.fromOpaque(ctx!).takeUnretainedValue().onLink(kind: kind, value: value)
        }, Unmanaged.passUnretained(self).toOpaque())
    }

    public func run() -> Never {
        for (args, what) in [(["reverse", "tcp:\(Self.port)", "tcp:\(Self.port)"], "USB link"), (["shell", "am", "start", "-n", Self.questApp], "headset app")] {
            print(Adb.run(args) ? "\(what): ok" : "\(what): failed (is the Quest plugged in with USB debugging allowed?)")
        }
        capture.onFrame = { [unowned self] f in
            lock.withLock { lastFrame = f.buffer }
            send(f.buffer)
        }
        capture.onStop = { error in print("capture stopped: \(error)") }
        Task {
            do {
                try await capture.start(displayID: screen.id, width: screen.spec.width, height: screen.spec.height, fps: 60)
                print("streaming \(screen.spec.name) \(screen.spec.width)x\(screen.spec.height)")
            } catch {
                print("capture failed: \(error)\nAllow Screen Recording for Skyscreens in System Settings, then try again.")
                exit(1)
            }
        }
        // A new headset connection needs a keyframe, even if the screen is not changing.
        Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [unowned self] _ in
            let connected = sk_server_connected(server)
            if connected && !wasConnected { resendKeyframe() }
            wasConnected = connected
        }
        Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [unowned self] _ in report() }
        for sig in [SIGINT, SIGTERM] {
            signal(sig, SIG_IGN)
            let source = DispatchSource.makeSignalSource(signal: sig, queue: .main)
            source.setEventHandler { Session.off(); exit(0) }
            source.resume()
            _ = Unmanaged.passRetained(source)
        }
        RunLoop.main.run()
        exit(0)
    }

    private func send(_ buffer: CVPixelBuffer) {
        let (index, key) = lock.withLock { () -> (Int64, Bool) in
            frameIndex += 1
            defer { needKey = false }
            return (frameIndex, needKey)
        }
        let captured = CACurrentMediaTime()
        encoder.encode(buffer, pts: CMTime(value: index, timescale: 60), forceKey: key) { [unowned self] f in
            let sent = f.data.withUnsafeBytes { raw in
                sk_send_video(server, 0, f.isKey, UInt64(captured * 1_000_000), raw.bindMemory(to: UInt8.self).baseAddress, raw.count, 2)
            }
            guard sent else { return }
            lock.withLock {
                stats.frames += 1
                stats.bytes += f.data.count
                stats.encodeMs.append(f.latency * 1000)
            }
        }
    }

    private func resendKeyframe() {
        let frame = lock.withLock { () -> CVPixelBuffer? in needKey = true; return lastFrame }
        if let frame { send(frame) }
    }

    private func onLink(kind: UInt8, value: UInt64) {
        switch kind {
        case 1: resendKeyframe()
        case 2:
            let ms = (CACurrentMediaTime() * 1_000_000 - Double(value)) / 1000
            lock.withLock { stats.roundTripMs.append(ms) }
        default: break
        }
    }

    private func report() {
        let s = lock.withLock { () -> Stats in defer { stats = Stats() }; return stats }
        func p(_ a: [Double], _ q: Double) -> Double { let x = a.sorted(); return x.isEmpty ? .nan : x[min(x.count - 1, Int(q * Double(x.count)))] }
        print(String(format: "%@  %2d fps  %5.1f Mbps  encode %4.1f ms  capture->decoded on Quest p50 %5.1f  p95 %5.1f ms",
                     sk_server_connected(server) ? "connected   " : "waiting...  ", s.frames, Double(s.bytes) * 8 / 1e6,
                     p(s.encodeMs, 0.5), p(s.roundTripMs, 0.5), p(s.roundTripMs, 0.95)))
    }
}

/// Finds and runs adb.
enum Adb {
    static var path: String? {
        let env = ProcessInfo.processInfo.environment
        let candidates = [env["ANDROID_HOME"].map { "\($0)/platform-tools/adb" }, "/opt/homebrew/share/android-commandlinetools/platform-tools/adb",
                          "/opt/homebrew/bin/adb", "/usr/local/bin/adb"].compactMap { $0 }
        return candidates.first { FileManager.default.isExecutableFile(atPath: $0) }
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
