import Compression
import CoreVideo
import Foundation
import Testing

@testable import SkyscreensKit

struct LosslessTileTests {
    func decompress(_ data: Data, size: Int) -> [UInt8] {
        var out = [UInt8](repeating: 0, count: size)
        let n = data.withUnsafeBytes { src in
            compression_decode_buffer(
                &out,
                size,
                src.bindMemory(to: UInt8.self).baseAddress.unsafelyUnwrapped,
                data.count,
                nil,
                COMPRESSION_LZ4_RAW
            )
        }
        return Array(out.prefix(n))
    }

    func textLikeScreen() throws -> CVPixelBuffer {
        try #require(
            makeBGRABuffer(width: 64, height: 48) { x, y in
                (x / 3 + y / 5) % 4 == 0 ? [20, 20, 20, 255] : [x < 32 ? 250 : 200, 248, 245, 255]
            }
        )
    }

    @Test func tilesDecompressToTheExactPixels() throws {
        let buffer = try textLikeScreen()
        let rect = CGRect(x: 8, y: 4, width: 32, height: 16)
        let tile = try #require(LosslessTile.encode(buffer, rect: rect))
        let expected = try #require(LosslessTile.pixels(of: buffer, in: rect))
        #expect(decompress(tile, size: expected.count) == [UInt8](expected))
        #expect(tile.count < expected.count)
    }

    @Test func rectsAreClippedToTheBuffer() throws {
        let buffer = try textLikeScreen()
        let tile = try #require(LosslessTile.encode(buffer, rect: CGRect(x: 48, y: 40, width: 100, height: 100)))
        let expected = try #require(LosslessTile.pixels(of: buffer, in: CGRect(x: 48, y: 40, width: 16, height: 8)))
        #expect(decompress(tile, size: expected.count) == [UInt8](expected))
    }

    @Test func emptyOrOutsideRectsGiveNothing() throws {
        let buffer = try textLikeScreen()
        #expect(LosslessTile.encode(buffer, rect: .zero) == nil)
        #expect(LosslessTile.encode(buffer, rect: CGRect(x: 500, y: 500, width: 10, height: 10)) == nil)
    }

    @Test func incompressibleNoiseIsLeftToTheVideoLane() throws {
        var rng = SystemRandomNumberGenerator()
        let noise = try #require(
            makeBGRABuffer(width: 32, height: 32) { _, _ in (0..<4).map { _ in UInt8.random(in: 0...255, using: &rng) }
            })
        #expect(LosslessTile.encode(noise, rect: CGRect(x: 0, y: 0, width: 32, height: 32)) == nil)
    }
}
