// VideoToolbox encoder benchmark for skyscreens architecture decisions.
// Measures real hardware encode latency, frame sizes and quality (PSNR) on synthetic
// desktop content: typing in a code editor, scrolling code, and full-motion video.
import Foundation
import CoreGraphics
import CoreText
import CoreVideo
import CoreMedia
import VideoToolbox
import ImageIO

let NV12 = kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
func nowNs() -> UInt64 { DispatchTime.now().uptimeNanoseconds }

func makeBuffer(_ w: Int, _ h: Int, _ fmt: OSType) -> CVPixelBuffer {
    var pb: CVPixelBuffer?
    let attrs: [CFString: Any] = [kCVPixelBufferIOSurfacePropertiesKey: [:] as CFDictionary]
    CVPixelBufferCreate(nil, w, h, fmt, attrs as CFDictionary, &pb)
    return pb!
}

// MARK: - Synthetic content

struct RNG { var s: UInt64; mutating func next() -> UInt64 { s ^= s << 13; s ^= s >> 7; s ^= s << 17; return s } }

func rgb(_ r: Int, _ g: Int, _ b: Int) -> CGColor { CGColor(srgbRed: CGFloat(r) / 255, green: CGFloat(g) / 255, blue: CGFloat(b) / 255, alpha: 1) }
let palette = [rgb(86, 156, 214), rgb(206, 145, 120), rgb(106, 153, 85), rgb(220, 220, 170), rgb(156, 220, 254), rgb(212, 212, 212), rgb(197, 134, 192)]
let words = ["let", "func", "return", "frame", "encoder", "session", "if", "guard", "else", "self", "width", "height", "buffer", "true", "nil", "for", "in", "struct", "Int", "String", "\"hello\"", "// note", "(", ")", "{", "}", "=", "+", ".", ","]
let font = CTFontCreateWithName("Menlo" as CFString, 14, nil)
let periodLines = 24 // scroll content repeats every 24 lines so the loop is seamless
let lineH = 20

func codeLine(_ n: Int) -> [(String, CGColor)] {
    var r = RNG(s: UInt64(n % periodLines) &* 2654435761 &+ 12345)
    let indent = String(repeating: " ", count: Int(r.next() % 4) * 4)
    var out: [(String, CGColor)] = [(indent, palette[5])]
    for _ in 0..<(3 + Int(r.next() % 7)) { out.append((words[Int(r.next() % UInt64(words.count))] + " ", palette[Int(r.next() % UInt64(palette.count))])) }
    return out
}

func drawText(_ ctx: CGContext, _ s: String, _ c: CGColor, _ x: CGFloat, _ y: CGFloat) -> CGFloat {
    let a = NSAttributedString(string: s, attributes: [kCTFontAttributeName as NSAttributedString.Key: font, kCTForegroundColorAttributeName as NSAttributedString.Key: c])
    let line = CTLineCreateWithAttributedString(a)
    ctx.textPosition = CGPoint(x: x, y: y)
    CTLineDraw(line, ctx)
    return x + CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil))
}

func drawCode(_ ctx: CGContext, _ w: Int, _ h: Int, scroll: Int, typed: Int) {
    ctx.setFillColor(rgb(30, 30, 30)); ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))
    ctx.setFillColor(rgb(37, 37, 38)); ctx.fill(CGRect(x: 0, y: 0, width: 260, height: h)) // sidebar
    for i in 0..<(h / 22) { _ = drawText(ctx, "  file_\(i).swift", palette[5], 10, CGFloat(h - 22 * (i + 1))) }
    let first = scroll / lineH, off = scroll % lineH
    for k in 0...(h / lineH + 1) {
        let n = first + k
        let y = CGFloat(h - (k + 1) * lineH + off)
        _ = drawText(ctx, String(format: "%4d", n % 1000 + 1), rgb(110, 118, 129), 270, y)
        var x: CGFloat = 330
        if k == 12 && typed >= 0 {
            let s = String(repeating: "abcdefghij", count: 12).prefix(typed)
            x = drawText(ctx, String(s), palette[4], x, y)
            ctx.setFillColor(rgb(220, 220, 220)); ctx.fill(CGRect(x: x, y: y - 3, width: 2, height: 17))
        } else {
            for (t, c) in codeLine(n) { x = drawText(ctx, t, c, x, y) }
        }
    }
}

func loadImage(_ path: String) -> CGImage? {
    guard let src = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil) else { return nil }
    return CGImageSourceCreateThumbnailAtIndex(src, 0, [kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: 5000] as CFDictionary)
}

enum Content: String { case typing, scroll, video }

/// Renders `count` NV12 frames of the given content type.
func makeFrames(_ c: Content, _ w: Int, _ h: Int, count: Int, image: CGImage?) -> [CVPixelBuffer] {
    var xfer: VTPixelTransferSession?
    VTPixelTransferSessionCreate(allocator: nil, pixelTransferSessionOut: &xfer)
    let bgra = makeBuffer(w, h, kCVPixelFormatType_32BGRA)
    var rng = RNG(s: 99)
    var out: [CVPixelBuffer] = []
    for i in 0..<count {
        CVPixelBufferLockBaseAddress(bgra, [])
        let ctx = CGContext(data: CVPixelBufferGetBaseAddress(bgra), width: w, height: h, bitsPerComponent: 8, bytesPerRow: CVPixelBufferGetBytesPerRow(bgra),
                            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue)!
        switch c {
        case .typing: drawCode(ctx, w, h, scroll: 0, typed: i * 3)
        case .scroll: drawCode(ctx, w, h, scroll: i * (periodLines * lineH / count), typed: -1)
        case .video:
            let z = 1.35 + 0.002 * Double(i)
            let iw = Double(w) * z, ih = iw * Double(image!.height) / Double(image!.width)
            ctx.interpolationQuality = .high
            ctx.draw(image!, in: CGRect(x: -Double(i) * 5, y: -Double(i) * 2, width: iw, height: max(ih, Double(h) * z)))
            // light per-frame sensor noise, like real camera footage
            let p = CVPixelBufferGetBaseAddress(bgra)!.assumingMemoryBound(to: UInt8.self)
            let bpr = CVPixelBufferGetBytesPerRow(bgra)
            for y in stride(from: 0, to: h, by: 1) { for x in stride(from: 0, to: w * 4, by: 4) where (x / 4 + y) & 3 == 0 {
                let d = Int(rng.next() % 9) - 4
                for k in 0..<3 { let v = Int(p[y * bpr + x + k]) + d; p[y * bpr + x + k] = UInt8(max(0, min(255, v))) }
            } }
        }
        CVPixelBufferUnlockBaseAddress(bgra, [])
        let nv = makeBuffer(w, h, NV12)
        VTPixelTransferSessionTransferImage(xfer!, from: bgra, to: nv)
        out.append(nv)
    }
    return out
}

// MARK: - Encoder

final class Recorder {
    let lock = NSLock()
    var lat: [Double] = []; var bytes: [Int] = []; var key: [Bool] = []; var samples: [CMSampleBuffer] = []; var drops = 0
    func add(_ l: Double, _ s: CMSampleBuffer?, keep: Bool) {
        lock.lock(); defer { lock.unlock() }
        guard let s else { drops += 1; return }
        lat.append(l)
        bytes.append(CMBlockBufferGetDataLength(CMSampleBufferGetDataBuffer(s)!))
        let att = CMSampleBufferGetSampleAttachmentsArray(s, createIfNecessary: false) as? [[CFString: Any]]
        key.append(!(att?.first?[kCMSampleAttachmentKey_NotSync] as? Bool ?? false))
        if keep { samples.append(s) }
    }
}

struct Config { var codec: CMVideoCodecType; var w: Int; var h: Int; var fps: Int; var mbps: Double; var lowLatency: Bool }

func codecName(_ c: CMVideoCodecType) -> String { c == kCMVideoCodecType_H264 ? "H.264" : "HEVC" }

func makeEncoder(_ c: Config, verbose: Bool) -> VTCompressionSession? {
    var spec: [CFString: Any] = [kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder: true]
    if c.lowLatency { spec[kVTVideoEncoderSpecification_EnableLowLatencyRateControl] = true }
    var s: VTCompressionSession?
    let st = VTCompressionSessionCreate(allocator: nil, width: Int32(c.w), height: Int32(c.h), codecType: c.codec, encoderSpecification: spec as CFDictionary,
                                        imageBufferAttributes: nil, compressedDataAllocator: nil, outputCallback: nil, refcon: nil, compressionSessionOut: &s)
    guard st == noErr, let s else { print("  create failed \(st) for \(codecName(c.codec)) lowLatency=\(c.lowLatency)"); return nil }
    let profile = c.codec == kCMVideoCodecType_H264 ? kVTProfileLevel_H264_ConstrainedHigh_AutoLevel : kVTProfileLevel_HEVC_Main_AutoLevel
    let props: [(CFString, CFTypeRef)] = [
        (kVTCompressionPropertyKey_RealTime, kCFBooleanTrue),
        (kVTCompressionPropertyKey_AllowFrameReordering, kCFBooleanFalse),
        (kVTCompressionPropertyKey_ProfileLevel, profile),
        (kVTCompressionPropertyKey_AverageBitRate, NSNumber(value: Int(c.mbps * 1_000_000))),
        (kVTCompressionPropertyKey_ExpectedFrameRate, NSNumber(value: c.fps)),
        (kVTCompressionPropertyKey_MaxKeyFrameInterval, NSNumber(value: 100_000)),
        (kVTCompressionPropertyKey_MaxFrameDelayCount, NSNumber(value: 0)),
    ]
    for (k, v) in props { let r = VTSessionSetProperty(s, key: k, value: v); if r != noErr && verbose { print("  set \(k) failed \(r)") } }
    if verbose {
        var dict: CFDictionary?
        VTSessionCopySupportedPropertyDictionary(s, supportedPropertyDictionaryOut: &dict)
        let keys = Set(((dict as? [String: Any]) ?? [:]).keys)
        let want = ["EnableLTR", "MaxAllowedFrameQP", "ConstantBitRate", "SpatialAdaptiveQPLevel", "MaxFrameDelayCount", "ReferenceBufferCount", "BaseLayerFrameRateFraction", "PrioritizeEncodingSpeedOverQuality"]
        print("  supports: " + want.map { "\($0)=\(keys.contains($0) ? "yes" : "no")" }.joined(separator: " "))
    }
    VTCompressionSessionPrepareToEncodeFrames(s)
    return s
}

/// Encodes frames at real-time pace and records per-frame latency.
func runEncode(_ c: Config, frames: [CVPixelBuffer], n: Int, rec: Recorder, keep: Bool, verbose: Bool, forceKeyAt: Set<Int> = [0, 300]) {
    guard let s = makeEncoder(c, verbose: verbose) else { return }
    let period = 1_000_000_000 / UInt64(c.fps)
    let t0 = nowNs() + 50_000_000
    for i in 0..<n {
        let due = t0 + UInt64(i) * period
        let now = nowNs(); if due > now { usleep(UInt32((due - now) / 1000)) }
        let m = frames.count, j = (i / (m - 1)) % 2 == 0 ? i % (m - 1) : (m - 1) - i % (m - 1) // ping-pong through frames
        let f = c.codec == 0 ? frames[i % m] : frames[j]
        let props = forceKeyAt.contains(i) ? [kVTEncodeFrameOptionKey_ForceKeyFrame: true] as CFDictionary : nil
        let sub = nowNs()
        VTCompressionSessionEncodeFrame(s, imageBuffer: f, presentationTimeStamp: CMTime(value: Int64(i), timescale: Int32(c.fps)), duration: .invalid,
                                        frameProperties: props, infoFlagsOut: nil) { _, _, sb in
            rec.add(Double(nowNs() - sub) / 1e6, sb, keep: keep)
        }
    }
    VTCompressionSessionCompleteFrames(s, untilPresentationTimeStamp: .invalid)
    VTCompressionSessionInvalidate(s)
}

func frameIndex(_ i: Int, _ m: Int) -> Int { (i / (m - 1)) % 2 == 0 ? i % (m - 1) : (m - 1) - i % (m - 1) }

// MARK: - Quality

func psnr(_ a: CVPixelBuffer, _ b: CVPixelBuffer) -> (Double, Double) {
    CVPixelBufferLockBaseAddress(a, .readOnly); CVPixelBufferLockBaseAddress(b, .readOnly)
    defer { CVPixelBufferUnlockBaseAddress(a, .readOnly); CVPixelBufferUnlockBaseAddress(b, .readOnly) }
    func plane(_ p: Int) -> Double {
        let w = CVPixelBufferGetWidthOfPlane(a, p) * (p == 1 ? 2 : 1), h = CVPixelBufferGetHeightOfPlane(a, p)
        let pa = CVPixelBufferGetBaseAddressOfPlane(a, p)!.assumingMemoryBound(to: UInt8.self), ra = CVPixelBufferGetBytesPerRowOfPlane(a, p)
        let pb = CVPixelBufferGetBaseAddressOfPlane(b, p)!.assumingMemoryBound(to: UInt8.self), rb = CVPixelBufferGetBytesPerRowOfPlane(b, p)
        var se: Int = 0
        for y in 0..<h { for x in 0..<w { let d = Int(pa[y * ra + x]) - Int(pb[y * rb + x]); se += d * d } }
        let mse = Double(se) / Double(w * h)
        return mse == 0 ? 99 : 10 * log10(255 * 255 / mse)
    }
    return (plane(0), plane(1))
}

func measureQuality(_ samples: [CMSampleBuffer], frames: [CVPixelBuffer], every: Int) -> (Double, Double, Double) {
    guard let fd = CMSampleBufferGetFormatDescription(samples[0]) else { return (0, 0, 0) }
    var d: VTDecompressionSession?
    VTDecompressionSessionCreate(allocator: nil, formatDescription: fd, decoderSpecification: nil,
                                 imageBufferAttributes: [kCVPixelBufferPixelFormatTypeKey: NV12] as CFDictionary, outputCallback: nil, decompressionSessionOut: &d)
    var ys: [Double] = [], uvs: [Double] = []
    for (i, sb) in samples.enumerated() {
        VTDecompressionSessionDecodeFrame(d!, sampleBuffer: sb, flags: [], infoFlagsOut: nil) { _, _, img, _, _ in
            guard i % every == 5, let img else { return }
            let (y, uv) = psnr(img, frames[frameIndex(i, frames.count)])
            ys.append(y); uvs.append(uv)
        }
    }
    VTDecompressionSessionWaitForAsynchronousFrames(d!)
    VTDecompressionSessionInvalidate(d!)
    return (ys.reduce(0, +) / Double(max(1, ys.count)), uvs.reduce(0, +) / Double(max(1, uvs.count)), ys.min() ?? 0)
}

// MARK: - Reporting

func pct(_ a: [Double], _ p: Double) -> Double { let s = a.sorted(); return s.isEmpty ? 0 : s[min(s.count - 1, Int(p * Double(s.count)))] }
func pcti(_ a: [Int], _ p: Double) -> Int { let s = a.sorted(); return s.isEmpty ? 0 : s[min(s.count - 1, Int(p * Double(s.count)))] }

func summary(_ tag: String, _ c: Config, _ r: Recorder, n: Int) -> String {
    let p = zip(r.bytes, r.key).filter { !$0.1 }.map { $0.0 }, k = zip(r.bytes, r.key).filter { $0.1 }.map { $0.0 }
    let mbps = Double(r.bytes.reduce(0, +)) * 8 / (Double(n) / Double(c.fps)) / 1e6
    return String(format: "%-34@ enc ms p50 %4.1f p99 %4.1f max %5.1f | %5.1f Mbps | P-frame KB p50 %5.1f p99 %6.1f max %6.1f | key KB %6.1f | drops %d",
                  tag as NSString, pct(r.lat, 0.5), pct(r.lat, 0.99), r.lat.max() ?? 0, mbps, Double(pcti(p, 0.5)) / 1024, Double(pcti(p, 0.99)) / 1024,
                  Double(p.max() ?? 0) / 1024, Double(k.max() ?? 0) / 1024, r.drops)
}


import Compression
setvbuf(stdout, nil, _IOLBF, 0)
let img = loadImage("/System/Library/Desktop Pictures/Sonoma.heic")
let N = 480
let H264 = kCMVideoCodecType_H264

func latOnly(_ c: Config, _ frames: [CVPixelBuffer]) -> Recorder { let r = Recorder(); runEncode(c, frames: frames, n: N, rec: r, keep: false, verbose: false); return r }

print("== A. Single-session latency vs frame size (H.264 low latency, typing content) ==")
for (w, h) in [(640, 368), (1280, 720), (2560, 368), (1920, 1080), (1920, 1200), (2560, 1440), (2560, 1600)] {
    let f = makeFrames(.typing, w, h, count: 24, image: img)
    let r = latOnly(Config(codec: H264, w: w, h: h, fps: 60, mbps: 15, lowLatency: true), f)
    print(String(format: "  %4dx%-4d %4.1f MP: p50 %5.2f  p99 %5.2f ms", w, h, Double(w * h) / 1e6, pct(r.lat, 0.5), pct(r.lat, 0.99)))
}

func concurrent(_ tag: String, _ jobs: [(String, Config, [CVPixelBuffer], UInt64)]) {
    let recs = jobs.map { _ in Recorder() }
    let g = DispatchGroup()
    for (i, j) in jobs.enumerated() {
        g.enter()
        Thread { usleep(UInt32(j.3)); runEncode(j.1, frames: j.2, n: N * j.1.fps / 60, rec: recs[i], keep: false, verbose: false, forceKeyAt: [0]); g.leave() }.start()
    }
    g.wait()
    print(tag)
    for (i, j) in jobs.enumerated() { print(String(format: "    %-28@ p50 %5.1f  p99 %5.1f ms", j.0 as NSString, pct(recs[i].lat, 0.5), pct(recs[i].lat, 0.99))) }
}

print("== B. Realistic concurrency (random phase between screens) ==")
let code1440 = makeFrames(.scroll, 2560, 1440, count: 40, image: img)
let vid1080 = makeFrames(.video, 1920, 1080, count: 40, image: img)
let vid720 = makeFrames(.video, 1280, 720, count: 40, image: img)
let vid1440 = makeFrames(.video, 2560, 1440, count: 40, image: img)
concurrent("  B1 code scroll 1440p60 + video 1440p30:", [("code scroll 2560x1440 @60", Config(codec: H264, w: 2560, h: 1440, fps: 60, mbps: 15, lowLatency: true), code1440, 0), ("video 2560x1440 @30", Config(codec: H264, w: 2560, h: 1440, fps: 30, mbps: 15, lowLatency: true), vid1440, 7000)])
concurrent("  B2 code scroll 1440p60 + video 1080p30:", [("code scroll 2560x1440 @60", Config(codec: H264, w: 2560, h: 1440, fps: 60, mbps: 15, lowLatency: true), code1440, 0), ("video 1920x1080 @30", Config(codec: H264, w: 1920, h: 1080, fps: 30, mbps: 10, lowLatency: true), vid1080, 7000)])
concurrent("  B3 code scroll 1440p60 + video 720p30:", [("code scroll 2560x1440 @60", Config(codec: H264, w: 2560, h: 1440, fps: 60, mbps: 15, lowLatency: true), code1440, 0), ("video 1280x720 @30", Config(codec: H264, w: 1280, h: 720, fps: 30, mbps: 6, lowLatency: true), vid720, 7000)])
concurrent("  B4 four 1440p60 screens, phases spread 4 ms apart:", (0..<4).map { i in ("screen \(i) 2560x1440 @60", Config(codec: H264, w: 2560, h: 1440, fps: 60, mbps: 15, lowLatency: true), code1440, UInt64(i * 4000)) })

print("== C. Lossless lane: LZ4 / LZFSE on raw BGRA (Apple Compression framework) ==")
func bgra(_ c: Content, _ w: Int, _ h: Int, _ i: Int) -> Data {
    let pb = makeBuffer(w, h, kCVPixelFormatType_32BGRA)
    CVPixelBufferLockBaseAddress(pb, [])
    let ctx = CGContext(data: CVPixelBufferGetBaseAddress(pb), width: w, height: h, bitsPerComponent: 8, bytesPerRow: CVPixelBufferGetBytesPerRow(pb), space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue)!
    drawCode(ctx, w, h, scroll: 0, typed: i)
    let d = Data(bytes: CVPixelBufferGetBaseAddress(pb)!, count: CVPixelBufferGetBytesPerRow(pb) * h)
    CVPixelBufferUnlockBaseAddress(pb, [])
    return d
}
func comp(_ d: Data, _ alg: compression_algorithm) -> (Int, Double) {
    let dst = UnsafeMutablePointer<UInt8>.allocate(capacity: d.count + 4096); defer { dst.deallocate() }
    var best = 1e9, n = 0
    for _ in 0..<5 {
        let t = nowNs()
        n = d.withUnsafeBytes { compression_encode_buffer(dst, d.count + 4096, $0.bindMemory(to: UInt8.self).baseAddress!, d.count, nil, alg) }
        best = min(best, Double(nowNs() - t) / 1e6)
    }
    return (n, best)
}
func decomp(_ d: Data, _ alg: compression_algorithm) -> Double {
    let src = UnsafeMutablePointer<UInt8>.allocate(capacity: d.count + 4096); defer { src.deallocate() }
    let n = d.withUnsafeBytes { compression_encode_buffer(src, d.count + 4096, $0.bindMemory(to: UInt8.self).baseAddress!, d.count, nil, alg) }
    let out = UnsafeMutablePointer<UInt8>.allocate(capacity: d.count); defer { out.deallocate() }
    var best = 1e9
    for _ in 0..<5 { let t = nowNs(); _ = compression_decode_buffer(out, d.count, src, n, nil, alg); best = min(best, Double(nowNs() - t) / 1e6) }
    return best
}
let full = bgra(.typing, 2560, 1440, 40)
// typing patch: the changed line region around the caret, 320x24 px
let patchW = 320, patchH = 24, bpr = 2560 * 4
var patch = Data(capacity: patchW * patchH * 4)
for y in 0..<patchH { patch.append(full.subdata(in: ((700 + y) * bpr + 330 * 4)..<((700 + y) * bpr + (330 + patchW) * 4))) }
for (name, d) in [("full 2560x1440 code screen", full), ("typing patch 320x24", patch)] {
    for (an, a) in [("LZ4", COMPRESSION_LZ4), ("LZFSE", COMPRESSION_LZFSE), ("zlib", COMPRESSION_ZLIB)] {
        let (n, ms) = comp(d, a)
        print(String(format: "  %-28@ %-5@ raw %7.1f KB -> %6.1f KB (%4.1fx)  encode %6.2f ms  decode %6.2f ms (M2)", name as NSString, an as NSString, Double(d.count) / 1024, Double(n) / 1024, Double(d.count) / Double(n), ms, decomp(d, a)))
    }
}
