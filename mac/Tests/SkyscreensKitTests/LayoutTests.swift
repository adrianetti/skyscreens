import Testing

@testable import SkyscreensKit

struct LayoutTests {
    @Test func defaultLayoutAt25PixelsPerDegreeMatchesTheArchitecture() {
        let sizes = Layout.standard(pixelsPerDegree: 25).map { [$0.width, $0.height] }
        #expect(sizes == [[1616, 1008], [1296, 816], [1296, 816], [1296, 736]])
    }

    @Test(arguments: Array(stride(from: 12.0, through: 45.0, by: 0.5)))
    func everyScreenIsAMacroblockMultiple(ppd: Double) {
        for spec in Layout.standard(pixelsPerDegree: ppd) {
            #expect(spec.width % Layout.macroblock == 0 && spec.height % Layout.macroblock == 0)
            #expect(spec.width >= Layout.macroblock && spec.height >= Layout.macroblock)
            #expect(spec.width <= Int(UInt16.max) && spec.height <= Int(UInt16.max))
        }
    }

    @Test func sharperHeadsetsGetMoreWorkspace() {
        let widths = [20.0, 25, 30, 40].map { Layout.standard(pixelsPerDegree: $0)[0].width }
        #expect(widths == widths.sorted() && Set(widths).count == widths.count)
    }

    @Test func screensAreNamedInLayoutOrder() {
        let names = Layout.standard(pixelsPerDegree: 25, roles: [.video, .code]).map(\.name)
        #expect(names == ["skyscreens 1 · video", "skyscreens 2 · code"])
    }

    @Test func refreshRateIsPassedThrough() {
        #expect(Layout.standard(pixelsPerDegree: 25, refreshRate: 90).allSatisfy { $0.refreshRate == 90 })
    }

    @Test func virtualDisplaysReportAPlausiblePhysicalSize() {
        let size = VirtualDisplaySet.physicalSize(Layout.standard(pixelsPerDegree: 25)[0])
        #expect(abs(size.width - 373.2) < 0.5)
    }
}
