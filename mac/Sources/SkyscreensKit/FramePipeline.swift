import CoreMedia
import CoreVideo
import Foundation
import QuartzCore

final class FramePipeline: @unchecked Sendable {
    static let microsPerSecond = 1_000_000.0

    private let link: LinkHelper
    private let encoder: VideoEncoder
    private let fps: Double
    private let lock = NSLock()
    private var lastFrame: CVPixelBuffer?
    private var needKey = true
    private var frameIndex: Int64 = 0
    private var interval = StreamStats()
    private var total = StreamStats()

    init(link: LinkHelper, encoder: VideoEncoder, fps: Double) {
        self.link = link
        self.encoder = encoder
        self.fps = fps
    }

    func onCapture(_ buffer: CVPixelBuffer) {
        lock.withLock { lastFrame = buffer }
        send(buffer)
    }

    func resendKeyframe() {
        let frame = lock.withLock { () -> CVPixelBuffer? in
            needKey = true
            return lastFrame
        }
        if let frame { send(frame) }
    }

    func onLinkEvent(_ event: LinkEvent) {
        switch event {
        case .keyframeRequest:
            resendKeyframe()
        case .pong(let capturedMicros):
            let ms = (CACurrentMediaTime() * Self.microsPerSecond - Double(capturedMicros)) / 1000
            lock.withLock { interval.roundTripMs.append(ms) }
        case .connected, .disconnected, .listening:
            break
        }
    }

    func takeInterval() -> StreamStats {
        lock.withLock {
            let finished = interval
            total.add(finished)
            interval = StreamStats()
            return finished
        }
    }

    func totals() -> StreamStats {
        encoder.flush()
        _ = takeInterval()
        return lock.withLock { total }
    }

    private func send(_ buffer: CVPixelBuffer) {
        let (index, key) = lock.withLock { () -> (Int64, Bool) in
            frameIndex += 1
            defer { needKey = false }
            return (frameIndex, needKey)
        }
        let captured = UInt64(CACurrentMediaTime() * Self.microsPerSecond)
        let pts = CMTime(value: index, timescale: CMTimeScale(fps))
        encoder.encode(buffer, pts: pts, forceKey: key) { [weak self] frame in
            self?.deliver(frame, capturedMicros: captured)
        }
    }

    private func deliver(_ frame: VideoEncoder.Frame, capturedMicros: UInt64) {
        guard link.sendVideo(frame.annexB, isKey: frame.isKey, capturedMicros: capturedMicros) else { return }
        lock.withLock {
            interval.frames += 1
            interval.keyframes += frame.isKey ? 1 : 0
            interval.bytes += frame.annexB.count
            interval.encodeMs.append(frame.encodeSeconds * 1000)
        }
    }
}
