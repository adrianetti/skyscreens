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

- Creates virtual displays with `CGVirtualDisplay`, sized to the headset's resolution.
- Captures them with ScreenCaptureKit, only when the content changes.
- Sends small changes such as typing as lossless LZ4 tiles, and motion as low-latency H.264 from the hardware encoder.
- Sends the cursor separately, so it moves with almost no delay.

**Headset app (Rust, OpenXR + Vulkan), Quest 3 first**

- Decodes with MediaCodec and merges video and lossless tiles into one texture per screen.
- Shows each screen as an OpenXR compositor layer, which keeps text sharp.
- Uses passthrough, so you can still see your real keyboard.

**Connection (Rust, shared by both apps):** USB-C through `adb forward`, or Wi-Fi with error correction. Always encrypted.

The full design, with the measurements behind it, is in [docs/architecture.md](docs/architecture.md).

## Roadmap

0. Toolchains and an empty Quest app.
1. One virtual screen over USB-C, with a latency readout.
2. Lossless text, local cursor, 3–4 screens, one-shortcut On/Off.
3. Wi-Fi and hotspot mode, pairing and encryption.
4. Moving and resizing screens, Quest controllers and keyboard.
5. Other headsets: Android XR (Galaxy XR, XREAL Project Aura), Steam Frame, and more.

## Requirements

- Apple Silicon Mac.
- Meta Quest 3 with Developer Mode turned on.

`CGVirtualDisplay` is a private macOS API, so the Mac app cannot ship on the Mac App Store and a future macOS update could break it.

## License

[MIT](LICENSE)

skyscreens is not affiliated with or endorsed by Meta or Apple. Meta Quest is a trademark of Meta Platforms, Inc. Mac is a trademark of Apple Inc.
