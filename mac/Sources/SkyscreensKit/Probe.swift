import AppKit
import CoreMedia
import QuartzCore

/// `skyscreens probe`: measures the Mac half of the pipeline on a real virtual display.
///
/// A window on a test screen flips a square between black and white. Each flip is timed
/// until ScreenCaptureKit delivers the changed frame, then through the H.264 encoder and the
/// LZ4 lossless lane. A second pass moves a bar every frame to count delivered frames per second.
public enum Probe {
    static let size = (w: 1600, h: 1000)
    static let square = CGRect(x: 100, y: 100, width: 200, height: 200) // from the top-left, in pixels

    public static func run(hz: Double) {
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        let probe = Runner(hz: hz)
        DispatchQueue.main.async { probe.start() }
        app.run()
    }

    final class Runner {
        let hz: Double
        var displays: VirtualDisplaySet?
        var window: NSWindow?
        let squareLayer = CALayer(), bar = CALayer()
        let capture = ScreenCapture()
        var encoder: VideoEncoder?
        let lock = NSLock()
        var pendingFlip: (time: CFTimeInterval, white: Bool)?
        var captureMs: [Double] = [], encodeMs: [Double] = [], tileMs: [Double] = [], tileBytes: [Int] = [], dirtyArea: [Double] = []
        var frameTimes: [CFTimeInterval] = []
        var counting = false
        var frameIndex: Int64 = 0

        init(hz: Double) { self.hz = hz }

        func start() {
            do {
                let set = try VirtualDisplaySet([ScreenSpec(name: "skyscreens probe", role: .code, width: Probe.size.w, height: Probe.size.h, refreshRate: hz)])
                try set.waitUntilReady()
                displays = set
                encoder = try VideoEncoder(width: Probe.size.w, height: Probe.size.h, fps: hz, bitrate: 15_000_000)
            } catch { finish("setup failed: \(error)") }
            let id = displays!.ids[0]
            waitForScreen(id) { screen in
                self.makeWindow(on: screen)
                self.capture.onFrame = { [unowned self] f in self.onFrame(f) }
                Task {
                    do { try await self.capture.start(displayID: id, width: Probe.size.w, height: Probe.size.h, fps: self.hz) } catch {
                        self.finish("""
                        capture failed: \(error)
                        Allow Screen Recording for the app that runs skyscreens (System Settings > Privacy & Security > Screen & System Audio Recording), then try again.
                        """)
                    }
                    try? await Task.sleep(nanoseconds: 500_000_000)
                    await MainActor.run { self.flip(remaining: 40) }
                }
            }
        }

        func waitForScreen(_ id: CGDirectDisplayID, tries: Int = 60, _ then: @escaping (NSScreen) -> Void) {
            if let s = NSScreen.screens.first(where: { ($0.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? CGDirectDisplayID) == id }) { then(s); return }
            guard tries > 0 else { finish("test screen never appeared in NSScreen") }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { self.waitForScreen(id, tries: tries - 1, then) }
        }

        func makeWindow(on screen: NSScreen) {
            let w = NSWindow(contentRect: screen.frame, styleMask: .borderless, backing: .buffered, defer: false)
            w.setFrame(screen.frame, display: false)
            w.level = .statusBar
            w.backgroundColor = .black
            let view = NSView(frame: NSRect(origin: .zero, size: screen.frame.size))
            view.wantsLayer = true
            view.layer!.backgroundColor = NSColor.black.cgColor
            // AppKit layers count from the bottom-left; the square is placed from the top-left.
            squareLayer.frame = CGRect(x: Probe.square.minX, y: screen.frame.height - Probe.square.maxY, width: Probe.square.width, height: Probe.square.height)
            squareLayer.backgroundColor = NSColor.black.cgColor
            bar.frame = CGRect(x: 0, y: 500, width: 40, height: 300)
            bar.backgroundColor = NSColor.systemBlue.cgColor
            view.layer!.addSublayer(squareLayer)
            view.layer!.addSublayer(bar)
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
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { self.flip(remaining: remaining - 1) }
        }

        func motionPass() {
            lock.withLock { counting = true }
            let start = CACurrentMediaTime()
            let timer = Timer(timeInterval: 1 / 480, repeats: true) { t in
                let x = (CACurrentMediaTime() - start) * 800
                CATransaction.begin(); CATransaction.setDisableActions(true)
                self.bar.frame.origin.x = CGFloat(x.truncatingRemainder(dividingBy: 1500))
                CATransaction.commit()
                if CACurrentMediaTime() - start > 3 { t.invalidate(); self.report() }
            }
            RunLoop.main.add(timer, forMode: .common)
        }

        func onFrame(_ f: ScreenCapture.Frame) {
            let isWhite = Self.pixelIsWhite(f.buffer, x: Int(Probe.square.midX), y: Int(Probe.square.midY))
            var flipTime: CFTimeInterval?
            lock.withLock {
                if counting { frameTimes.append(f.arrival) }
                if let p = pendingFlip, p.white == isWhite { flipTime = p.time; pendingFlip = nil }
            }
            guard let flipTime else { return }
            let area = f.dirty.reduce(0) { $0 + $1.width * $1.height } / Double(Probe.size.w * Probe.size.h)
            let rect = f.dirty.reduce(CGRect.null) { $0.union($1) }
            let t0 = CACurrentMediaTime()
            let tile = LosslessTile.encode(f.buffer, rect: rect.isNull ? Probe.square : rect)
            let tileTime = CACurrentMediaTime() - t0
            frameIndex += 1
            encoder?.encode(f.buffer, pts: CMTime(value: frameIndex, timescale: CMTimeScale(hz)), forceKey: frameIndex == 1) { [unowned self] e in
                lock.withLock { encodeMs.append(e.latency * 1000) }
            }
            lock.withLock {
                captureMs.append((f.arrival - flipTime) * 1000)
                tileMs.append(tileTime * 1000)
                tileBytes.append(tile?.count ?? 0)
                dirtyArea.append(area * 100)
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
            lock.lock(); defer { lock.unlock() }
            func p(_ a: [Double], _ q: Double) -> Double { let s = a.sorted(); return s.isEmpty ? .nan : s[min(s.count - 1, Int(q * Double(s.count)))] }
            let fps = frameTimes.count > 1 ? Double(frameTimes.count - 1) / (frameTimes.last! - frameTimes.first!) : 0
            finish(String(format: """
            virtual display %dx%d @ %.0f Hz, %d flips measured
              change -> captured frame:   p50 %5.1f  p99 %5.1f ms
              H.264 encode:               p50 %5.1f  p99 %5.1f ms
              lossless tile (LZ4):        p50 %5.2f  p99 %5.2f ms, %d bytes p50, dirty area %.1f%% of screen
              frames delivered in motion: %.1f fps
            """, Probe.size.w, Probe.size.h, hz, captureMs.count, p(captureMs, 0.5), p(captureMs, 0.99), p(encodeMs, 0.5), p(encodeMs, 0.99),
                          p(tileMs, 0.5), p(tileMs, 0.99), tileBytes.sorted()[safe: tileBytes.count / 2] ?? 0, p(dirtyArea, 0.5), fps))
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
