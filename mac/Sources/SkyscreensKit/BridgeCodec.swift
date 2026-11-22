import Foundation

enum LinkPriority: UInt8, Sendable {
    case control = 0
    case cursor = 1
    case focused = 2
    case video = 3
    case background = 4
    case refine = 5
}

enum LinkEvent: Equatable, Sendable {
    case keyframeRequest(screen: UInt8)
    case pong(capturedMicros: UInt64)
    case connected
    case disconnected
    case listening(port: UInt16)
}

enum BridgeCodec {
    static let eventSize = 10
    static let screensTag: UInt8 = 2
    static let videoTag: UInt8 = 3

    static func screensRecord(_ specs: [ScreenSpec]) -> Data {
        var body = Data([screensTag, UInt8(clamping: specs.count)])
        for (index, spec) in specs.enumerated() {
            body.append(UInt8(clamping: index))
            body.append(spec.role.wireValue)
            body.appendLittleEndian(UInt16(clamping: spec.width))
            body.appendLittleEndian(UInt16(clamping: spec.height))
        }
        return record(priority: .control, body: body)
    }

    static func videoRecord(screen: UInt8, isKey: Bool, capturedMicros: UInt64, annexB: Data) -> Data {
        var body = Data([videoTag, screen, isKey ? 1 : 0])
        body.appendLittleEndian(capturedMicros)
        body.append(annexB)
        return record(priority: .focused, body: body)
    }

    static func record(priority: LinkPriority, body: Data) -> Data {
        var out = Data()
        out.appendLittleEndian(UInt32(clamping: body.count + 1))
        out.append(priority.rawValue)
        out.append(body)
        return out
    }

    static func event(_ bytes: [UInt8]) -> LinkEvent? {
        guard bytes.count == eventSize else { return nil }
        let value = bytes[2...].reversed().reduce(UInt64(0)) { $0 << 8 | UInt64($1) }
        switch bytes[0] {
        case 1: return .keyframeRequest(screen: bytes[1])
        case 2: return .pong(capturedMicros: value)
        case 3: return .connected
        case 4: return .disconnected
        case 5: return .listening(port: UInt16(clamping: value))
        default: return nil
        }
    }
}

struct EventStream {
    private var pending: [UInt8] = []

    mutating func append(_ data: Data) -> [LinkEvent] {
        pending.append(contentsOf: data)
        let complete = pending.count - pending.count % BridgeCodec.eventSize
        let events = stride(from: 0, to: complete, by: BridgeCodec.eventSize).compactMap { start in
            BridgeCodec.event(Array(pending[start..<start + BridgeCodec.eventSize]))
        }
        pending.removeFirst(complete)
        return events
    }
}

extension Role {
    var wireValue: UInt8 {
        switch self {
        case .code: 0
        case .chat: 1
        case .email: 2
        case .video: 3
        }
    }
}

extension Data {
    mutating func appendLittleEndian<T: FixedWidthInteger>(_ value: T) {
        for byte in 0..<(T.bitWidth / 8) {
            append(UInt8(truncatingIfNeeded: value >> (byte * 8)))
        }
    }
}
