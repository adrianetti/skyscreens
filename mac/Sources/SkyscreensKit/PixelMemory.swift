import Compression
import CoreVideo
import Foundation

enum PixelMemory {
    static let bytesPerPixel = 4

    static func rows(of buffer: CVPixelBuffer, in region: CGRect) -> Data? {
        CVPixelBufferLockBaseAddress(buffer, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(buffer, .readOnly) }
        guard let base = unsafe CVPixelBufferGetBaseAddress(buffer) else { return nil }
        let rowBytes = CVPixelBufferGetBytesPerRow(buffer)
        let tileRow = Int(region.width) * bytesPerPixel
        var out = Data(capacity: tileRow * Int(region.height))
        for row in Int(region.minY)..<Int(region.maxY) {
            let start = unsafe base + row * rowBytes + Int(region.minX) * bytesPerPixel
            unsafe out.append(start.assumingMemoryBound(to: UInt8.self), count: tileRow)
        }
        return out
    }

    static func firstByte(of buffer: CVPixelBuffer, x: Int, y: Int) -> UInt8? {
        CVPixelBufferLockBaseAddress(buffer, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(buffer, .readOnly) }
        guard let base = unsafe CVPixelBufferGetBaseAddress(buffer) else { return nil }
        let offset = y * CVPixelBufferGetBytesPerRow(buffer) + x * bytesPerPixel
        return unsafe base.load(fromByteOffset: offset, as: UInt8.self)
    }

    static func makeBGRA(width: Int, height: Int, pixels: Data) -> CVPixelBuffer? {
        guard pixels.count == width * height * bytesPerPixel else { return nil }
        var created: CVPixelBuffer?
        let attributes = [kCVPixelBufferIOSurfacePropertiesKey: [:]] as CFDictionary
        unsafe CVPixelBufferCreate(nil, width, height, kCVPixelFormatType_32BGRA, attributes, &created)
        guard let buffer = created else { return nil }
        CVPixelBufferLockBaseAddress(buffer, [])
        defer { CVPixelBufferUnlockBaseAddress(buffer, []) }
        guard let base = unsafe CVPixelBufferGetBaseAddress(buffer) else { return nil }
        let rowBytes = CVPixelBufferGetBytesPerRow(buffer)
        let tileRow = width * bytesPerPixel
        for row in 0..<height {
            let source = pixels[pixels.startIndex + row * tileRow..<pixels.startIndex + (row + 1) * tileRow]
            unsafe source.copyBytes(to: (base + row * rowBytes).assumingMemoryBound(to: UInt8.self), count: tileRow)
        }
        return buffer
    }

    static func compressRawLZ4(_ input: Data) -> Data? {
        rawLZ4(input, capacity: input.count, operation: COMPRESSION_STREAM_ENCODE)
    }

    static func decompressRawLZ4(_ input: Data, expectedSize: Int) -> Data? {
        rawLZ4(input, capacity: expectedSize, operation: COMPRESSION_STREAM_DECODE)
    }

    private static func rawLZ4(_ input: Data, capacity: Int, operation: compression_stream_operation) -> Data? {
        var output = Data(count: capacity)
        let written = unsafe output.withUnsafeMutableBytes { destination -> Int in
            guard let target = unsafe destination.bindMemory(to: UInt8.self).baseAddress else { return 0 }
            return unsafe input.withUnsafeBytes { source -> Int in
                guard let origin = unsafe source.bindMemory(to: UInt8.self).baseAddress else { return 0 }
                return operation == COMPRESSION_STREAM_ENCODE
                    ? unsafe compression_encode_buffer(target, capacity, origin, source.count, nil, COMPRESSION_LZ4_RAW)
                    : unsafe compression_decode_buffer(target, capacity, origin, source.count, nil, COMPRESSION_LZ4_RAW)
            }
        }
        return written == 0 ? nil : output.prefix(written)
    }
}
