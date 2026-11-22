import CoreGraphics
import Foundation
import Testing

@testable import SkyscreensKit

struct StreamStatsTests {
    @Test func percentilesOfNothingAreNaN() {
        #expect(StreamStats.percentile([], 0.5).isNaN)
    }

    @Test func percentilesPickFromTheSortedSamples() {
        let samples = (1...100).map(Double.init).shuffled()
        #expect(StreamStats.percentile(samples, 0.5) == 51)
        #expect(StreamStats.percentile(samples, 0.95) == 96)
        #expect(StreamStats.percentile(samples, 1) == 100)
    }

    @Test func summaryIsOneMachineReadableLine() throws {
        var stats = StreamStats()
        stats.add(StreamStats(frames: 3, keyframes: 1, bytes: 250_000, encodeMs: [2, 3, 4], roundTripMs: [10, 12]))
        let line = stats.summaryJSON(seconds: 2, everConnected: true, connectedSeconds: 1.5)
        #expect(line.hasPrefix("summary {"))
        let json = try JSONSerialization.jsonObject(with: Data(line.dropFirst("summary ".count).utf8)) as? [String: Any]
        let object = try #require(json)
        #expect(object["connected"] as? Bool == true)
        #expect(object["frames"] as? Int == 3)
        #expect(object["round_trips"] as? Int == 2)
        #expect(object["mbps"] as? Double == 1)
        #expect(object["fps"] as? Double == 2)
    }

    @Test func missingMeasurementsAreNull() throws {
        let line = StreamStats().summaryJSON(seconds: 1, everConnected: false)
        #expect(line.contains("\"round_trip_p50_ms\":null"))
    }
}

struct FixedFormatTests {
    @Test func formatsLikePrintfWithoutLocaleSurprises() {
        #expect(FixedFormat.decimal(7.94, places: 1, width: 5) == "  7.9")
        #expect(FixedFormat.decimal(12_345.678, places: 2) == "12345.68")
        #expect(FixedFormat.decimal(.nan, places: 1, width: 5) == "  nan")
        #expect(FixedFormat.integer(7, width: 2) == " 7")
    }

    @Test func theCaptureColorSpaceNameIsValid() {
        #expect(CGColorSpace(name: ScreenCapture.sRGBName as CFString) != nil)
    }
}

struct StreamLineTests {
    @Test func theLiveLineShowsRatesAndLatency() {
        let stats = StreamStats(frames: 60, keyframes: 1, bytes: 1_500_000, encodeMs: [6.1], roundTripMs: [18, 24])
        let line = stats.line(connected: true)
        #expect(line.hasPrefix("connected   60 fps   12.0 Mbps  encode  6.1 ms"))
        #expect(line.hasSuffix("p50  24.0  p95  24.0 ms"))
    }

    @Test func aWaitingStreamShowsNan() {
        #expect(StreamStats().line(connected: false).contains("waiting...   0 fps"))
        #expect(StreamStats().line(connected: false).contains("p50   nan"))
    }
}
