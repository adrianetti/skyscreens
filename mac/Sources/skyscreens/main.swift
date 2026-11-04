// skyscreens command line: `on`, `off`, `status`, `probe`.
import CoreGraphics
import Foundation
import SkyscreensKit

let args = Array(CommandLine.arguments.dropFirst())

func fail(_ message: String) -> Never {
    FileHandle.standardError.write("skyscreens: \(message)\n".data(using: .utf8)!)
    exit(1)
}

switch args.first {
case "on":
    do { print(try Session.on()) } catch { fail("\(error)") }
case "off":
    print(Session.off() ? "off" : "already off")
case "status":
    print(Session.status() ?? "off")
case "probe":
    Probe.run(hz: args.dropFirst().first.flatMap(Double.init) ?? 60)
case "stream":
    do { try Streamer().run() } catch { fail("\(error)") }
case "permission":
    // Shows the system prompt the first time and adds Skyscreens to the Screen Recording list.
    if CGPreflightScreenCaptureAccess() || CGRequestScreenCaptureAccess() {
        print("screen recording: allowed")
    } else {
        print("""
        screen recording: not allowed yet
        If System Settings already shows Skyscreens switched on, that switch belongs to an older build.
        Select Skyscreens there, remove it with the minus button, then run this command again.
        """)
    }
default:
    print("""
    usage: skyscreens on        create the virtual screens
           skyscreens off       remove them; windows move back to the laptop
           skyscreens status    show the running screens
           skyscreens probe [hz]  measure capture and encode latency on a test screen
           skyscreens stream    stream one screen to the Quest over USB (Phase 1)
           skyscreens permission  ask macOS for Screen Recording access
    """)
}
