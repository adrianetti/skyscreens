import Foundation
import Testing

@testable import SkyscreensKit

struct StreamOptionsTests {
    @Test func defaultsStreamForeverToTheHeadset() throws {
        #expect(try StreamOptions(arguments: []) == StreamOptions(seconds: nil, launchHeadset: true))
    }

    @Test func parsesEveryOption() throws {
        let options = try StreamOptions(arguments: ["--seconds", "12.5", "--no-headset", "--test-pattern"])
        #expect(options == StreamOptions(seconds: 12.5, launchHeadset: false, testPattern: true))
    }

    @Test(arguments: [["--seconds"], ["--seconds", "zero"], ["--seconds", "-1"], ["--seconds", "0"], ["--bogus"]])
    func rejectsBadArguments(arguments: [String]) {
        #expect(throws: StreamOptions.ParseError.self) { try StreamOptions(arguments: arguments) }
    }
}
