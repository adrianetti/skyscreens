import Compression
import CoreVideo
import Foundation

/// The lossless lane: a rectangle of BGRA pixels compressed with raw LZ4 blocks,
/// which the headset decodes with any standard LZ4 implementation.
public enum LosslessTile {
    /// Compresses `rect` (in pixels) of a BGRA buffer. Returns nil if LZ4 cannot shrink it.
    public static func encode(_ buffer: CVPixelBuffer, rect: CGRect) -> Data? {
        let r = rect.integral.intersection(CGRect(x: 0, y: 0, width: CVPixelBufferGetWidth(buffer), height: CVPixelBufferGetHeight(buffer)))
        guard !r.isEmpty else { return nil }
        let w = Int(r.width), h = Int(r.height), x0 = Int(r.minX), y0 = Int(r.minY)
        CVPixelBufferLockBaseAddress(buffer, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(buffer, .readOnly) }
        guard let base = CVPixelBufferGetBaseAddress(buffer) else { return nil }
        let rowBytes = CVPixelBufferGetBytesPerRow(buffer)
        let raw = UnsafeMutablePointer<UInt8>.allocate(capacity: w * h * 4)
        defer { raw.deallocate() }
        for y in 0..<h { memcpy(raw + y * w * 4, base + (y0 + y) * rowBytes + x0 * 4, w * 4) }
        let capacity = w * h * 4
        let dst = UnsafeMutablePointer<UInt8>.allocate(capacity: capacity)
        defer { dst.deallocate() }
        let n = compression_encode_buffer(dst, capacity, raw, w * h * 4, nil, COMPRESSION_LZ4_RAW)
        return n == 0 ? nil : Data(bytes: dst, count: n)
    }
}
