# skyscreens

Extra monitors for your Mac, inside a Meta Quest 3.

skyscreens creates virtual displays on your Mac and streams them to the headset, where they float in your room like real monitors. macOS treats them as real displays, so your keyboard, trackpad and cursor work across them as usual.

> **Status:** early development. Nothing to install yet.

## Goals

- **Close to a real monitor:** sharp text, low latency, stable connection.
- **Easy setup:** no account, no sign-in. Open the app on both devices, or plug in a cable.
- **Works on poor networks:** usable over a phone hotspot, best over USB-C.

## How it works

**Mac app (Swift)**

- Creates virtual displays with `CGVirtualDisplay`.
- Captures them with ScreenCaptureKit, only when the content changes.
- Encodes with the VideoToolbox hardware HEVC encoder in low-latency mode.
- Finds the headset on the local network with Bonjour.

**Quest app (Kotlin, Meta Spatial SDK)**

- Decodes with MediaCodec.
- Shows each screen as an OpenXR compositor layer, which keeps text sharp.
- Uses passthrough, so you can still see your real keyboard.

**Connection:** USB-C through `adb forward`, or Wi-Fi.

## Roadmap

1. One virtual screen over USB-C.
2. Wi-Fi and hotspot mode with adaptive bitrate.
3. Multiple screens, layouts and polish.

## Requirements

- Apple Silicon Mac.
- Meta Quest 3 with Developer Mode turned on.

`CGVirtualDisplay` is a private macOS API, so the Mac app cannot ship on the Mac App Store and a future macOS update could break it.

## License

[MIT](LICENSE)

skyscreens is not affiliated with or endorsed by Meta or Apple. Meta Quest is a trademark of Meta Platforms, Inc. Mac is a trademark of Apple Inc.
