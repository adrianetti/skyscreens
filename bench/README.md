# bench

The measurements and simulations behind [docs/architecture.md](../docs/architecture.md). Run them on a Mac with Apple Silicon. Results from a MacBook Air M2 are in the architecture doc.

| File | What it measures | Run |
|---|---|---|
| `virtual_displays.swift` | How many virtual displays, which sizes and refresh rates macOS accepts | `swiftc -O -import-objc-header CGVirtualDisplay.h virtual_displays.swift -o vd && ./vd 4 2560 1440 120 0 101` (count, width, height, Hz, HiDPI 0/1, serial base) |
| `display_lifecycle.swift` | Adding displays later and parking one by mirroring | `swiftc -O -import-objc-header CGVirtualDisplay.h display_lifecycle.swift -o lc && ./lc` |
| `encode_codecs.swift` | H.264 vs HEVC, low-latency mode, frame sizes, quality, 4 screens at once | `swiftc -O -swift-version 5 encode_codecs.swift -o codecs && ./codecs all` |
| `encode_latency.swift` | Encode latency vs frame size, realistic concurrency, LZ4 lossless lane | `swiftc -O -swift-version 5 encode_latency.swift -o latency && ./latency` |
| `sharpness.swift` | Text sharpness through the headset compositor vs capture scale | `swiftc -O -swift-version 5 sharpness.swift -o sharp && ./sharp` |
| `netsim.py` | Packet-level simulation of transport designs on USB, router and hotspot | `python3 netsim.py params_1x.json` |

The virtual display tests make your screen flicker for a few seconds while displays are added and removed. They use `CGVirtualDisplay`, a private macOS API.

`params.json` uses 2560×1440 screens and `params_1x.json` uses headset-sized screens. Both come from the encoder benchmarks. `results_encode_codecs.txt` is the raw output of `encode_codecs.swift` on the M2.

## Notes

- `netsim.py` describes each link by bandwidth in Mbps (`bw`), one-way base delay in ms (`base`), mean jitter in ms (`jit`), random loss (`loss`), burst loss as enter, stay and in-burst loss probabilities (`burst`), stalls per second (`spikes`), and stall length range in ms (`spike`). Once motion stops, it refreshes the screen losslessly in the background at the lowest priority.
- The encoder benchmarks scroll content that repeats every 24 lines, so the loop is seamless, and add light per-frame noise to the video content, like real camera footage. The typing patch is the changed line around the caret, 320×24 pixels.
- `sharpness.swift` compares capture scale (source pixels per point) and compositor filtering against an ideal rendering made at headset resolution, under small head movements.
