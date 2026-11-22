import AppKit
import CoreMedia
import QuartzCore

public enum Probe {
    static let size = (width: 1600, height: 1000)
    static let squareFromTopLeft = CGRect(x: 100, y: 100, width: 200, height: 200)
    static let flips = 40
    static let flipInterval: TimeInterval = 0.2
    static let motionSeconds: CFTimeInterval = 3
    static let motionTick: TimeInterval = 1.0 / 480
    static let motionPointsPerSecond = 800.0
    static let motionTravel = 1500.0
    static let settleNanoseconds: UInt64 = 500_000_000
    static let bitrate = 15_000_000

    @MainActor
    public static func run(hz: Double) {
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        let runner = ProbeRunner(hz: hz)
        Task { @MainActor in runner.start() }
        app.run()
    }
}

@MainActor
final class ProbeRunner {
    private let hz: Double
    private var displays: VirtualDisplaySet?
    private var window: NSWindow?
    private let squareLayer = CALayer()
    private let bar = CALayer()
    private let capture = ScreenCapture()
    private var recorder: ProbeRecorder?
    private var motionTimer: Timer?

    init(hz: Double) { self.hz = hz }

    func start() {
        let spec = ScreenSpec(
            name: "skyscreens probe",
            role: .code,
            width: Probe.size.width,
            height: Probe.size.height,
            refreshRate: hz
        )
        let set: VirtualDisplaySet
        do {
            set = try VirtualDisplaySet([spec])
            try set.waitUntilReady()
            let encoder = try VideoEncoder(
                width: Probe.size.width,
                height: Probe.size.height,
                fps: hz,
                bitrate: Probe.bitrate
            )
            recorder = ProbeRecorder(encoder: encoder, hz: hz)
        } catch { finish("setup failed: \(error)") }
        displays = set
        let id = set.ids[0]
        NSScreen.whenAvailable(id) { [weak self] found in
            guard let found else { Self.exitWith("test screen never appeared in NSScreen") }
            self?.begin(on: found, displayID: id)
        }
    }

    private func begin(on screen: NSScreen, displayID: CGDirectDisplayID) {
        makeWindow(on: screen)
        if let recorder { capture.onFrame = { frame in recorder.record(frame) } }
        Task { @MainActor [weak self, capture, hz] in
            do {
                try await capture.start(
                    displayID: displayID,
                    width: Probe.size.width,
                    height: Probe.size.height,
                    fps: hz
                )
            } catch {
                self?.finish("capture failed: \(error)\nAllow Screen Recording for the app that runs skyscreens.")
            }
            try? await Task.sleep(nanoseconds: Probe.settleNanoseconds)
            self?.flip(remaining: Probe.flips)
        }
    }

    private func makeWindow(on screen: NSScreen) {
        let window = NSWindow(contentRect: screen.frame, styleMask: .borderless, backing: .buffered, defer: false)
        window.setFrame(screen.frame, display: false)
        window.level = .statusBar
        window.backgroundColor = .black
        let root = CALayer()
        root.backgroundColor = NSColor.black.cgColor
        let square = Probe.squareFromTopLeft
        let bottomLeftY = screen.frame.height - square.maxY
        squareLayer.frame = CGRect(x: square.minX, y: bottomLeftY, width: square.width, height: square.height)
        squareLayer.backgroundColor = NSColor.black.cgColor
        bar.frame = CGRect(x: 0, y: 500, width: 40, height: 300)
        bar.backgroundColor = NSColor.systemBlue.cgColor
        root.addSublayer(squareLayer)
        root.addSublayer(bar)
        let view = NSView(frame: NSRect(origin: .zero, size: screen.frame.size))
        view.layer = root
        view.wantsLayer = true
        window.contentView = view
        window.orderFrontRegardless()
        self.window = window
    }

    private func flip(remaining: Int) {
        guard remaining > 0 else {
            motionPass()
            return
        }
        let white = remaining.isMultiple(of: 2)
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        squareLayer.backgroundColor = (white ? NSColor.white : NSColor.black).cgColor
        CATransaction.commit()
        CATransaction.flush()
        recorder?.flipped(toWhite: white)
        Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(Probe.flipInterval * 1e9))
            self?.flip(remaining: remaining - 1)
        }
    }

    private func motionPass() {
        recorder?.startCountingFrames()
        let start = CACurrentMediaTime()
        let timer = Timer(timeInterval: Probe.motionTick, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.moveBar(since: start) }
        }
        RunLoop.main.add(timer, forMode: .common)
        motionTimer = timer
    }

    private func moveBar(since start: CFTimeInterval) {
        let elapsed = CACurrentMediaTime() - start
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        bar.frame.origin.x = (elapsed * Probe.motionPointsPerSecond).truncatingRemainder(dividingBy: Probe.motionTravel)
        CATransaction.commit()
        guard elapsed > Probe.motionSeconds else { return }
        motionTimer?.invalidate()
        finish(recorder?.report() ?? "no measurements")
    }

    private func finish(_ message: String) -> Never { Self.exitWith(message) }

    private static func exitWith(_ message: String) -> Never {
        print(message)
        exit(0)
    }
}
