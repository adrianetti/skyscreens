import CoreMedia
import QuartzCore
import VideoToolbox

public final class VideoEncoder {
    public struct Frame {
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
        let status = VTCompressionSessionCreate(
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
        let props: [CFString: CFTypeRef] = [
            kVTCompressionPropertyKey_RealTime: kCFBooleanTrue,
            kVTCompressionPropertyKey_AllowFrameReordering: kCFBooleanFalse,
            kVTCompressionPropertyKey_ProfileLevel: kVTProfileLevel_H264_ConstrainedHigh_AutoLevel,
            kVTCompressionPropertyKey_AverageBitRate: NSNumber(value: bitrate),
            kVTCompressionPropertyKey_ExpectedFrameRate: NSNumber(value: fps),
            kVTCompressionPropertyKey_MaxKeyFrameInterval: NSNumber(value: Int32.max),
        ]
        for (k, v) in props { VTSessionSetProperty(created, key: k, value: v) }
        VTCompressionSessionPrepareToEncodeFrames(created)
    }

    deinit { VTCompressionSessionInvalidate(session) }

    public func encode(_ buffer: CVPixelBuffer, pts: CMTime, forceKey: Bool = false, done: @escaping (Frame) -> Void) {
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
        guard let block = CMSampleBufferGetDataBuffer(sample),
            let format = CMSampleBufferGetFormatDescription(sample)
        else { return nil }
        let attachments =
            CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: false) as? [[CFString: Any]]
        let isKey = !(attachments?.first?[kCMSampleAttachmentKey_NotSync] as? Bool ?? false)
        var out = Data()
        var lengthSize: Int32 = 4
        if isKey {
            appendParameterSets(of: format, to: &out, lengthSize: &lengthSize)
        }
        guard appendUnits(of: block, lengthSize: Int(lengthSize), to: &out) else { return nil }
        return (out, isKey)
    }

    static func appendParameterSets(of format: CMFormatDescription, to out: inout Data, lengthSize: inout Int32) {
        var count = 0
        CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
            format,
            parameterSetIndex: 0,
            parameterSetPointerOut: nil,
            parameterSetSizeOut: nil,
            parameterSetCountOut: &count,
            nalUnitHeaderLengthOut: &lengthSize
        )
        for i in 0..<count {
            var pointer: UnsafePointer<UInt8>?
            var size = 0
            CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
                format,
                parameterSetIndex: i,
                parameterSetPointerOut: &pointer,
                parameterSetSizeOut: &size,
                parameterSetCountOut: nil,
                nalUnitHeaderLengthOut: nil
            )
            if let pointer {
                out.append(contentsOf: startCode)
                out.append(pointer, count: size)
            }
        }
    }

    static func appendUnits(of block: CMBlockBuffer, lengthSize: Int, to out: inout Data) -> Bool {
        var total = 0
        var base: UnsafeMutablePointer<CChar>?
        let status = CMBlockBufferGetDataPointer(
            block,
            atOffset: 0,
            lengthAtOffsetOut: nil,
            totalLengthOut: &total,
            dataPointerOut: &base
        )
        guard status == noErr, let base else { return false }
        let bytes = UnsafeRawPointer(base).assumingMemoryBound(to: UInt8.self)
        return appendUnits(
            lengthPrefixed: UnsafeBufferPointer(start: bytes, count: total), lengthSize: lengthSize, to: &out)
    }

    static func appendUnits(lengthPrefixed bytes: UnsafeBufferPointer<UInt8>, lengthSize: Int, to out: inout Data)
        -> Bool
    {
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
