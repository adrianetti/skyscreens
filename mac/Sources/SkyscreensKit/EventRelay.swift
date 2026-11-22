import os

final class EventRelay: Sendable {
    private let target = OSAllocatedUnfairLock<(@Sendable (LinkEvent) -> Void)?>(initialState: nil)

    func connect(_ handler: @escaping @Sendable (LinkEvent) -> Void) {
        target.withLock { $0 = handler }
    }

    func forward(_ event: LinkEvent) {
        target.withLock { $0 }?(event)
    }
}
