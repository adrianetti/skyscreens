import CoreGraphics
import Foundation
import SkyscreensKit

let usage = """
    usage: skyscreens on                 create the virtual screens
           skyscreens off                remove them; windows move back to the laptop
           skyscreens status             show the running screens
           skyscreens probe [hz]         measure capture and encode latency on a test screen
           skyscreens stream [options]   stream one screen to the Quest over USB
               --seconds N               stop after N seconds and print a JSON summary
               --no-headset              skip adb; wait for any client on port \(Streamer.port)
               --test-pattern            animate a bar on the streamed screen, for measurements
           skyscreens permission         ask macOS for Screen Recording access
    """

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("skyscreens: \(message)\n".utf8))
    exit(1)
}

let args = Array(CommandLine.arguments.dropFirst())

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
    do {
        let options = try StreamOptions(arguments: Array(args.dropFirst()))
        try Streamer(options: options).run()
    } catch { fail("\(error)") }
case "permission":
    if CGPreflightScreenCaptureAccess() || CGRequestScreenCaptureAccess() {
        print("screen recording: allowed")
    } else {
        print(
            """
            screen recording: not allowed yet
            If System Settings already shows Skyscreens switched on, that switch belongs to an older build.
            Select Skyscreens there, remove it with the minus button, then run this command again.
            """
        )
        exit(1)
    }
default:
    print(usage)
}
