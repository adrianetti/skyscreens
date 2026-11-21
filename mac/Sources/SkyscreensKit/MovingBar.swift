import AppKit
import QuartzCore

extension NSScreen {
    static func with(displayID id: CGDirectDisplayID) -> NSScreen? {
        let key = NSDeviceDescriptionKey("NSScreenNumber")
        return screens.first { ($0.deviceDescription[key] as? CGDirectDisplayID) == id }
    }

    static func whenAvailable(_ id: CGDirectDisplayID, tries: Int = 60, _ then: @escaping (NSScreen?) -> Void) {
        if let screen = with(displayID: id) { return then(screen) }
        guard tries > 0 else { return then(nil) }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { whenAvailable(id, tries: tries - 1, then) }
    }
}

final class MovingBar {
    static let pointsPerSecond = 800.0
    static let tick: TimeInterval = 1.0 / 120
    static let barSize = CGSize(width: 40, height: 300)

    private var window: NSWindow?
    private var timer: Timer?
    private let bar = CALayer()

    func show(on screen: NSScreen) {
        let w = NSWindow(contentRect: screen.frame, styleMask: .borderless, backing: .buffered, defer: false)
        w.setFrame(screen.frame, display: false)
        w.level = .statusBar
        w.backgroundColor = .black
        let root = CALayer()
        root.backgroundColor = NSColor.black.cgColor
        bar.frame = CGRect(origin: CGPoint(x: 0, y: screen.frame.height / 3), size: Self.barSize)
        bar.backgroundColor = NSColor.systemBlue.cgColor
        root.addSublayer(bar)
        let view = NSView(frame: NSRect(origin: .zero, size: screen.frame.size))
        view.layer = root
        view.wantsLayer = true
        w.contentView = view
        w.orderFrontRegardless()
        window = w
        let start = CACurrentMediaTime()
        let travel = max(1, screen.frame.width - Self.barSize.width)
        let timer = Timer(timeInterval: Self.tick, repeats: true) { [bar] _ in
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            bar.frame.origin.x = ((CACurrentMediaTime() - start) * Self.pointsPerSecond).truncatingRemainder(
                dividingBy: travel)
            CATransaction.commit()
        }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }
}
