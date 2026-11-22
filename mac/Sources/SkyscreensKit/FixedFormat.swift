import Foundation

enum FixedFormat {
    static let posix = Locale(identifier: "en_US_POSIX")

    static func decimal(_ value: Double, places: Int, width: Int = 0) -> String {
        let text =
            value.isFinite
            ? value.formatted(.number.locale(posix).grouping(.never).precision(.fractionLength(places)))
            : "nan"
        return padded(text, width: width)
    }

    static func integer(_ value: Int, width: Int) -> String {
        padded(String(value), width: width)
    }

    static func padded(_ text: String, width: Int) -> String {
        String(repeating: " ", count: max(0, width - text.count)) + text
    }
}
