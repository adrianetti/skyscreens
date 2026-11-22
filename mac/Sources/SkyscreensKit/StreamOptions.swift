import Foundation

public struct StreamOptions: Equatable {
    public enum ParseError: Error, CustomStringConvertible, Equatable {
        case badSeconds(String?)
        case unknown(String)

        public var description: String {
            switch self {
            case .badSeconds(let value): "--seconds needs a positive number, got \(value ?? "nothing")"
            case .unknown(let argument): "unknown stream option \(argument)"
            }
        }
    }

    public var seconds: Double?
    public var launchHeadset = true
    public var testPattern = false

    public init(seconds: Double? = nil, launchHeadset: Bool = true, testPattern: Bool = false) {
        self.seconds = seconds
        self.launchHeadset = launchHeadset
        self.testPattern = testPattern
    }

    public init(arguments: [String]) throws {
        var rest = arguments[...]
        while let arg = rest.popFirst() {
            switch arg {
            case "--seconds":
                let value = rest.popFirst()
                guard let parsed = value.flatMap(Double.init), parsed > 0, parsed.isFinite else {
                    throw ParseError.badSeconds(value)
                }
                seconds = parsed
            case "--no-headset":
                launchHeadset = false
            case "--test-pattern":
                testPattern = true
            default:
                throw ParseError.unknown(arg)
            }
        }
    }
}
