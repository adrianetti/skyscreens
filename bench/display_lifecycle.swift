import Foundation
import CoreGraphics

func make(_ i: Int, w: Int, h: Int, hz: Double, hiDPI: Bool) -> CGVirtualDisplay? {
    let d = CGVirtualDisplayDescriptor()
    d.queue = DispatchQueue.main
    d.name = "skyscreens test \(i)"
    d.maxPixelsWide = UInt32(hiDPI ? w * 2 : w); d.maxPixelsHigh = UInt32(hiDPI ? h * 2 : h)
    d.sizeInMillimeters = CGSize(width: 600, height: 340)
    d.vendorID = 0x5353; d.productID = 0x0100; d.serialNum = UInt32(i)
    guard let vd = CGVirtualDisplay(descriptor: d) else { return nil }
    let s = CGVirtualDisplaySettings()
    s.hiDPI = hiDPI ? 1 : 0
    s.modes = hiDPI ? [CGVirtualDisplayMode(width: UInt(w * 2), height: UInt(h * 2), refreshRate: hz), CGVirtualDisplayMode(width: UInt(w), height: UInt(h), refreshRate: hz)]
                    : [CGVirtualDisplayMode(width: UInt(w), height: UInt(h), refreshRate: hz)]
    return vd.apply(s) ? vd : nil
}

func report(_ tag: String, _ vds: [CGVirtualDisplay]) {
    var n: UInt32 = 0; CGGetActiveDisplayList(0, nil, &n)
    var ids = [CGDirectDisplayID](repeating: 0, count: Int(n)); CGGetActiveDisplayList(n, &ids, &n)
    print("\(tag): created \(vds.count), active displays now \(n), our ids \(vds.map { $0.displayID })")
    for id in ids where CGDisplayIsBuiltin(id) == 0 {
        let m = CGDisplayCopyDisplayMode(id)
        let all = (CGDisplayCopyAllDisplayModes(id, [kCGDisplayShowDuplicateLowResolutionModes: true] as CFDictionary) as? [CGDisplayMode]) ?? []
        print("  active id \(id): \(m.map { "\($0.width)x\($0.height) pt, \($0.pixelWidth)x\($0.pixelHeight) px, \($0.refreshRate) Hz" } ?? "no mode"); modes: " + all.map { "\($0.width)x\($0.height)@\(Int($0.refreshRate))\($0.pixelWidth != $0.width ? "(2x)" : "")" }.joined(separator: " "))
    }
}

func mode(_ id: CGDirectDisplayID) -> String { CGDisplayCopyDisplayMode(id).map { "\($0.pixelWidth)x\($0.pixelHeight)@\(Int($0.refreshRate))" } ?? "NO MODE" }
func settings(_ w: Int, _ h: Int, _ hz: Double) -> CGVirtualDisplaySettings { let s = CGVirtualDisplaySettings(); s.hiDPI = 0; s.modes = [CGVirtualDisplayMode(width: UInt(w), height: UInt(h), refreshRate: hz)]; return s }
var live: [CGVirtualDisplay] = [make(701, w: 1920, h: 1080, hz: 60, hiDPI: false)!]
func step(_ t: Double, _ f: @escaping () -> Void) { DispatchQueue.main.asyncAfter(deadline: .now() + t, execute: f) }
step(2) {
    live.append(make(702, w: 2560, h: 1440, hz: 90, hiDPI: false)!)
    live.append(make(703, w: 1920, h: 1080, hz: 60, hiDPI: false)!)
    step(2) {
        print("1 added larger + same later:", live.map { mode($0.displayID) })
        // park instead of destroy: mirror display 703 onto the built-in
        var cfg: CGDisplayConfigRef?; CGBeginDisplayConfiguration(&cfg)
        CGConfigureDisplayMirrorOfDisplay(cfg, live[2].displayID, CGMainDisplayID())
        print("  mirror err:", CGCompleteDisplayConfiguration(cfg, .forSession).rawValue)
        step(2) {
            var n: UInt32 = 0; CGGetActiveDisplayList(0, nil, &n); var on: UInt32 = 0; CGGetOnlineDisplayList(0, nil, &on)
            print("2 parked 703 as mirror: in mirror set \(CGDisplayIsInMirrorSet(live[2].displayID) != 0), active \(n), online \(on), main bounds \(CGDisplayBounds(CGMainDisplayID()).size)")
            var cfg2: CGDisplayConfigRef?; CGBeginDisplayConfiguration(&cfg2)
            CGConfigureDisplayMirrorOfDisplay(cfg2, live[2].displayID, kCGNullDirectDisplay)
            print("  unmirror err:", CGCompleteDisplayConfiguration(cfg2, .forSession).rawValue)
            step(2) { print("3 unparked:", live.map { mode($0.displayID) }, "in mirror set:", CGDisplayIsInMirrorSet(live[2].displayID) != 0); exit(0) }
        }
    }
}
RunLoop.main.run()
