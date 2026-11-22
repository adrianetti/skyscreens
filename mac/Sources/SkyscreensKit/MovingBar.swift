import AppKit
import QuartzCore

extension NSScreen {
    @MainActor
    static func with(displayID id: CGDirectDisplayID) -> NSScreen? {
        let key = NSDeviceDescriptionKey("NSScreenNumber")
        return screens.first { ($0.deviceDescription[key] as? CGDirectDisplayID) == id }
    }

    @MainActor
    static func whenAvailable(
        _ id: CGDirectDisplayID, tries: Int = 60, _ then: @escaping @MainActor (NSScreen?) -> Void
    ) {
        if let screen = with(displayID: id) {
            then(screen)
            return
        }
        guard tries > 0 else {
            then(nil)
            return
        }
        Task { @MainActor in
            try? await Task.sleep(nanoseconds: 50_000_000)
            whenAvailable(id, tries: tries - 1, then)
        }
    }
}

@MainActor
final class MovingBar {
    static let pointsPerSecond = 800.0
    static let tick: TimeInterval = 1.0 / 120
    static let barSize = CGSize(width: 40, height: 300)

    private var window: NSWindow?
    private var timer: Timer?
    private let bar = CALayer()
    private var start: CFTimeInterval = 0
    private var travel: CGFloat = 1

    func show(on screen: NSScreen) {
        let window = NSWindow(contentRect: screen.frame, styleMask: .borderless, backing: .buffered, defer: false)
        window.setFrame(screen.frame, display: false)
        window.level = .statusBar
        window.backgroundColor = .black
        let root = CALayer()
        root.backgroundColor = NSColor.black.cgColor
        bar.frame = CGRect(origin: CGPoint(x: 0, y: screen.frame.height / 3), size: Self.barSize)
        bar.backgroundColor = NSColor.systemBlue.cgColor
        root.addSublayer(bar)
        let view = NSView(frame: NSRect(origin: .zero, size: screen.frame.size))
        view.layer = root
        view.wantsLayer = true
        window.contentView = view
        window.orderFrontRegardless()
        self.window = window
        start = CACurrentMediaTime()
        travel = max(1, screen.frame.width - Self.barSize.width)
        let timer = Timer(timeInterval: Self.tick, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.advance() }
        }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    private func advance() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        let distance = (CACurrentMediaTime() - start) * Self.pointsPerSecond
        bar.frame.origin.x = distance.truncatingRemainder(dividingBy: travel)
        CATransaction.commit()
    }
}
