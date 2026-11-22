import Foundation
import Testing

@testable import SkyscreensKit

struct ProbeRecorderTests {
    @Test func summarizesSamplesIntoTheProbeReport() {
        var samples = ProbeRecorder.Samples()
        samples.captureMs = [10, 12, 14]
        samples.frameTimes = [0, 0.5, 1]
        let report = ProbeRecorder.summary(samples, hz: 60)
        #expect(report.contains("1600x1000 @ 60 Hz, 3 flips measured"))
        #expect(report.contains("frames delivered in motion: 2.0 fps"))
    }

    @Test func emptySamplesDoNotCrash() {
        #expect(ProbeRecorder.summary(ProbeRecorder.Samples(), hz: 60).contains("0 flips measured"))
    }
}
