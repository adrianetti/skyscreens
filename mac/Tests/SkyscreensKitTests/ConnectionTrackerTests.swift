import Foundation
import Testing

@testable import SkyscreensKit

struct ConnectionTrackerTests {
    @Test func reportsANewConnectionOnlyOnTheRisingEdge() {
        var tracker = ConnectionTracker()
        let edges = [false, true, true, false, true].map { tracker.observe(connected: $0, after: 0.1) }
        #expect(edges == [false, true, false, false, true])
    }

    @Test func countsOnlyConnectedTime() {
        var tracker = ConnectionTracker()
        for connected in [false, true, true, false, true] { _ = tracker.observe(connected: connected, after: 0.5) }
        #expect(tracker.connectedSeconds == 1.5)
        #expect(tracker.everConnected)
    }

    @Test func aHeadsetThatNeverConnectsIsReported() {
        var tracker = ConnectionTracker()
        for _ in 0..<10 { _ = tracker.observe(connected: false, after: 0.1) }
        #expect(!tracker.everConnected && tracker.connectedSeconds == 0)
    }
}
