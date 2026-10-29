import CoreGraphics
import CVirtualDisplay
import Foundation

public enum DisplayError: Error, CustomStringConvertible {
    case createFailed(String)
    case notReady([String])
    case configFailed(CGError)

    public var description: String {
        switch self {
        case .createFailed(let name): "could not create display \(name)"
        case .notReady(let names): "displays never got a mode: \(names.joined(separator: ", "))"
        case .configFailed(let e): "display configuration failed (\(e.rawValue))"
        }
    }
}

/// A set of virtual displays that lives as long as its process.
///
/// macOS hands out dead displays (no mode) when a display is created after another was
/// destroyed in the same process, so all displays are created once, up front. Screens are
/// hidden and shown again by mirroring them onto the built-in display (`park`), and the
/// whole set goes away when the process exits.
public final class VirtualDisplaySet {
    public let specs: [ScreenSpec]
    private var displays: [CGVirtualDisplay] = []

    public var ids: [CGDirectDisplayID] { displays.map(\.displayID) }

    /// Fixed vendor and product IDs, so macOS remembers the arrangement between sessions.
    static let vendorID: UInt32 = 0x5353
    static let productID: UInt32 = 0x0001

    public init(_ specs: [ScreenSpec]) throws {
        self.specs = specs
        for (i, spec) in specs.enumerated() {
            let d = CGVirtualDisplayDescriptor()
            d.queue = DispatchQueue.main
            d.name = spec.name
            d.maxPixelsWide = UInt32(spec.width)
            d.maxPixelsHigh = UInt32(spec.height)
            // About 110 ppi, a typical non-Retina desktop monitor.
            d.sizeInMillimeters = CGSize(width: Double(spec.width) / 110 * 25.4, height: Double(spec.height) / 110 * 25.4)
            d.vendorID = Self.vendorID
            d.productID = Self.productID
            d.serialNum = UInt32(i + 1)
            guard let display = CGVirtualDisplay(descriptor: d), display.apply(Self.settings(spec, refreshRate: spec.refreshRate)) else {
                throw DisplayError.createFailed(spec.name)
            }
            displays.append(display)
        }
    }

    /// Spins the run loop until every display reports a mode.
    public func waitUntilReady(timeout: TimeInterval = 5) throws {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if ids.allSatisfy({ CGDisplayCopyDisplayMode($0) != nil }) { return }
            RunLoop.current.run(until: Date().addingTimeInterval(0.05))
        }
        throw DisplayError.notReady(zip(specs, ids).filter { CGDisplayCopyDisplayMode($0.1) == nil }.map(\.0.name))
    }

    /// Changes a screen's refresh rate in place, without recreating it.
    @discardableResult
    public func setRefreshRate(_ index: Int, _ hz: Double) -> Bool {
        displays[index].apply(Self.settings(specs[index], refreshRate: hz))
    }

    /// Removes a screen from the desktop by mirroring it onto the built-in display.
    public func park(_ index: Int) throws { try mirror(index, onto: Self.builtInDisplay()) }

    /// Puts a parked screen back on the desktop.
    public func unpark(_ index: Int) throws { try mirror(index, onto: kCGNullDirectDisplay) }

    private func mirror(_ index: Int, onto target: CGDirectDisplayID) throws {
        var config: CGDisplayConfigRef?
        CGBeginDisplayConfiguration(&config)
        CGConfigureDisplayMirrorOfDisplay(config, ids[index], target)
        let err = CGCompleteDisplayConfiguration(config, .forSession)
        if err != .success { throw DisplayError.configFailed(err) }
    }

    private static func settings(_ spec: ScreenSpec, refreshRate: Double) -> CGVirtualDisplaySettings {
        let s = CGVirtualDisplaySettings()
        s.hiDPI = 0
        s.modes = [CGVirtualDisplayMode(width: UInt(spec.width), height: UInt(spec.height), refreshRate: refreshRate)]
        return s
    }

    static func builtInDisplay() -> CGDirectDisplayID {
        var count: UInt32 = 0
        CGGetOnlineDisplayList(0, nil, &count)
        var list = [CGDirectDisplayID](repeating: 0, count: Int(count))
        CGGetOnlineDisplayList(count, &list, &count)
        return list.first { CGDisplayIsBuiltin($0) != 0 } ?? CGMainDisplayID()
    }
}
