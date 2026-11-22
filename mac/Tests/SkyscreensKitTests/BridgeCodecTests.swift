import Foundation
import Testing

@testable import SkyscreensKit

struct BridgeVectors: Decodable {
    struct Entry: Decodable {
        let name: String
        let hex: String
    }
    let records: [Entry]
    let events: [Entry]

    static func load() throws -> Self {
        let tests = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
        let package = tests.deletingLastPathComponent().deletingLastPathComponent()
        let url = package.appendingPathComponent("../crates/link-helper/tests/vectors.json").standardized
        return try JSONDecoder().decode(Self.self, from: Data(contentsOf: url))
    }

    func record(_ name: String) throws -> String { try #require(records.first { $0.name == name }).hex }
    func event(_ name: String) throws -> String { try #require(events.first { $0.name == name }).hex }
}

func hex(_ data: Data) -> String {
    data.map { byte in FixedFormat.padded(String(byte, radix: 16), width: 2).replacingOccurrences(of: " ", with: "0") }
        .joined()
}

func bytes(_ hex: String) -> Data {
    Data(
        stride(from: 0, to: hex.count, by: 2).compactMap { offset in
            let start = hex.index(hex.startIndex, offsetBy: offset)
            return UInt8(hex[start..<hex.index(start, offsetBy: 2)], radix: 16)
        })
}

struct BridgeCodecTests {
    @Test func screenRecordsMatchTheRustHelper() throws {
        let spec = ScreenSpec(name: "code", role: .code, width: 1616, height: 1008, refreshRate: 60)
        let expected = try BridgeVectors.load().record("one code screen, control priority")
        #expect(hex(BridgeCodec.screensRecord([spec])) == expected)
    }

    @Test func videoRecordsMatchTheRustHelper() throws {
        let record = BridgeCodec.videoRecord(screen: 0, isKey: true, capturedMicros: 258, annexB: Data([0, 0, 0, 1]))
        #expect(hex(record) == (try BridgeVectors.load().record("keyframe video, focused priority")))
    }

    @Test func eventsFromTheRustHelperDecode() throws {
        let vectors = try BridgeVectors.load()
        let cases: [(String, LinkEvent)] = [
            ("keyframe request for screen 2", .keyframeRequest(screen: 2)),
            ("pong", .pong(capturedMicros: 4242)),
            ("connected", .connected),
            ("disconnected", .disconnected),
            ("listening on 7243", .listening(port: 7243)),
        ]
        for (name, event) in cases {
            #expect(BridgeCodec.event([UInt8](bytes(try vectors.event(name)))) == event, "\(name)")
        }
    }

    @Test func eventsSplitAcrossReadsReassemble() throws {
        let vectors = try BridgeVectors.load()
        let stream = bytes(try vectors.event("connected")) + bytes(try vectors.event("pong"))
        var events = EventStream()
        var decoded: [LinkEvent] = []
        for chunk in [stream.prefix(3), stream.dropFirst(3).prefix(11), stream.dropFirst(14)] {
            decoded += events.append(Data(chunk))
        }
        #expect(decoded == [.connected, .pong(capturedMicros: 4242)])
    }

    @Test func unknownEventsAreSkipped() {
        var events = EventStream()
        #expect(events.append(Data(repeating: 9, count: BridgeCodec.eventSize)).isEmpty)
    }

    @Test func littleEndianEncodingIsExplicit() {
        var data = Data()
        data.appendLittleEndian(UInt32(0x0102_0304))
        #expect([UInt8](data) == [4, 3, 2, 1])
    }
}
