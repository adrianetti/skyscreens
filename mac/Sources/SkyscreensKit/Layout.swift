import Foundation

public enum Role: String, Codable, Sendable, CaseIterable {
    case code, chat, email, video

    var widthDegrees: Double {
        switch self {
        case .code: 58
        case .chat, .email: 44
        case .video: 31
        }
    }

    var headsetPixelsPerPoint: Double {
        switch self {
        case .code: 0.9
        case .chat, .email: 0.85
        case .video: 0.6
        }
    }

    var aspect: Double { self == .video ? 16.0 / 9.0 : 16.0 / 10.0 }
}

public struct ScreenSpec: Codable, Sendable, Equatable {
    public var name: String
    public var role: Role
    public var width: Int
    public var height: Int
    public var refreshRate: Double

    public init(name: String, role: Role, width: Int, height: Int, refreshRate: Double) {
        self.name = name
        self.role = role
        self.width = width
        self.height = height
        self.refreshRate = refreshRate
    }
}

public enum Layout {
    public static let macroblock = 16

    public static func standard(
        pixelsPerDegree ppd: Double,
        roles: [Role] = [.code, .chat, .email, .video],
        refreshRate: Double = 60
    ) -> [ScreenSpec] {
        roles.enumerated().map { i, role in
            let width = roundToMacroblock(role.widthDegrees * ppd / role.headsetPixelsPerPoint)
            return ScreenSpec(
                name: "skyscreens \(i + 1) · \(role.rawValue)",
                role: role,
                width: width,
                height: roundToMacroblock(Double(width) / role.aspect),
                refreshRate: refreshRate
            )
        }
    }

    static func roundToMacroblock(_ v: Double) -> Int {
        max(macroblock, Int((v / Double(macroblock)).rounded()) * macroblock)
    }
}
