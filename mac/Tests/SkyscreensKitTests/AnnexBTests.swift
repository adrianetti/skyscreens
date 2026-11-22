import Foundation
import Testing

@testable import SkyscreensKit

func nalUnitTypes(_ annexB: Data) -> [UInt8] {
    let bytes = [UInt8](annexB)
    var types: [UInt8] = []
    var i = 0
    while i + 4 < bytes.count {
        if bytes[i..<i + 4] == [0, 0, 0, 1] {
            types.append(bytes[i + 4] & 0x1f)
            i += 4
        } else {
            i += 1
        }
    }
    return types
}

struct AnnexBTests {
    func convert(_ input: [UInt8], lengthSize: Int = 4) -> (Bool, [UInt8]) {
        var out = Data()
        let ok = VideoEncoder.appendUnits(lengthPrefixed: input, lengthSize: lengthSize, to: &out)
        return (ok, [UInt8](out))
    }

    @Test func lengthPrefixesBecomeStartCodes() {
        let (ok, out) = convert([0, 0, 0, 2, 0x65, 0xaa, 0, 0, 0, 1, 0x41])
        #expect(ok)
        #expect(out == [0, 0, 0, 1, 0x65, 0xaa, 0, 0, 0, 1, 0x41])
    }

    @Test func twoByteLengthPrefixesAreSupported() {
        let (ok, out) = convert([0, 3, 0x65, 1, 2], lengthSize: 2)
        #expect(ok && out == [0, 0, 0, 1, 0x65, 1, 2])
    }

    @Test func truncatedUnitsAreRejected() {
        #expect(!convert([0, 0, 0, 9, 0x65, 1]).0)
    }

    @Test func emptyInputProducesNothing() {
        let (ok, out) = convert([])
        #expect(ok && out.isEmpty)
    }

    @Test func nalTypeHelperFindsEveryUnit() {
        let data = Data([0, 0, 0, 1, 0x67, 1, 0, 0, 0, 1, 0x68, 2, 0, 0, 0, 1, 0x65, 3])
        #expect(nalUnitTypes(data) == [7, 8, 5])
    }
}
