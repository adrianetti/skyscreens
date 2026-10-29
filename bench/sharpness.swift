// Text sharpness model for a virtual screen seen through a Quest 3.
// Compares capture scale (source px per point) and compositor filtering against an
// ideal rendering made directly at headset resolution, under small head-pose changes.
import Foundation
import CoreGraphics
import CoreText

let ptW = 360, ptH = 150
let lines = ["func encode(_ frame: Frame) -> Packet? {", "    guard let session = encoder else { return nil }", "    let size = frame.width * frame.height // pixels",
             "    return session.encode(frame, deadline: 4.0)", "}", "Inbox (3)  Re: flight to Granada  10:42", "Status: 12 errors, 3 warnings   Ln 42, Col 17"]

struct Gray { var w: Int; var h: Int; var p: [Float] }

func render(scale: Double, dx: Double, dy: Double, rot: Double, outW: Int, outH: Int) -> Gray {
    let ctx = CGContext(data: nil, width: outW, height: outH, bitsPerComponent: 8, bytesPerRow: outW, space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue)!
    ctx.setFillColor(gray: 30 / 255, alpha: 1); ctx.fill(CGRect(x: 0, y: 0, width: outW, height: outH))
    ctx.setShouldSmoothFonts(false); ctx.setAllowsFontSmoothing(false); ctx.setShouldAntialias(true)
    ctx.translateBy(x: dx, y: dy); ctx.rotate(by: rot); ctx.scaleBy(x: scale, y: scale)
    for (i, s) in lines.enumerated() {
        let size: CGFloat = i >= 5 ? 11 : 13
        let font = CTFontCreateWithName((i >= 5 ? "Helvetica" : "Menlo") as CFString, size, nil)
        let a = NSAttributedString(string: s, attributes: [kCTFontAttributeName as NSAttributedString.Key: font,
                                                           kCTForegroundColorAttributeName as NSAttributedString.Key: CGColor(gray: 212 / 255, alpha: 1)])
        ctx.textPosition = CGPoint(x: 6, y: CGFloat(ptH) - 18 - CGFloat(i) * 19)
        CTLineDraw(CTLineCreateWithAttributedString(a), ctx)
    }
    let d = ctx.data!.assumingMemoryBound(to: UInt8.self)
    var p = [Float](repeating: 0, count: outW * outH)
    for y in 0..<outH { for x in 0..<outW { p[y * outW + x] = Float(d[(outH - 1 - y) * outW + x]) } } // store y-up
    return Gray(w: outW, h: outH, p: p)
}

func bilinear(_ g: Gray, _ x: Double, _ y: Double) -> Float {
    let fx = x - 0.5, fy = y - 0.5
    let x0 = Int(floor(fx)), y0 = Int(floor(fy)); let ax = Float(fx - Double(x0)), ay = Float(fy - Double(y0))
    func at(_ x: Int, _ y: Int) -> Float { g.p[max(0, min(g.h - 1, y)) * g.w + max(0, min(g.w - 1, x))] }
    return (at(x0, y0) * (1 - ax) + at(x0 + 1, y0) * ax) * (1 - ay) + (at(x0, y0 + 1) * (1 - ax) + at(x0 + 1, y0 + 1) * ax) * ay
}

func blur(_ g: Gray, sigma: Double) -> Gray {
    let r = Int(ceil(sigma * 3)); var k = (-r...r).map { Float(exp(-Double($0 * $0) / (2 * sigma * sigma))) }; let s = k.reduce(0, +); k = k.map { $0 / s }
    var t = g, o = g
    for y in 0..<g.h { for x in 0..<g.w { var a: Float = 0; for i in -r...r { a += k[i + r] * g.p[y * g.w + max(0, min(g.w - 1, x + i))] }; t.p[y * g.w + x] = a } }
    for y in 0..<g.h { for x in 0..<g.w { var a: Float = 0; for i in -r...r { a += k[i + r] * t.p[max(0, min(g.h - 1, y + i)) * g.w + x] }; o.p[y * g.w + x] = a } }
    return o
}

func psnr(_ a: Gray, _ b: Gray) -> Double {
    var se = 0.0, n = 0
    for y in 3..<(a.h - 3) { for x in 3..<(a.w - 3) { let d = Double(a.p[y * a.w + x] - b.p[y * b.w + x]); se += d * d; n += 1 } }
    return 10 * log10(255 * 255 / (se / Double(n)))
}

/// Headset view of a source image (k px per point) shown at r headset px per point.
func composite(_ src: Gray, k: Double, r: Double, dx: Double, dy: Double, rot: Double, taps: Int, outW: Int, outH: Int) -> Gray {
    var o = Gray(w: outW, h: outH, p: [Float](repeating: 0, count: outW * outH))
    let c = cos(-rot), s = sin(-rot)
    let offs: [Double] = taps == 1 ? [0] : (0..<taps).map { (Double($0) + 0.5) / Double(taps) - 0.5 }
    for y in 0..<outH { for x in 0..<outW {
        var acc: Float = 0
        for oy in offs { for ox in offs {
            let hx = Double(x) + 0.5 + ox - dx, hy = Double(y) + 0.5 + oy - dy
            let ux = (c * hx - s * hy) / r, uy = (s * hx + c * hy) / r // headset px -> points
            acc += bilinear(src, ux * k, uy * k)
        } }
        o.p[y * outW + x] = acc / Float(offs.count * offs.count)
    } }
    return o
}

setvbuf(stdout, nil, _IOLBF, 0)
var rng = SystemRandomNumberGenerator()
let poses = (0..<8).map { _ in (Double.random(in: 0..<1, using: &rng), Double.random(in: 0..<1, using: &rng), Double.random(in: -0.4...0.4, using: &rng) * .pi / 180) }
print("PSNR vs ideal text at headset resolution (higher = sharper, closer to ideal). Lens blur sigma 0.5 px applied to both.")
print(String(format: "%-26@ %@", "capture scale / filter" as NSString, [0.7, 0.85, 1.0, 1.25].map { String(format: "r=%.2f", $0) }.joined(separator: "   ")))
for (k, taps, name) in [(1.0, 1, "1x, plain bilinear"), (1.0, 3, "1x, supersampled"), (1.5, 1, "1.5x, plain bilinear"), (1.5, 3, "1.5x, supersampled"), (2.0, 1, "2x, plain bilinear"), (2.0, 3, "2x, supersampled")] {
    let src = render(scale: k, dx: 0, dy: 0, rot: 0, outW: Int(Double(ptW) * k), outH: Int(Double(ptH) * k))
    var row: [String] = []
    for r in [0.7, 0.85, 1.0, 1.25] {
        let ow = Int(Double(ptW) * r), oh = Int(Double(ptH) * r)
        var tot = 0.0
        for (dx, dy, rot) in poses {
            let ideal = blur(render(scale: r, dx: dx, dy: dy, rot: rot, outW: ow, outH: oh), sigma: 0.5)
            let seen = blur(composite(src, k: k, r: r, dx: dx, dy: dy, rot: rot, taps: taps, outW: ow, outH: oh), sigma: 0.5)
            tot += psnr(ideal, seen)
        }
        row.append(String(format: "%6.1f", tot / Double(poses.count)))
    }
    print(String(format: "%-26@ %@", name as NSString, row.joined(separator: "   ")))
}
