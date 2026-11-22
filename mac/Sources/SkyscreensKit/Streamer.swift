import AppKit
import CLink
import CoreMedia
import Foundation
import QuartzCore

@MainActor
public final class Streamer {
    public static let port = UInt16(SK_DEFAULT_PORT)
    static let questApp = "io.github.adrianetti.skyscreens/android.app.NativeActivity"
    static let fps = 60.0
    static let bitrate = 15_000_000
    static let connectionPoll: TimeInterval = 0.1
    static let reportInterval: TimeInterval = 1

    private let options: StreamOptions
    private let screen: Session.Screen
    private let server: LinkServer
    private let pipeline: FramePipeline
    private let capture = ScreenCapture()
    private let movingBar = MovingBar()
    private var connection = ConnectionTracker()
    private let started = CACurrentMediaTime()

    public init(options: StreamOptions = StreamOptions()) throws {
        self.options = options
        _ = try Session.on(count: 1)
        guard let screen = Session.screens().first else { throw Session.SessionError.helperFailed("no screens") }
        guard let server = LinkServer.start(port: Self.port) else {
            throw Session.SessionError.helperFailed("port \(Self.port) is busy")
        }
        let encoder = try VideoEncoder(
            width: screen.spec.width,
            height: screen.spec.height,
            fps: Self.fps,
            bitrate: Self.bitrate
        )
        self.screen = screen
        self.server = server
        pipeline = FramePipeline(server: server, encoder: encoder, fps: Self.fps)
        server.announce(screen.spec)
        server.deliverEvents(to: pipeline)
    }

    public func run() -> Never {
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        if options.launchHeadset { prepareHeadset() }
        if options.testPattern { showTestPattern() }
        startCapture()
        repeatOnMain(every: Self.connectionPoll) { $0.pollConnection() }
        repeatOnMain(every: Self.reportInterval) { $0.report() }
        if let seconds = options.seconds {
            Task { @MainActor [weak self] in
                try? await Task.sleep(nanoseconds: UInt64(seconds * 1e9))
                self?.finish()
            }
        }
        finishOnSignals()
        app.run()
        exit(0)
    }

    private func repeatOnMain(every interval: TimeInterval, _ action: @escaping @MainActor (Streamer) -> Void) {
        Timer.scheduledTimer(withTimeInterval: interval, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                if let self { action(self) }
            }
        }
    }

    private func finishOnSignals() {
        for sig in [SIGINT, SIGTERM] {
            signal(sig, SIG_IGN)
            let source = DispatchSource.makeSignalSource(signal: sig, queue: .main)
            source.setEventHandler { [weak self] in
                MainActor.assumeIsolated { self?.finish() }
            }
            source.resume()
            _ = Unmanaged.passRetained(source)
        }
    }

    private func showTestPattern() {
        let id = screen.id
        NSScreen.whenAvailable(id) { [weak self] found in
            guard let found else { return print("test pattern: screen \(id) never appeared") }
            self?.movingBar.show(on: found)
        }
    }

    private func prepareHeadset() {
        let steps = [
            (["reverse", "tcp:\(Self.port)", "tcp:\(Self.port)"], "USB link"),
            (["shell", "am", "start", "-n", Self.questApp], "headset app"),
        ]
        for (args, what) in steps {
            let outcome = Adb.run(args) ? "ok" : "failed (is the Quest plugged in with USB debugging allowed?)"
            print("\(what): \(outcome)")
        }
    }

    private func startCapture() {
        let pipeline = pipeline
        capture.onFrame = { frame in pipeline.onCapture(frame.buffer) }
        capture.onStop = { error in print("capture stopped: \(error)") }
        let spec = screen.spec
        let id = screen.id
        Task { @MainActor [capture] in
            do {
                try await capture.start(displayID: id, width: spec.width, height: spec.height, fps: Self.fps)
                print("streaming \(spec.name) \(spec.width)x\(spec.height)")
            } catch {
                print("capture failed: \(error)")
                print("Allow Screen Recording for Skyscreens in System Settings, then try again.")
                Session.off()
                exit(1)
            }
        }
    }

    private func pollConnection() {
        if connection.observe(connected: server.isConnected, after: Self.connectionPoll) {
            pipeline.resendKeyframe()
        }
    }

    private func report() {
        print(pipeline.takeInterval().line(connected: server.isConnected))
    }

    private func finish() -> Never {
        let summary = pipeline.totals()
        print(
            summary.summaryJSON(
                seconds: CACurrentMediaTime() - started,
                everConnected: connection.everConnected,
                connectedSeconds: connection.connectedSeconds
            )
        )
        capture.onStop = nil
        Session.off()
        exit(0)
    }
}
