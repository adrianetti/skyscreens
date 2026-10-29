import CoreMedia
import QuartzCore
import VideoToolbox

/// Hardware H.264 in VideoToolbox's low-latency mode, producing Annex B for MediaCodec.
public final class VideoEncoder {
    public struct Frame {
        /// Annex B NAL units; keyframes start with SPS and PPS.
        public let data: Data
        public let isKey: Bool
        public let pts: CMTime
        /// Encode time in seconds.
        public let latency: Double
    }

    public enum EncoderError: Error { case createFailed(OSStatus) }

    private let session: VTCompressionSession

    public init(width: Int, height: Int, fps: Double, bitrate: Int) throws {
        let spec: [CFString: Any] = [
            kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder: true,
            kVTVideoEncoderSpecification_EnableLowLatencyRateControl: true,
        ]
        var s: VTCompressionSession?
        let status = VTCompressionSessionCreate(allocator: nil, width: Int32(width), height: Int32(height), codecType: kCMVideoCodecType_H264,
                                                encoderSpecification: spec as CFDictionary, imageBufferAttributes: nil, compressedDataAllocator: nil,
                                                outputCallback: nil, refcon: nil, compressionSessionOut: &s)
        guard status == noErr, let s else { throw EncoderError.createFailed(status) }
        session = s
        let props: [CFString: CFTypeRef] = [
            kVTCompressionPropertyKey_RealTime: kCFBooleanTrue,
            kVTCompressionPropertyKey_AllowFrameReordering: kCFBooleanFalse,
            kVTCompressionPropertyKey_ProfileLevel: kVTProfileLevel_H264_ConstrainedHigh_AutoLevel,
            kVTCompressionPropertyKey_AverageBitRate: NSNumber(value: bitrate),
            kVTCompressionPropertyKey_ExpectedFrameRate: NSNumber(value: fps),
            kVTCompressionPropertyKey_MaxKeyFrameInterval: NSNumber(value: Int32.max),
        ]
        for (k, v) in props { VTSessionSetProperty(s, key: k, value: v) }
        VTCompressionSessionPrepareToEncodeFrames(s)
    }

    deinit { VTCompressionSessionInvalidate(session) }

    public func encode(_ buffer: CVPixelBuffer, pts: CMTime, forceKey: Bool = false, done: @escaping (Frame) -> Void) {
        let start = CACurrentMediaTime()
        let props = forceKey ? [kVTEncodeFrameOptionKey_ForceKeyFrame: true] as CFDictionary : nil
        VTCompressionSessionEncodeFrame(session, imageBuffer: buffer, presentationTimeStamp: pts, duration: .invalid,
                                        frameProperties: props, infoFlagsOut: nil) { status, _, sample in
            guard status == noErr, let sample, let (data, isKey) = Self.annexB(sample) else { return }
            done(Frame(data: data, isKey: isKey, pts: pts, latency: CACurrentMediaTime() - start))
        }
    }

    public func flush() { VTCompressionSessionCompleteFrames(session, untilPresentationTimeStamp: .invalid) }

    static let startCode: [UInt8] = [0, 0, 0, 1]

    /// Converts length-prefixed NAL units to Annex B, adding SPS and PPS to keyframes.
    static func annexB(_ sample: CMSampleBuffer) -> (Data, Bool)? {
        guard let block = CMSampleBufferGetDataBuffer(sample), let format = CMSampleBufferGetFormatDescription(sample) else { return nil }
        let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: false) as? [[CFString: Any]]
        let isKey = !(attachments?.first?[kCMSampleAttachmentKey_NotSync] as? Bool ?? false)
        var out = Data()
        var lengthSize: Int32 = 4
        if isKey {
            var count = 0
            CMVideoFormatDescriptionGetH264ParameterSetAtIndex(format, parameterSetIndex: 0, parameterSetPointerOut: nil, parameterSetSizeOut: nil,
                                                              parameterSetCountOut: &count, nalUnitHeaderLengthOut: &lengthSize)
            for i in 0..<count {
                var ptr: UnsafePointer<UInt8>?
                var size = 0
                CMVideoFormatDescriptionGetH264ParameterSetAtIndex(format, parameterSetIndex: i, parameterSetPointerOut: &ptr, parameterSetSizeOut: &size,
                                                                  parameterSetCountOut: nil, nalUnitHeaderLengthOut: nil)
                if let ptr { out.append(contentsOf: startCode); out.append(ptr, count: size) }
            }
        }
        var total = 0
        var base: UnsafeMutablePointer<CChar>?
        guard CMBlockBufferGetDataPointer(block, atOffset: 0, lengthAtOffsetOut: nil, totalLengthOut: &total, dataPointerOut: &base) == noErr, let base else { return nil }
        let bytes = UnsafeRawPointer(base).assumingMemoryBound(to: UInt8.self)
        var offset = 0
        let n = Int(lengthSize)
        while offset + n <= total {
            var length = 0
            for i in 0..<n { length = length << 8 | Int(bytes[offset + i]) }
            offset += n
            guard offset + length <= total else { return nil }
            out.append(contentsOf: startCode)
            out.append(bytes + offset, count: length)
            offset += length
        }
        return (out, isKey)
    }
}
