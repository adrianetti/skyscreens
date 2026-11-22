import CoreMedia
import ScreenCaptureKit

public final class ScreenCapture: NSObject, SCStreamOutput, SCStreamDelegate, @unchecked Sendable {
    public struct Frame {
        public let buffer: CVPixelBuffer
        public let dirtyRegions: [CGRect]
        public let arrivalTime: CFTimeInterval
    }

    public enum CaptureError: Error, CustomStringConvertible {
        case displayNotFound(CGDirectDisplayID)

        public var description: String {
            switch self {
            case .displayNotFound(let id): "display \(id) is not shareable"
            }
        }
    }

    private let lock = NSLock()
    private var frameHandler: (@Sendable (Frame) -> Void)?
    private var stopHandler: (@Sendable (any Error) -> Void)?
    private var activeStream: SCStream?

    public var onFrame: (@Sendable (Frame) -> Void)? {
        get { lock.withLock { frameHandler } }
        set { lock.withLock { frameHandler = newValue } }
    }

    public var onStop: (@Sendable (any Error) -> Void)? {
        get { lock.withLock { stopHandler } }
        set { lock.withLock { stopHandler = newValue } }
    }
    private let queue = DispatchQueue(label: "skyscreens.capture", qos: .userInteractive)

    override public init() {}

    public func start(displayID: CGDirectDisplayID, width: Int, height: Int, fps: Double) async throws {
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: false)
        guard let display = content.displays.first(where: { $0.displayID == displayID }) else {
            throw CaptureError.displayNotFound(displayID)
        }
        let stream = SCStream(
            filter: SCContentFilter(display: display, excludingWindows: []),
            configuration: Self.configuration(width: width, height: height, fps: fps),
            delegate: self
        )
        try stream.addStreamOutput(self, type: .screen, sampleHandlerQueue: queue)
        try await stream.startCapture()
        lock.withLock { activeStream = stream }
    }

    static func configuration(width: Int, height: Int, fps: Double) -> SCStreamConfiguration {
        let config = SCStreamConfiguration()
        config.width = width
        config.height = height
        config.pixelFormat = kCVPixelFormatType_32BGRA
        config.colorSpaceName = CGColorSpace.sRGB
        config.minimumFrameInterval = CMTime(value: 1, timescale: CMTimeScale(fps.rounded()))
        config.queueDepth = 3
        config.showsCursor = false
        return config
    }

    public func stop() async {
        let stream = lock.withLock { () -> SCStream? in
            defer { activeStream = nil }
            return activeStream
        }
        try? await stream?.stopCapture()
    }

    public func stream(_: SCStream, didOutputSampleBuffer sample: CMSampleBuffer, of type: SCStreamOutputType) {
        let arrival = CACurrentMediaTime()
        guard type == .screen,
            let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: false)
                as? [[SCStreamFrameInfo: Any]],
            let info = attachments.first,
            let status = info[.status] as? Int, SCFrameStatus(rawValue: status) == .complete,
            let buffer = sample.imageBuffer
        else { return }
        let dirty = (info[.dirtyRects] as? [[String: Any]])?.compactMap { rect in
            CGRect(dictionaryRepresentation: rect as CFDictionary)
        }
        onFrame?(Frame(buffer: buffer, dirtyRegions: dirty ?? [], arrivalTime: arrival))
    }

    public func stream(_: SCStream, didStopWithError error: any Error) { onStop?(error) }
}
