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

let a = CommandLine.arguments
let n = Int(a[1])!, w = Int(a[2])!, h = Int(a[3])!, hz = Double(a[4])!, hi = a[5] == "1", base = Int(a[6])!
var vds: [CGVirtualDisplay] = []
for i in 0..<n { if let v = make(base + i, w: w, h: h, hz: hz, hiDPI: hi) { vds.append(v) } }
DispatchQueue.main.asyncAfter(deadline: .now() + 3) {
    report("\(n) x \(w)x\(h) @\(Int(hz)) hiDPI=\(hi)", vds)
    exit(0)
}
dispatchMain()
