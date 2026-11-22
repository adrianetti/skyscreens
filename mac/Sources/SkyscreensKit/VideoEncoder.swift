import CoreMedia
import QuartzCore
import VideoToolbox

public final class VideoEncoder: @unchecked Sendable {
    public struct Frame: Sendable {
        public let annexB: Data
        public let isKey: Bool
        public let pts: CMTime
        public let encodeSeconds: Double
    }

    public enum EncoderError: Error, CustomStringConvertible {
        case createFailed(OSStatus)

        public var description: String {
            switch self {
            case .createFailed(let status): "hardware H.264 encoder unavailable (\(status))"
            }
        }
    }

    static let startCode: [UInt8] = [0, 0, 0, 1]

    private let session: VTCompressionSession

    public init(width: Int, height: Int, fps: Double, bitrate: Int) throws {
        let spec: [CFString: Any] = [
            kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder: true,
            kVTVideoEncoderSpecification_EnableLowLatencyRateControl: true,
        ]
        var created: VTCompressionSession?
        let status = unsafe VTCompressionSessionCreate(
            allocator: nil,
            width: Int32(width),
            height: Int32(height),
            codecType: kCMVideoCodecType_H264,
            encoderSpecification: spec as CFDictionary,
            imageBufferAttributes: nil,
            compressedDataAllocator: nil,
            outputCallback: nil,
            refcon: nil,
            compressionSessionOut: &created
        )
        guard status == noErr, let created else { throw EncoderError.createFailed(status) }
        session = created
        let props: [CFString: Any] = [
            kVTCompressionPropertyKey_RealTime: true,
            kVTCompressionPropertyKey_AllowFrameReordering: false,
            kVTCompressionPropertyKey_ProfileLevel: kVTProfileLevel_H264_ConstrainedHigh_AutoLevel,
            kVTCompressionPropertyKey_AverageBitRate: bitrate,
            kVTCompressionPropertyKey_ExpectedFrameRate: fps,
            kVTCompressionPropertyKey_MaxKeyFrameInterval: Int32.max,
        ]
        VTSessionSetProperties(created, propertyDictionary: props as CFDictionary)
        VTCompressionSessionPrepareToEncodeFrames(created)
    }

    deinit { VTCompressionSessionInvalidate(session) }

    public func encode(
        _ buffer: CVPixelBuffer, pts: CMTime, forceKey: Bool = false, done: @escaping @Sendable (Frame) -> Void
    ) {
        let start = CACurrentMediaTime()
        let props = forceKey ? [kVTEncodeFrameOptionKey_ForceKeyFrame: true] as CFDictionary : nil
        VTCompressionSessionEncodeFrame(
            session,
            imageBuffer: buffer,
            presentationTimeStamp: pts,
            duration: .invalid,
            frameProperties: props,
            infoFlagsOut: nil
        ) { status, _, sample in
            guard status == noErr, let sample, let (data, isKey) = Self.annexB(sample) else { return }
            done(Frame(annexB: data, isKey: isKey, pts: pts, encodeSeconds: CACurrentMediaTime() - start))
        }
    }

    public func flush() { VTCompressionSessionCompleteFrames(session, untilPresentationTimeStamp: .invalid) }

    static func annexB(_ sample: CMSampleBuffer) -> (Data, Bool)? {
        guard let block = sample.dataBuffer, let format = sample.formatDescription else { return nil }
        let payload: Data
        do {
            payload = try block.dataBytes()
        } catch {
            return nil
        }
        let attachments =
            CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: false) as? [[CFString: Any]]
        let isKey = !(attachments?.first?[kCMSampleAttachmentKey_NotSync] as? Bool ?? false)
        var out = Data()
        if isKey {
            for parameterSet in format.parameterSets {
                out.append(contentsOf: startCode)
                out.append(parameterSet)
            }
        }
        let lengthSize = format.nalUnitHeaderLength ?? startCode.count
        guard appendUnits(lengthPrefixed: [UInt8](payload), lengthSize: lengthSize, to: &out) else { return nil }
        return (out, isKey)
    }

    static func appendUnits(lengthPrefixed bytes: [UInt8], lengthSize: Int, to out: inout Data) -> Bool {
        var offset = 0
        while offset + lengthSize <= bytes.count {
            let length = bytes[offset..<offset + lengthSize].reduce(0) { $0 << 8 | Int($1) }
            offset += lengthSize
            guard offset + length <= bytes.count else { return false }
            out.append(contentsOf: startCode)
            out.append(contentsOf: bytes[offset..<offset + length])
            offset += length
        }
        return true
    }
}
