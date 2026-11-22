import Foundation

struct StreamStats {
    var frames = 0
    var keyframes = 0
    var bytes = 0
    var encodeMs: [Double] = []
    var roundTripMs: [Double] = []

    mutating func add(_ other: Self) {
        frames += other.frames
        keyframes += other.keyframes
        bytes += other.bytes
        encodeMs += other.encodeMs
        roundTripMs += other.roundTripMs
    }

    static func percentile(_ values: [Double], _ quantile: Double) -> Double {
        let sorted = values.sorted()
        guard !sorted.isEmpty else { return .nan }
        return sorted[min(sorted.count - 1, Int(quantile * Double(sorted.count)))]
    }

    func line(connected: Bool, seconds: Double = 1) -> String {
        String(
            format: "%@  %2d fps  %5.1f Mbps  encode %4.1f ms  capture->decoded on Quest p50 %5.1f  p95 %5.1f ms",
            connected ? "connected   " : "waiting...  ",
            frames,
            Double(bytes) * 8 / 1e6 / seconds,
            Self.percentile(encodeMs, 0.5),
            Self.percentile(roundTripMs, 0.5),
            Self.percentile(roundTripMs, 0.95)
        )
    }

    func summaryJSON(seconds: Double, everConnected: Bool, connectedSeconds: Double = 0) -> String {
        func number(_ value: Double) -> String { value.isFinite ? String(format: "%.2f", value) : "null" }
        let fields = [
            "\"connected\":\(everConnected)",
            "\"seconds\":\(number(seconds))",
            "\"connected_seconds\":\(number(connectedSeconds))",
            "\"fps\":\(number(connectedSeconds > 0 ? Double(frames) / connectedSeconds : .nan))",
            "\"frames\":\(frames)",
            "\"keyframes\":\(keyframes)",
            "\"mbps\":\(number(seconds > 0 ? Double(bytes) * 8 / 1e6 / seconds : .nan))",
            "\"encode_p50_ms\":\(number(Self.percentile(encodeMs, 0.5)))",
            "\"round_trips\":\(roundTripMs.count)",
            "\"round_trip_p50_ms\":\(number(Self.percentile(roundTripMs, 0.5)))",
            "\"round_trip_p95_ms\":\(number(Self.percentile(roundTripMs, 0.95)))",
        ]
        return "summary " + "{" + fields.joined(separator: ",") + "}"
    }
}
