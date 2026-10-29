// Owns the virtual displays for one session.
//
// It is kept tiny so it never crashes: the displays exist exactly as long as this process.
// When it exits, for any reason, macOS moves the windows back to the laptop screen.
// On start it prints one JSON line describing the displays, then waits for SIGTERM.
import CoreGraphics
import Foundation
import SkyscreensKit

setvbuf(stdout, nil, _IOLBF, 0)

let args = CommandLine.arguments
func option(_ name: String) -> String? { args.firstIndex(of: name).flatMap { $0 + 1 < args.count ? args[$0 + 1] : nil } }

let ppd = option("--ppd").flatMap(Double.init) ?? 25
let hz = option("--hz").flatMap(Double.init) ?? 60
let specs = Layout.standard(pixelsPerDegree: ppd, refreshRate: hz)

struct Started: Encodable {
    struct Screen: Encodable { let id: CGDirectDisplayID; let spec: ScreenSpec }
    let pid: Int32
    let screens: [Screen]
}

let set: VirtualDisplaySet
do {
    set = try VirtualDisplaySet(specs)
    try set.waitUntilReady()
} catch {
    FileHandle.standardError.write("skyscreens-displays: \(error)\n".data(using: .utf8)!)
    exit(1)
}

let started = Started(pid: getpid(), screens: zip(set.ids, specs).map { .init(id: $0, spec: $1) })
print(String(data: try JSONEncoder().encode(started), encoding: .utf8)!)

for sig in [SIGTERM, SIGINT, SIGHUP] {
    signal(sig, SIG_IGN)
    let source = DispatchSource.makeSignalSource(signal: sig, queue: .main)
    source.setEventHandler { exit(0) }
    source.resume()
    _ = Unmanaged.passRetained(source)
}

// Exit with the supervising process, if one was named.
if let parent = option("--parent").flatMap(pid_t.init) {
    let source = DispatchSource.makeProcessSource(identifier: parent, eventMask: .exit, queue: .main)
    source.setEventHandler { exit(0) }
    source.resume()
    _ = Unmanaged.passRetained(source)
}

withExtendedLifetime(set) { RunLoop.main.run() }
