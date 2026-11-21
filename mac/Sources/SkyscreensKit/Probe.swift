import AppKit
import CoreMedia
import QuartzCore

public enum Probe {
    static let size = (w: 1600, h: 1000)
    static let squareFromTopLeft = CGRect(x: 100, y: 100, width: 200, height: 200)
    static let flips = 40
    static let flipInterval: TimeInterval = 0.2
    static let motionSeconds: CFTimeInterval = 3
    static let bitrate = 15_000_000

    public static func run(hz: Double) {
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        let probe = Runner(hz: hz)
        DispatchQueue.main.async { probe.start() }
        app.run()
    }

    final class Runner {
        struct Samples {
            var captureMs: [Double] = []
            var encodeMs: [Double] = []
            var tileMs: [Double] = []
            var tileBytes: [Int] = []
            var dirtyPercent: [Double] = []
            var frameTimes: [CFTimeInterval] = []
        }

        let hz: Double
        var displays: VirtualDisplaySet?
        var window: NSWindow?
        let squareLayer = CALayer()
        let bar = CALayer()
        let capture = ScreenCapture()
        var encoder: VideoEncoder?
        let lock = NSLock()
        var pendingFlip: (time: CFTimeInterval, white: Bool)?
        var samples = Samples()
        var counting = false
        var frameIndex: Int64 = 0

        init(hz: Double) { self.hz = hz }

        func start() {
            let set: VirtualDisplaySet
            do {
                let spec = ScreenSpec(
                    name: "skyscreens probe",
                    role: .code,
                    width: Probe.size.w,
                    height: Probe.size.h,
                    refreshRate: hz
                )
                set = try VirtualDisplaySet([spec])
                try set.waitUntilReady()
                encoder = try VideoEncoder(width: Probe.size.w, height: Probe.size.h, fps: hz, bitrate: Probe.bitrate)
            } catch { finish("setup failed: \(error)") }
            displays = set
            let id = set.ids[0]
            NSScreen.whenAvailable(id) { [unowned self] found in
                guard let screen = found else { finish("test screen never appeared in NSScreen") }
                self.makeWindow(on: screen)
                self.capture.onFrame = { [unowned self] f in self.onFrame(f) }
                Task {
                    do {
                        try await self.capture.start(
                            displayID: id, width: Probe.size.w, height: Probe.size.h, fps: self.hz)
                    } catch {
                        self.finish(
                            """
                            capture failed: \(error)
                            Allow Screen Recording for the app that runs skyscreens, then try again.
                            """
                        )
                    }
                    try? await Task.sleep(nanoseconds: 500_000_000)
                    await MainActor.run { self.flip(remaining: Probe.flips) }
                }
            }
        }

        func makeWindow(on screen: NSScreen) {
            let w = NSWindow(contentRect: screen.frame, styleMask: .borderless, backing: .buffered, defer: false)
            w.setFrame(screen.frame, display: false)
            w.level = .statusBar
            w.backgroundColor = .black
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
            w.contentView = view
            w.orderFrontRegardless()
            window = w
        }

        func flip(remaining: Int) {
            guard remaining > 0 else { return motionPass() }
            let white = remaining % 2 == 0
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            squareLayer.backgroundColor = (white ? NSColor.white : NSColor.black).cgColor
            CATransaction.commit()
            CATransaction.flush()
            lock.withLock { pendingFlip = (CACurrentMediaTime(), white) }
            DispatchQueue.main.asyncAfter(deadline: .now() + Probe.flipInterval) { self.flip(remaining: remaining - 1) }
        }

        func motionPass() {
            lock.withLock { counting = true }
            let start = CACurrentMediaTime()
            let timer = Timer(timeInterval: 1 / 480, repeats: true) { t in
                let x = (CACurrentMediaTime() - start) * 800
                CATransaction.begin()
                CATransaction.setDisableActions(true)
                self.bar.frame.origin.x = CGFloat(x.truncatingRemainder(dividingBy: 1500))
                CATransaction.commit()
                if CACurrentMediaTime() - start > Probe.motionSeconds {
                    t.invalidate()
                    self.report()
                }
            }
            RunLoop.main.add(timer, forMode: .common)
        }

        func onFrame(_ f: ScreenCapture.Frame) {
            let square = Probe.squareFromTopLeft
            let isWhite = Self.pixelIsWhite(f.buffer, x: Int(square.midX), y: Int(square.midY))
            var flipTime: CFTimeInterval?
            lock.withLock {
                if counting { samples.frameTimes.append(f.arrivalTime) }
                if let p = pendingFlip, p.white == isWhite {
                    flipTime = p.time
                    pendingFlip = nil
                }
            }
            guard let flipTime else { return }
            let area = f.dirtyRegions.reduce(0) { $0 + $1.width * $1.height } / Double(Probe.size.w * Probe.size.h)
            let rect = f.dirtyRegions.reduce(CGRect.null) { $0.union($1) }
            let t0 = CACurrentMediaTime()
            let tile = LosslessTile.encode(f.buffer, rect: rect.isNull ? square : rect)
            let tileTime = CACurrentMediaTime() - t0
            frameIndex += 1
            let pts = CMTime(value: frameIndex, timescale: CMTimeScale(hz))
            encoder?.encode(f.buffer, pts: pts, forceKey: frameIndex == 1) { [unowned self] e in
                lock.withLock { samples.encodeMs.append(e.encodeSeconds * 1000) }
            }
            lock.withLock {
                samples.captureMs.append((f.arrivalTime - flipTime) * 1000)
                samples.tileMs.append(tileTime * 1000)
                samples.tileBytes.append(tile?.count ?? 0)
                samples.dirtyPercent.append(area * 100)
            }
        }

        static func pixelIsWhite(_ b: CVPixelBuffer, x: Int, y: Int) -> Bool {
            CVPixelBufferLockBaseAddress(b, .readOnly)
            defer { CVPixelBufferUnlockBaseAddress(b, .readOnly) }
            guard let base = CVPixelBufferGetBaseAddress(b) else { return false }
            return base.load(fromByteOffset: y * CVPixelBufferGetBytesPerRow(b) + x * 4, as: UInt8.self) > 128
        }

        func report() {
            encoder?.flush()
            let s = lock.withLock { samples }
            let p = StreamStats.percentile
            var fps = 0.0
            if let first = s.frameTimes.first, let last = s.frameTimes.last, last > first {
                fps = Double(s.frameTimes.count - 1) / (last - first)
            }
            let medianBytes = s.tileBytes.sorted()[safe: s.tileBytes.count / 2] ?? 0
            finish(
                String(
                    format: """
                        virtual display %dx%d @ %.0f Hz, %d flips measured
                          change -> captured frame:   p50 %5.1f  p99 %5.1f ms
                          H.264 encode:               p50 %5.1f  p99 %5.1f ms
                          lossless tile (LZ4):        p50 %5.2f  p99 %5.2f ms, %d bytes p50, dirty area %.1f%% of screen
                          frames delivered in motion: %.1f fps
                        """,
                    Probe.size.w,
                    Probe.size.h,
                    hz,
                    s.captureMs.count,
                    p(s.captureMs, 0.5),
                    p(s.captureMs, 0.99),
                    p(s.encodeMs, 0.5),
                    p(s.encodeMs, 0.99),
                    p(s.tileMs, 0.5),
                    p(s.tileMs, 0.99),
                    medianBytes,
                    p(s.dirtyPercent, 0.5),
                    fps
                )
            )
        }

        func finish(_ message: String) -> Never {
            print(message)
            exit(0)
        }
    }
}

extension Array {
    subscript(safe i: Int) -> Element? { indices.contains(i) ? self[i] : nil }
}
