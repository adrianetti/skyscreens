import CoreGraphics
import Foundation
import SkyscreensKit

setvbuf(stdout, nil, _IOLBF, 0)

let args = CommandLine.arguments

func option(_ name: String) -> String? {
    guard let i = args.firstIndex(of: name), i + 1 < args.count else { return nil }
    return args[i + 1]
}

func exitOnSignalsAndParentExit() {
    for sig in [SIGTERM, SIGINT, SIGHUP] {
        signal(sig, SIG_IGN)
        let source = DispatchSource.makeSignalSource(signal: sig, queue: .main)
        source.setEventHandler { exit(0) }
        source.resume()
        _ = Unmanaged.passRetained(source)
    }
    guard let parent = option("--parent").flatMap(pid_t.init) else { return }
    let source = DispatchSource.makeProcessSource(identifier: parent, eventMask: .exit, queue: .main)
    source.setEventHandler { exit(0) }
    source.resume()
    _ = Unmanaged.passRetained(source)
}

struct Started: Encodable {
    struct Screen: Encodable {
        let id: CGDirectDisplayID
        let spec: ScreenSpec
    }
    let pid: Int32
    let screens: [Screen]
}

let ppd = option("--ppd").flatMap(Double.init) ?? 25
let hz = option("--hz").flatMap(Double.init) ?? 60
let count = option("--count").flatMap(Int.init) ?? 4
let specs = Array(Layout.standard(pixelsPerDegree: ppd, refreshRate: hz).prefix(count))

let set: VirtualDisplaySet
do {
    set = try VirtualDisplaySet(specs)
    try set.waitUntilReady()
    let started = Started(pid: getpid(), screens: zip(set.ids, specs).map { .init(id: $0, spec: $1) })
    print(String(bytes: try JSONEncoder().encode(started), encoding: .utf8) ?? "")
} catch {
    FileHandle.standardError.write(Data("skyscreens-displays: \(error)\n".utf8))
    exit(1)
}

exitOnSignalsAndParentExit()
withExtendedLifetime(set) { RunLoop.main.run() }
