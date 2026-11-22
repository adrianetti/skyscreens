import Foundation

struct ConnectionTracker: Equatable {
    private(set) var isConnected = false
    private(set) var everConnected = false
    private(set) var connectedSeconds = 0.0

    mutating func observe(connected: Bool, after elapsed: TimeInterval) -> Bool {
        let isNewConnection = connected && !isConnected
        isConnected = connected
        everConnected = everConnected || connected
        if connected { connectedSeconds += elapsed }
        return isNewConnection
    }
}
