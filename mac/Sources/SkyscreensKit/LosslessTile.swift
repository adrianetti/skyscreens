import Compression
import CoreVideo
import Foundation

public enum LosslessTile {
    static let bytesPerPixel = 4

    public static func encode(_ buffer: CVPixelBuffer, rect: CGRect) -> Data? {
        let bounds = CGRect(x: 0, y: 0, width: CVPixelBufferGetWidth(buffer), height: CVPixelBufferGetHeight(buffer))
        let region = rect.integral.intersection(bounds)
        guard !region.isEmpty, let raw = pixels(of: buffer, in: region) else { return nil }
        let dst = UnsafeMutablePointer<UInt8>.allocate(capacity: raw.count)
        defer { dst.deallocate() }
        let n = raw.withUnsafeBytes { src -> Int in
            guard let src = src.bindMemory(to: UInt8.self).baseAddress else { return 0 }
            return compression_encode_buffer(dst, raw.count, src, raw.count, nil, COMPRESSION_LZ4_RAW)
        }
        return n == 0 ? nil : Data(bytes: dst, count: n)
    }

    static func pixels(of buffer: CVPixelBuffer, in region: CGRect) -> Data? {
        CVPixelBufferLockBaseAddress(buffer, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(buffer, .readOnly) }
        guard let base = CVPixelBufferGetBaseAddress(buffer) else { return nil }
        let rowBytes = CVPixelBufferGetBytesPerRow(buffer)
        let (w, h, x0, y0) = (Int(region.width), Int(region.height), Int(region.minX), Int(region.minY))
        let tileRow = w * bytesPerPixel
        var out = Data(count: tileRow * h)
        out.withUnsafeMutableBytes { dst in
            guard let dst = dst.baseAddress else { return }
            for y in 0..<h {
                memcpy(dst + y * tileRow, base + (y0 + y) * rowBytes + x0 * bytesPerPixel, tileRow)
            }
        }
        return out
    }
}
