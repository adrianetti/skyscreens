import CVirtualDisplay
import CoreGraphics
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

public final class VirtualDisplaySet {
    public let specs: [ScreenSpec]
    private var displays: [CGVirtualDisplay] = []

    public var ids: [CGDirectDisplayID] { displays.map(\.displayID) }

    static let vendorID: UInt32 = 0x5353
    static let productID: UInt32 = 0x0001
    static let pixelsPerInch = 110.0
    static let millimetersPerInch = 25.4

    public init(_ specs: [ScreenSpec]) throws {
        self.specs = specs
        for (i, spec) in specs.enumerated() {
            let d = CGVirtualDisplayDescriptor()
            d.queue = DispatchQueue.main
            d.name = spec.name
            d.maxPixelsWide = UInt32(spec.width)
            d.maxPixelsHigh = UInt32(spec.height)
            d.sizeInMillimeters = Self.physicalSize(spec)
            d.vendorID = Self.vendorID
            d.productID = Self.productID
            d.serialNum = UInt32(i + 1)
            guard let display = CGVirtualDisplay(descriptor: d),
                display.apply(Self.settings(spec, refreshRate: spec.refreshRate))
            else {
                throw DisplayError.createFailed(spec.name)
            }
            displays.append(display)
        }
    }

    static func physicalSize(_ spec: ScreenSpec) -> CGSize {
        let mm = { (pixels: Int) in Double(pixels) / pixelsPerInch * millimetersPerInch }
        return CGSize(width: mm(spec.width), height: mm(spec.height))
    }

    public func waitUntilReady(timeout: TimeInterval = 5) throws {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if ids.allSatisfy({ CGDisplayCopyDisplayMode($0) != nil }) { return }
            RunLoop.current.run(until: Date().addingTimeInterval(0.05))
        }
        throw DisplayError.notReady(zip(specs, ids).filter { CGDisplayCopyDisplayMode($0.1) == nil }.map(\.0.name))
    }

    @discardableResult
    public func setRefreshRate(_ index: Int, _ hz: Double) -> Bool {
        displays[index].apply(Self.settings(specs[index], refreshRate: hz))
    }

    public func park(_ index: Int) throws { try mirror(index, onto: Self.builtInDisplay()) }

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
