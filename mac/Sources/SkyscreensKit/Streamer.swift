import AppKit
import CLink
import CoreMedia
import Foundation
import QuartzCore

public final class Streamer {
    public static let port = UInt16(SK_DEFAULT_PORT)
    static let questApp = "io.github.adrianetti.skyscreens/android.app.NativeActivity"
    static let fps = 60.0
    static let bitrate = 15_000_000
    static let connectionPoll: TimeInterval = 0.1

    private let options: StreamOptions
    private let server: OpaquePointer
    private let screen: Session.Screen
    private let capture = ScreenCapture()
    private let encoder: VideoEncoder
    private let lock = NSLock()
    private var lastFrame: CVPixelBuffer?
    private var needKey = true
    private var frameIndex: Int64 = 0
    private var interval = StreamStats()
    private var total = StreamStats()
    private var wasConnected = false
    private var everConnected = false
    private var connectedSeconds = 0.0
    private let started = CACurrentMediaTime()
    private let movingBar = MovingBar()

    public init(options: StreamOptions = StreamOptions()) throws {
        self.options = options
        _ = try Session.on(count: 1)
        guard let screen = Session.screens().first else { throw Session.SessionError.helperFailed("no screens") }
        guard let server = sk_server_start(Self.port) else {
            throw Session.SessionError.helperFailed("port \(Self.port) is busy")
        }
        self.server = server
        self.screen = screen
        encoder = try VideoEncoder(
            width: screen.spec.width,
            height: screen.spec.height,
            fps: Self.fps,
            bitrate: Self.bitrate
        )
        var ids: [UInt8] = [0]
        var roles: [UInt8] = [0]
        var widths = [UInt16(screen.spec.width)]
        var heights = [UInt16(screen.spec.height)]
        sk_set_screens(server, 1, &ids, &roles, &widths, &heights)
        sk_server_on_message(
            server,
            { ctx, kind, _, value in
                guard let ctx else { return }
                Unmanaged<Streamer>.fromOpaque(ctx).takeUnretainedValue().onLink(kind: kind, value: value)
            },
            Unmanaged.passUnretained(self).toOpaque()
        )
    }

    public func run() -> Never {
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        if options.launchHeadset { prepareHeadset() }
        if options.testPattern { showTestPattern() }
        startCapture()
        Timer.scheduledTimer(withTimeInterval: Self.connectionPoll, repeats: true) { [unowned self] _ in
            sendKeyframeOnNewConnection()
        }
        Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [unowned self] _ in report() }
        if let seconds = options.seconds {
            DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { [unowned self] in finish() }
        }
        for sig in [SIGINT, SIGTERM] {
            signal(sig, SIG_IGN)
            let source = DispatchSource.makeSignalSource(signal: sig, queue: .main)
            source.setEventHandler { [unowned self] in finish() }
            source.resume()
            _ = Unmanaged.passRetained(source)
        }
        app.run()
        exit(0)
    }

    private func showTestPattern() {
        NSScreen.whenAvailable(screen.id) { [unowned self] found in
            guard let found else { return print("test pattern: screen \(screen.id) never appeared") }
            movingBar.show(on: found)
        }
    }

    private func prepareHeadset() {
        let steps = [
            (["reverse", "tcp:\(Self.port)", "tcp:\(Self.port)"], "USB link"),
            (["shell", "am", "start", "-n", Self.questApp], "headset app"),
        ]
        for (args, what) in steps {
            print(
                Adb.run(args) ? "\(what): ok" : "\(what): failed (is the Quest plugged in with USB debugging allowed?)")
        }
    }

    private func startCapture() {
        capture.onFrame = { [unowned self] f in
            lock.withLock { lastFrame = f.buffer }
            send(f.buffer)
        }
        capture.onStop = { error in print("capture stopped: \(error)") }
        Task {
            do {
                try await capture.start(
                    displayID: screen.id,
                    width: screen.spec.width,
                    height: screen.spec.height,
                    fps: Self.fps
                )
                print("streaming \(screen.spec.name) \(screen.spec.width)x\(screen.spec.height)")
            } catch {
                print(
                    "capture failed: \(error)\nAllow Screen Recording for Skyscreens in System Settings, then try again."
                )
                Session.off()
                exit(1)
            }
        }
    }

    private func sendKeyframeOnNewConnection() {
        let connected = sk_server_connected(server)
        if connected && !wasConnected { resendKeyframe() }
        wasConnected = connected
        everConnected = everConnected || connected
        if connected { connectedSeconds += Self.connectionPoll }
    }

    private func send(_ buffer: CVPixelBuffer) {
        let (index, key) = lock.withLock { () -> (Int64, Bool) in
            frameIndex += 1
            defer { needKey = false }
            return (frameIndex, needKey)
        }
        let captured = CACurrentMediaTime()
        encoder.encode(buffer, pts: CMTime(value: index, timescale: CMTimeScale(Self.fps)), forceKey: key) {
            [unowned self] f in
            let sent = f.annexB.withUnsafeBytes { raw in
                sk_send_video(
                    server,
                    0,
                    f.isKey,
                    UInt64(captured * 1_000_000),
                    raw.bindMemory(to: UInt8.self).baseAddress,
                    raw.count,
                    UInt8(SK_PRIORITY_FOCUSED)
                )
            }
            guard sent else { return }
            lock.withLock {
                interval.frames += 1
                interval.keyframes += f.isKey ? 1 : 0
                interval.bytes += f.annexB.count
                interval.encodeMs.append(f.encodeSeconds * 1000)
            }
        }
    }

    private func resendKeyframe() {
        let frame = lock.withLock { () -> CVPixelBuffer? in
            needKey = true
            return lastFrame
        }
        if let frame { send(frame) }
    }

    private func onLink(kind: UInt8, value: UInt64) {
        switch Int32(kind) {
        case SK_EVENT_KEYFRAME_REQUEST: resendKeyframe()
        case SK_EVENT_PONG:
            let ms = (CACurrentMediaTime() * 1_000_000 - Double(value)) / 1000
            lock.withLock { interval.roundTripMs.append(ms) }
        default: break
        }
    }

    private func takeInterval() -> StreamStats {
        lock.withLock {
            let s = interval
            total.add(s)
            interval = StreamStats()
            return s
        }
    }

    private func report() {
        print(takeInterval().line(connected: sk_server_connected(server)))
    }

    private func finish() {
        _ = takeInterval()
        encoder.flush()
        let summary = lock.withLock { total }
        print(
            summary.summaryJSON(
                seconds: CACurrentMediaTime() - started,
                connectedSeconds: connectedSeconds,
                everConnected: everConnected
            )
        )
        capture.onStop = nil
        Session.off()
        exit(0)
    }
}
