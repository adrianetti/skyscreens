import CoreMedia
import ScreenCaptureKit

/// Captures one display with ScreenCaptureKit. Frames arrive only when something changed.
public final class ScreenCapture: NSObject, SCStreamOutput, SCStreamDelegate {
    public struct Frame {
        public let buffer: CVPixelBuffer
        /// Changed regions, in output pixels.
        public let dirty: [CGRect]
        /// When the frame reached this process (CACurrentMediaTime).
        public let arrival: CFTimeInterval
    }

    public enum CaptureError: Error, CustomStringConvertible {
        case displayNotFound(CGDirectDisplayID)
        public var description: String { if case .displayNotFound(let id) = self { "display \(id) is not shareable" } else { "" } }
    }

    public var onFrame: ((Frame) -> Void)?
    public var onStop: ((Error) -> Void)?
    private var stream: SCStream?
    private let queue = DispatchQueue(label: "skyscreens.capture", qos: .userInteractive)

    public override init() {}

    /// Captures BGRA, so small changes can be sent losslessly. The cursor is left out:
    /// the headset draws it.
    public func start(displayID: CGDirectDisplayID, width: Int, height: Int, fps: Double) async throws {
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: false)
        guard let display = content.displays.first(where: { $0.displayID == displayID }) else { throw CaptureError.displayNotFound(displayID) }
        let config = SCStreamConfiguration()
        config.width = width
        config.height = height
        config.pixelFormat = kCVPixelFormatType_32BGRA
        config.colorSpaceName = CGColorSpace.sRGB
        config.minimumFrameInterval = CMTime(value: 1, timescale: CMTimeScale(fps.rounded()))
        config.queueDepth = 3
        config.showsCursor = false
        let stream = SCStream(filter: SCContentFilter(display: display, excludingWindows: []), configuration: config, delegate: self)
        try stream.addStreamOutput(self, type: .screen, sampleHandlerQueue: queue)
        try await stream.startCapture()
        self.stream = stream
    }

    public func stop() async { try? await stream?.stopCapture(); stream = nil }

    public func stream(_ stream: SCStream, didOutputSampleBuffer sample: CMSampleBuffer, of type: SCStreamOutputType) {
        let arrival = CACurrentMediaTime()
        guard type == .screen,
              let info = (CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]])?.first,
              let status = info[.status] as? Int, SCFrameStatus(rawValue: status) == .complete,
              let buffer = sample.imageBuffer else { return }
        let dirty = (info[.dirtyRects] as? [NSDictionary])?.compactMap { CGRect(dictionaryRepresentation: $0 as CFDictionary) } ?? []
        onFrame?(Frame(buffer: buffer, dirty: dirty, arrival: arrival))
    }

    public func stream(_ stream: SCStream, didStopWithError error: Error) { onStop?(error) }
}
