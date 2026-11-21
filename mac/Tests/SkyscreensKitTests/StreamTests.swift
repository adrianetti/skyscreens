import Foundation
import Testing

@testable import SkyscreensKit

struct StreamOptionsTests {
    @Test func defaultsStreamForeverToTheHeadset() throws {
        #expect(try StreamOptions(arguments: []) == StreamOptions(seconds: nil, launchHeadset: true))
    }

    @Test func parsesEveryOption() throws {
        let options = try StreamOptions(arguments: ["--seconds", "12.5", "--no-headset", "--test-pattern"])
        #expect(options == StreamOptions(seconds: 12.5, launchHeadset: false, testPattern: true))
    }

    @Test(arguments: [["--seconds"], ["--seconds", "zero"], ["--seconds", "-1"], ["--seconds", "0"], ["--bogus"]])
    func rejectsBadArguments(arguments: [String]) {
        #expect(throws: StreamOptions.ParseError.self) { try StreamOptions(arguments: arguments) }
    }
}

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
        let line = stats.summaryJSON(seconds: 2, connectedSeconds: 1.5, everConnected: true)
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
