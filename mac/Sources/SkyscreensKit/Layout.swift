import Foundation

/// What a screen is for. It sets the screen's default size and how sharp its text needs to be.
public enum Role: String, Codable, Sendable, CaseIterable {
    case code, chat, email, video

    /// Default width in degrees of the headset's field of view.
    var widthDegrees: Double {
        switch self {
        case .code: 58
        case .chat, .email: 44
        case .video: 31
        }
    }

    /// Headset pixels per desktop point. About 1 keeps 13 pt text comfortable; video needs less.
    var density: Double {
        switch self {
        case .code: 0.9
        case .chat, .email: 0.85
        case .video: 0.6
        }
    }

    var aspect: Double { self == .video ? 16.0 / 9.0 : 16.0 / 10.0 }
}

/// One virtual display. Displays run at 1x, so points and pixels are the same.
public struct ScreenSpec: Codable, Sendable, Equatable {
    public var name: String
    public var role: Role
    public var width: Int
    public var height: Int
    public var refreshRate: Double
}

public enum Layout {
    /// Sizes each screen from the headset's pixels per degree, so a sharper headset gets more
    /// workspace per screen. Sizes are rounded to multiples of 16 to match encoder macroblocks.
    public static func standard(pixelsPerDegree ppd: Double, roles: [Role] = [.code, .chat, .email, .video], refreshRate: Double = 60) -> [ScreenSpec] {
        roles.enumerated().map { i, role in
            let width = round16(role.widthDegrees * ppd / role.density)
            return ScreenSpec(name: "skyscreens \(i + 1) · \(role.rawValue)", role: role, width: width,
                              height: round16(Double(width) / role.aspect), refreshRate: refreshRate)
        }
    }

    static func round16(_ v: Double) -> Int { max(16, Int((v / 16).rounded()) * 16) }
}
