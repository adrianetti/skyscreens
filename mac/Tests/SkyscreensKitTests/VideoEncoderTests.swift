import CoreMedia
import CoreVideo
import Foundation
import Testing
import os

@testable import SkyscreensKit

func makeBGRABuffer(width: Int, height: Int, fill: (Int, Int) -> [UInt8]) -> CVPixelBuffer? {
    var buffer: CVPixelBuffer?
    let attrs = [kCVPixelBufferIOSurfacePropertiesKey: [:]] as CFDictionary
    CVPixelBufferCreate(nil, width, height, kCVPixelFormatType_32BGRA, attrs, &buffer)
    guard let buffer else { return nil }
    CVPixelBufferLockBaseAddress(buffer, [])
    defer { CVPixelBufferUnlockBaseAddress(buffer, []) }
    guard let base = CVPixelBufferGetBaseAddress(buffer) else { return nil }
    let rowBytes = CVPixelBufferGetBytesPerRow(buffer)
    for y in 0..<height {
        for x in 0..<width {
            let pixel = fill(x, y)
            for channel in 0..<4 {
                base.storeBytes(of: pixel[channel], toByteOffset: y * rowBytes + x * 4 + channel, as: UInt8.self)
            }
        }
    }
    return buffer
}

let hardwareEncoderAvailable = (try? VideoEncoder(width: 256, height: 160, fps: 60, bitrate: 2_000_000)) != nil

@Suite(.enabled(if: hardwareEncoderAvailable, "needs the Mac's hardware H.264 encoder"))
struct VideoEncoderTests {
    func encode(frames: [(CVPixelBuffer, Bool)]) throws -> [VideoEncoder.Frame] {
        let encoder = try VideoEncoder(width: 256, height: 160, fps: 60, bitrate: 2_000_000)
        let collected = OSAllocatedUnfairLock(initialState: [VideoEncoder.Frame]())
        for (i, (buffer, forceKey)) in frames.enumerated() {
            encoder.encode(buffer, pts: CMTime(value: Int64(i + 1), timescale: 60), forceKey: forceKey) { frame in
                collected.withLock { $0.append(frame) }
            }
        }
        encoder.flush()
        return collected.withLock { $0 }
    }

    func frame(_ shade: UInt8) throws -> CVPixelBuffer {
        try #require(makeBGRABuffer(width: 256, height: 160) { x, y in [UInt8(x % 256), UInt8(y % 256), shade, 255] })
    }

    @Test func firstFrameIsADecodableKeyframe() throws {
        let out = try encode(frames: [(try frame(10), true)])
        let first = try #require(out.first)
        #expect(first.isKey)
        let types = nalUnitTypes(first.annexB)
        #expect(Array(types.prefix(2)) == [7, 8], "SPS and PPS must lead a keyframe, got \(types)")
        #expect(types.contains(5), "keyframe needs an IDR slice, got \(types)")
    }

    @Test func laterFramesAreDeltasWithoutParameterSets() throws {
        let out = try encode(frames: [(try frame(10), true), (try frame(60), false), (try frame(90), false)])
        #expect(out.count == 3)
        for frame in out.dropFirst() {
            #expect(!frame.isKey)
            #expect(!nalUnitTypes(frame.annexB).contains(7))
        }
    }

    @Test func forcedKeyframesCarryParameterSetsAgain() throws {
        let out = try encode(frames: [(try frame(10), true), (try frame(60), false), (try frame(90), true)])
        let last = try #require(out.last)
        #expect(last.isKey && nalUnitTypes(last.annexB).prefix(2) == [7, 8])
    }

    @Test func framesComeBackInOrderWithTheirTimestamps() throws {
        let frames = try (0..<5).map { (try frame(UInt8($0 * 40)), $0 == 0) }
        let out = try encode(frames: frames)
        #expect(out.map(\.pts.value) == [1, 2, 3, 4, 5])
    }
}
