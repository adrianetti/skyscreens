import Testing
import os

@testable import SkyscreensKit

struct EventRelayTests {
    @Test func eventsBeforeConnectingAreDropped() {
        let relay = EventRelay()
        relay.forward(.connected)
        let seen = OSAllocatedUnfairLock(initialState: [LinkEvent]())
        relay.connect { event in seen.withLock { $0.append(event) } }
        relay.forward(.pong(capturedMicros: 7))
        #expect(seen.withLock { $0 } == [.pong(capturedMicros: 7)])
    }

    @Test func reconnectingReplacesTheHandler() {
        let relay = EventRelay()
        let first = OSAllocatedUnfairLock(initialState: 0)
        let second = OSAllocatedUnfairLock(initialState: 0)
        relay.connect { _ in first.withLock { $0 += 1 } }
        relay.connect { _ in second.withLock { $0 += 1 } }
        relay.forward(.disconnected)
        #expect(first.withLock { $0 } == 0 && second.withLock { $0 } == 1)
    }
}
