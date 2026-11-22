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
        case .configFailed(let error): "display configuration failed (\(error.rawValue))"
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
            let descriptor = CGVirtualDisplayDescriptor()
            descriptor.queue = DispatchQueue.main
            descriptor.name = spec.name
            descriptor.maxPixelsWide = UInt32(spec.width)
            descriptor.maxPixelsHigh = UInt32(spec.height)
            descriptor.sizeInMillimeters = Self.physicalSize(spec)
            descriptor.vendorID = Self.vendorID
            descriptor.productID = Self.productID
            descriptor.serialNum = UInt32(i + 1)
            guard let display = CGVirtualDisplay(descriptor: descriptor),
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
        unsafe CGBeginDisplayConfiguration(&config)
        unsafe CGConfigureDisplayMirrorOfDisplay(config, ids[index], target)
        let err = unsafe CGCompleteDisplayConfiguration(config, .forSession)
        if err != .success { throw DisplayError.configFailed(err) }
    }

    private static func settings(_ spec: ScreenSpec, refreshRate: Double) -> CGVirtualDisplaySettings {
        let settings = CGVirtualDisplaySettings()
        settings.hiDPI = 0
        settings.modes = [
            CGVirtualDisplayMode(width: UInt(spec.width), height: UInt(spec.height), refreshRate: refreshRate)
        ]
        return settings
    }

    static func builtInDisplay() -> CGDirectDisplayID {
        var count: UInt32 = 0
        unsafe CGGetOnlineDisplayList(0, nil, &count)
        var list = [CGDirectDisplayID](repeating: 0, count: Int(count))
        unsafe CGGetOnlineDisplayList(count, &list, &count)
        return list.first { CGDisplayIsBuiltin($0) != 0 } ?? CGMainDisplayID()
    }
}
