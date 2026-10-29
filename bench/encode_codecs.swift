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

// MARK: - Tests

setvbuf(stdout, nil, _IOLBF, 0)
let img = loadImage("/System/Library/Desktop Pictures/Sonoma.heic")
let N = 600
let mode = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "all"

func single(_ tag: String, _ c: Config, _ content: Content, frames: [CVPixelBuffer], quality: Bool, verbose: Bool = false) {
    let r = Recorder()
    runEncode(c, frames: frames, n: N, rec: r, keep: quality, verbose: verbose)
    var line = summary(tag, c, r, n: N)
    if quality, !r.samples.isEmpty { let (y, uv, ymin) = measureQuality(r.samples, frames: frames, every: 10); line += String(format: " | PSNR Y %.1f (min %.1f) UV %.1f dB", y, ymin, uv) }
    print(line)
}

if mode == "all" || mode == "codecs" {
    print("== 1. Codec x content, 2560x1440 @60, 20 Mbps target, real-time pace ==")
    var cache: [Content: [CVPixelBuffer]] = [:]
    for content in [Content.typing, .scroll, .video] { cache[content] = makeFrames(content, 2560, 1440, count: 40, image: img) }
    for (codec, ll) in [(kCMVideoCodecType_H264, true), (kCMVideoCodecType_HEVC, true), (kCMVideoCodecType_HEVC, false)] {
        let probe = Config(codec: codec, w: 2560, h: 1440, fps: 60, mbps: 20, lowLatency: ll)
        print("\(codecName(codec)) lowLatency=\(ll):")
        if makeEncoder(probe, verbose: true) == nil { continue }
        for content in [Content.typing, .scroll, .video] {
            single("  \(content.rawValue)", probe, content, frames: cache[content]!, quality: true)
        }
    }
}

if mode == "all" || mode == "scaling" {
    print("== 2. Resolution and refresh scaling, scrolling code (hardest text case), H.264 low latency ==")
    for (w, h, fps, mbps) in [(1920, 1080, 60, 12.0), (2560, 1440, 60, 20.0), (2560, 1440, 90, 28.0), (2560, 1440, 120, 36.0), (3840, 2160, 60, 40.0)] {
        let f = makeFrames(.scroll, w, h, count: 40, image: img)
        single("  \(w)x\(h) @\(fps) \(Int(mbps)) Mbps", Config(codec: kCMVideoCodecType_H264, w: w, h: h, fps: fps, mbps: mbps, lowLatency: true), .scroll, frames: f, quality: false)
    }
}

if mode == "all" || mode == "concurrent" {
    for codec in [kCMVideoCodecType_H264, kCMVideoCodecType_HEVC] {
        print("== 3. Four screens at once, 2560x1440 @60, \(codecName(codec)) low latency (video, code typing, scroll, chat typing) ==")
        let set: [(String, Content)] = [("video", .video), ("code typing", .typing), ("code scroll", .scroll), ("chat typing", .typing)]
        var frames: [Content: [CVPixelBuffer]] = [:]
        for c in [Content.typing, .scroll, .video] { frames[c] = makeFrames(c, 2560, 1440, count: 40, image: img) }
        let recs = set.map { _ in Recorder() }
        let g = DispatchGroup()
        for (i, (_, content)) in set.enumerated() {
            g.enter()
            Thread {
                runEncode(Config(codec: codec, w: 2560, h: 1440, fps: 60, mbps: content == .video ? 25 : 15, lowLatency: true), frames: frames[content]!, n: N, rec: recs[i], keep: false, verbose: false, forceKeyAt: [0, 300 + i * 7])
                g.leave()
            }.start()
        }
        g.wait()
        for (i, (name, content)) in set.enumerated() { print(summary("  \(name)", Config(codec: codec, w: 2560, h: 1440, fps: 60, mbps: content == .video ? 25 : 15, lowLatency: true), recs[i], n: N)) }
    }
}
