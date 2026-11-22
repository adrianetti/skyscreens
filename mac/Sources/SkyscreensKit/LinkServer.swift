import CLink
import Foundation

struct LinkServer: @unchecked Sendable {
    let handle: OpaquePointer

    static func start(port: UInt16) -> Self? {
        sk_server_start(port).map(Self.init(handle:))
    }

    var isConnected: Bool { sk_server_connected(handle) }

    func announce(_ spec: ScreenSpec) {
        var ids: [UInt8] = [0]
        var roles: [UInt8] = [0]
        var widths = [UInt16(clamping: spec.width)]
        var heights = [UInt16(clamping: spec.height)]
        sk_set_screens(handle, 1, &ids, &roles, &widths, &heights)
    }

    func sendVideo(_ annexB: Data, isKey: Bool, capturedMicros: UInt64) -> Bool {
        annexB.withUnsafeBytes { raw in
            sk_send_video(
                handle,
                0,
                isKey,
                capturedMicros,
                raw.bindMemory(to: UInt8.self).baseAddress,
                raw.count,
                UInt8(SK_PRIORITY_FOCUSED)
            )
        }
    }

    func deliverEvents(to pipeline: FramePipeline) {
        sk_server_on_message(
            handle,
            { context, kind, _, value in
                guard let context else { return }
                Unmanaged<FramePipeline>.fromOpaque(context).takeUnretainedValue().onLinkEvent(kind: kind, value: value)
            },
            Unmanaged.passRetained(pipeline).toOpaque()
        )
    }
}
