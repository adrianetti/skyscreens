# AGENTS.md

This guide is for AI coding agents, and for anyone else, working on skyscreens. Read it before changing code. Then read [docs/architecture.md](docs/architecture.md), at least "Key findings" and "Phases".

## The project in one paragraph

skyscreens gives a Mac extra monitors inside a Meta Quest 3. A Mac helper creates virtual displays with the private `CGVirtualDisplay` API. The Mac app captures them with ScreenCaptureKit, encodes them with VideoToolbox, and streams them through the Rust `link` crate. A native headset app (Rust, OpenXR, Vulkan, MediaCodec, no engine) shows each display as an OpenXR compositor layer over passthrough. The current state is Phase 1 of 5: one screen over USB. It builds, and its first device test is pending. Nothing is released.

## Map

| Path | Language | Contents |
|---|---|---|
| `mac/Sources/skyscreens/main.swift` | Swift | CLI entry: `on`, `off`, `status`, `probe [hz]`, `stream`, `permission` |
| `mac/Sources/skyscreens-displays/main.swift` | Swift | Helper process that owns the virtual displays (`--ppd`, `--hz`, `--count`) |
| `mac/Sources/SkyscreensKit/` | Swift | `Session` (helper lifecycle), `VirtualDisplays`, `Layout` (screen sizes), `ScreenCapture`, `VideoEncoder`, `LosslessTile`, `Probe`, `Streamer` (Phase 1) |
| `mac/Sources/CVirtualDisplay/` | Obj-C header | Declarations for the private `CGVirtualDisplay` API |
| `mac/Sources/CLink/include/link.h` | C header | The C interface of `crates/link`, linked from `target/release/liblink.a` |
| `mac/scripts/bundle.sh` | zsh | Builds `liblink.a`, then the Swift package, then `mac/.build/Skyscreens.app`, and signs it |
| `mac/scripts/run.sh` | zsh | Runs a CLI command through the app bundle and shows its output |
| `crates/link/src/` | Rust | `message.rs` (wire format), `sender.rs` (priority fragment scheduler), `receiver.rs` (reassembly), `ffi.rs` (C ABI for the Mac) |
| `xr/src/app.rs` | Rust | OpenXR session, frame loop, quad layers, the surface swapchain |
| `xr/src/net.rs`, `xr/src/decoder.rs` | Rust | Connection to the Mac, and MediaCodec decoding into the layer's surface |
| `xr/src/platform.rs` | Rust | Every vendor-specific OpenXR extension, detected at startup |
| `xr/src/gpu.rs`, `xr/src/pattern.rs` | Rust | Vulkan upload helper, and the Phase 0 test screen |
| `xr/android/AndroidManifest.xml`, `xr/build.sh` | XML, zsh | Quest manifest; the APK build without Gradle |
| `bench/` | Swift, Python | Measurements and simulations behind the design. Not part of the apps |

## Commands

Run everything from the repo root. The [development guide](docs/development.md) explains how to install the toolchains.

| Task | Command | Needs |
|---|---|---|
| Protocol tests | `cargo test -p link` | Rust |
| Build the Mac app | `mac/scripts/bundle.sh` | Xcode tools, Rust |
| Run a Mac command | `mac/scripts/run.sh <command>` | the built app |
| Compile-check the Mac code only | `cargo build --release -p link && (cd mac && swift build -c release)` | Xcode tools, Rust |
| Build the headset APK | `xr/build.sh` | Android SDK + NDK 27, JDK 17, `aarch64-linux-android` target |
| Build and install on the Quest | `xr/build.sh --install` | a Quest connected over adb |
| Headset logs | `adb logcat -s skyscreens:V` | adb |

Rust comes from Homebrew's keg-only `rustup`. The scripts add `/opt/homebrew/opt/rustup/bin` and `~/.cargo/bin` to `PATH` themselves. Your shell may not, so export that `PATH` before running `cargo` yourself.

Before you call a change done, run `cargo test -p link`, the Mac compile check and `xr/build.sh`. All three must pass. There is no CI yet.

## Rules

**Mac**
- Launch the app through the bundle (`mac/scripts/run.sh` or `open -n mac/.build/Skyscreens.app --args …`), never `.build/release/skyscreens` directly. macOS grants Screen Recording to the bundle, not to a raw binary.
- Don't run `bundle.sh` without a reason on a machine with ad-hoc signing. Each rebuild makes macOS forget the Screen Recording grant. `bundle.sh` prints a warning when it signs ad-hoc. The fix is in [Code signing](docs/development.md#code-signing).
- Virtual displays: create all of them once, up front, in the helper process. Creating a display after one was destroyed in the same process gives a dead display. Hide and show screens by mirroring (`park` and `unpark`), and change modes in place with `apply()`.
- Only `skyscreens-displays` owns displays. When it exits, macOS moves the windows back to the laptop, which is the intended safe failure.

**Protocol (`crates/link`)**
- The Mac and the headset share this crate, so they can't disagree about the format. Change messages in `message.rs` only, add a round-trip test, and bump `PROTOCOL_VERSION` on any incompatible change.
- `ffi.rs` and `mac/Sources/CLink/include/link.h` must match exactly. Change both in the same commit.
- The Mac server binds `127.0.0.1` only. Keep it that way until pairing and encryption land (Phase 3). See [SECURITY.md](SECURITY.md).
- The cursor and control messages must never wait behind big frames. Use `Priority` and `Sender::replace` for "latest wins" streams.

**Headset (`xr/`)**
- Use plain OpenXR and Vulkan. Anything vendor-specific belongs in `platform.rs`, enabled only when the runtime reports the extension. Never branch on the device name.
- No engine, no Gradle, no Java code. The manifest sets `android:hasCode="false"`.
- Video goes from the network thread straight into the decoder, never through the frame loop.

**General**
- Measure, don't assume. Latency claims need a number from `skyscreens stream`, `skyscreens probe`, or `bench/`.
- Keep the code small and direct. Comments explain why, not what.
- Commit messages: one line in Conventional Commits style, such as `feat(xr): …`, `fix(mac): …` or `docs: …`.

## Testing on the device

1. `adb devices` must list the Quest as `device`. If it shows `unauthorized`, accept the USB debugging prompt inside the headset.
2. Build and install: `mac/scripts/bundle.sh`, then `xr/build.sh --install`.
3. Check the permission: `mac/scripts/run.sh permission` must print `screen recording: allowed`.
4. Stream: `mac/scripts/run.sh stream`. It sets up `adb reverse tcp:7243 tcp:7243`, launches the headset app, and prints one line per second: fps, Mbps, encode time, and the capture-to-decoded round trip.
5. Watch the headset side with `adb logcat -s skyscreens:V`.
6. To run without anyone wearing the headset, send `adb shell am broadcast -a com.oculus.vrpowermanager.prox_close`. Restore worn detection afterwards with `adb shell am broadcast -a com.oculus.vrpowermanager.automation_disable`.

## Pitfalls already found

| Symptom | Cause and fix |
|---|---|
| `permission` prints "not allowed yet" although System Settings shows Skyscreens on | The grant belongs to an older ad-hoc build. Remove Skyscreens from the list with the minus button and run `permission` again. Stable signing stops this: [Code signing](docs/development.md#code-signing) |
| Capture fails with error -3801 | Same as above: Screen Recording isn't granted to this build |
| Quest session stays IDLE forever | OpenXR needs the Android *Activity*, but `android-activity` puts the *Application* into `ndk_context`. `xr/src/app.rs` re-initializes `ndk_context` with `android.activity_as_ptr()` |
| Android says the app isn't responding | Input events must be drained every frame (`xr/src/app.rs`) |
| Session misbehaves right after launch | The session must be created after `MainEvent::InitWindow`, as `xr/src/app.rs` does. The manifest also needs `vr_only`, `focusaware` and a fullscreen theme |
| No `B8G8R8A8` swapchain format | The Quest doesn't offer it. The GPU path (Phase 2) must swizzle BGRA to RGBA |
| Surface swapchain creation fails | Format, samples, faces, array size and mips must all be 0 for `xrCreateSwapchainAndroidSurfaceKHR` |
| MediaCodec `configure` fails | It may reject `vendor.qti-ext-dec-low-latency.enable` or `operating-rate`. Drop them in `xr/src/decoder.rs` and retry |
| `sdkmanager` finishes but installs nothing | Use the `android` tool from the same folder (see the development guide) |

## Where things are going

Phase 2 replaces the surface swapchain with a GPU path, because surface swapchains have no GPU access and no mipmaps. MediaCodec will decode into `AImageReader`, and a Vulkan copy will move each frame into a persistent texture per screen. LZ4 lossless tiles will be uploaded into the same texture. Phase 2 also adds a cursor layer drawn on the headset, 3–4 screens, and one-shortcut On/Off. Phase 3 adds UDP with FEC, resend and LTR recovery, plus Noise encryption. The details and their reasons are in [docs/architecture.md](docs/architecture.md).
