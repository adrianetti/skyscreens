// skyscreens command line: `on`, `off`, `status`, `probe`.
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
default:
    print("""
    usage: skyscreens on        create the virtual screens
           skyscreens off       remove them; windows move back to the laptop
           skyscreens status    show the running screens
           skyscreens probe [hz]  measure capture and encode latency on a test screen
    """)
}
