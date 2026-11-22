import CoreMedia
import CoreVideo
import Foundation
import QuartzCore

final class ProbeRecorder: @unchecked Sendable {
    struct Samples {
        var captureMs: [Double] = []
        var encodeMs: [Double] = []
        var tileMs: [Double] = []
        var tileBytes: [Int] = []
        var dirtyPercent: [Double] = []
        var frameTimes: [CFTimeInterval] = []
    }

    static let whiteThreshold: UInt8 = 128
    static let bytesPerPixel = 4

    private let encoder: VideoEncoder
    private let hz: Double
    private let lock = NSLock()
    private var pendingFlip: (time: CFTimeInterval, white: Bool)?
    private var samples = Samples()
    private var counting = false
    private var frameIndex: Int64 = 0

    init(encoder: VideoEncoder, hz: Double) {
        self.encoder = encoder
        self.hz = hz
    }

    func flipped(toWhite white: Bool) {
        lock.withLock { pendingFlip = (CACurrentMediaTime(), white) }
    }

    func startCountingFrames() {
        lock.withLock { counting = true }
    }

    func record(_ frame: ScreenCapture.Frame) {
        let square = Probe.squareFromTopLeft
        let isWhite = Self.pixelIsWhite(frame.buffer, x: Int(square.midX), y: Int(square.midY))
        guard let flipTime = matchFlip(isWhite: isWhite, arrival: frame.arrivalTime) else { return }
        let screenArea = Double(Probe.size.width * Probe.size.height)
        let area = frame.dirtyRegions.reduce(0) { $0 + $1.width * $1.height } / screenArea
        let rect = frame.dirtyRegions.reduce(CGRect.null) { $0.union($1) }
        let tileStart = CACurrentMediaTime()
        let tile = LosslessTile.encode(frame.buffer, rect: rect.isNull ? square : rect)
        let tileTime = CACurrentMediaTime() - tileStart
        let index = lock.withLock { () -> Int64 in
            frameIndex += 1
            return frameIndex
        }
        let pts = CMTime(value: index, timescale: CMTimeScale(hz))
        encoder.encode(frame.buffer, pts: pts, forceKey: index == 1) { [weak self] encoded in
            self?.lock.withLock { self?.samples.encodeMs.append(encoded.encodeSeconds * 1000) }
        }
        lock.withLock {
            samples.captureMs.append((frame.arrivalTime - flipTime) * 1000)
            samples.tileMs.append(tileTime * 1000)
            samples.tileBytes.append(tile?.count ?? 0)
            samples.dirtyPercent.append(area * 100)
        }
    }

    private func matchFlip(isWhite: Bool, arrival: CFTimeInterval) -> CFTimeInterval? {
        lock.withLock {
            if counting { samples.frameTimes.append(arrival) }
            guard let pending = pendingFlip, pending.white == isWhite else { return nil }
            pendingFlip = nil
            return pending.time
        }
    }

    static func pixelIsWhite(_ buffer: CVPixelBuffer, x: Int, y: Int) -> Bool {
        CVPixelBufferLockBaseAddress(buffer, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(buffer, .readOnly) }
        guard let base = CVPixelBufferGetBaseAddress(buffer) else { return false }
        let offset = y * CVPixelBufferGetBytesPerRow(buffer) + x * bytesPerPixel
        return base.load(fromByteOffset: offset, as: UInt8.self) > whiteThreshold
    }

    func report() -> String {
        encoder.flush()
        return Self.summary(lock.withLock { samples }, hz: hz)
    }

    static func summary(_ samples: Samples, hz: Double) -> String {
        let pct = StreamStats.percentile
        var fps = 0.0
        if let first = samples.frameTimes.first, let last = samples.frameTimes.last, last > first {
            fps = Double(samples.frameTimes.count - 1) / (last - first)
        }
        let medianBytes = samples.tileBytes.sorted()[safe: samples.tileBytes.count / 2] ?? 0
        return String(
            format: """
                virtual display %dx%d @ %.0f Hz, %d flips measured
                  change -> captured frame:   p50 %5.1f  p99 %5.1f ms
                  H.264 encode:               p50 %5.1f  p99 %5.1f ms
                  lossless tile (LZ4):        p50 %5.2f  p99 %5.2f ms, %d bytes p50, dirty area %.1f%% of screen
                  frames delivered in motion: %.1f fps
                """,
            Probe.size.width,
            Probe.size.height,
            hz,
            samples.captureMs.count,
            pct(samples.captureMs, 0.5),
            pct(samples.captureMs, 0.99),
            pct(samples.encodeMs, 0.5),
            pct(samples.encodeMs, 0.99),
            pct(samples.tileMs, 0.5),
            pct(samples.tileMs, 0.99),
            medianBytes,
            pct(samples.dirtyPercent, 0.5),
            fps
        )
    }
}

extension Array {
    subscript(safe index: Int) -> Element? { indices.contains(index) ? self[index] : nil }
}
