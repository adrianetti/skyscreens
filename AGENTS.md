# AGENTS.md

This guide is for AI coding agents, and for anyone else, working on skyscreens. Read it before changing code. Then read [docs/architecture.md](docs/architecture.md), at least "Key findings" and "Phases".

## The project in one paragraph

skyscreens gives a Mac extra monitors inside a Meta Quest 3. A Mac helper creates virtual displays with the private `CGVirtualDisplay` API. The Mac app captures them with ScreenCaptureKit, encodes them with VideoToolbox, and streams them through the Rust `link` crate. A native headset app (Rust, OpenXR, Vulkan, MediaCodec, no engine) shows each display as an OpenXR compositor layer over passthrough. The current state is Phase 1 of 5: one screen over USB. The Mac half passes its end-to-end eval. The first test on the headset is pending. Nothing is released.

## Map

| Path | Language | Contents |
|---|---|---|
| `mac/Sources/skyscreens/main.swift` | Swift | CLI entry: `on`, `off`, `status`, `probe [hz]`, `stream [--seconds N] [--no-headset] [--test-pattern]`, `permission` |
| `mac/Sources/skyscreens-displays/main.swift` | Swift | Helper process that owns the virtual displays (`--ppd`, `--hz`, `--count`) |
| `mac/Sources/SkyscreensKit/` | Swift | `Session` (helper lifecycle), `VirtualDisplays`, `Layout`, `ScreenCapture`, `VideoEncoder`, `LosslessTile`, `Probe`, `Streamer`, `StreamOptions`, `StreamStats`, `MovingBar`, `Adb` |
| `mac/Tests/SkyscreensKitTests/` | Swift | Swift Testing suites. The encoder suite runs on the real hardware encoder and skips where there is none |
| `mac/Sources/CVirtualDisplay/` | Obj-C header | Declarations for the private `CGVirtualDisplay` API |
| `mac/Sources/CLink/include/link.h` | C header | Generated from `crates/link/src/ffi.rs` by cbindgen. Never edit it by hand |
| `mac/scripts/bundle.sh`, `run.sh` | bash | Build and sign `Skyscreens.app`; run a CLI command through the bundle |
| `crates/link/src/` | Rust | `message.rs` (wire format), `sender.rs` (priority fragment scheduler), `receiver.rs` (reassembly), `channel.rs` (channel numbers), `annexb.rs` (H.264 framing), `server.rs` (Mac endpoint), `ffi.rs` (C ABI over `server.rs`) |
| `crates/link/tests/properties.rs` | Rust | Seeded property tests for the protocol |
| `crates/fake-headset/` | Rust | Headless stand-in for the headset, used by the loopback tests and `scripts/eval.sh mac` |
| `xr/src/` | Rust | `app.rs` (session, test screen, video layer, frame loop), `runtime.rs` (OpenXR and Vulkan bootstrap), `android.rs`, `net.rs`, `decoder.rs`, `gpu.rs`, `platform.rs`, `layout.rs`, `pattern.rs` |
| `xr/android/AndroidManifest.xml`, `xr/build.sh` | XML, bash | Quest manifest; the APK build without Gradle |
| `scripts/` | bash, Python | `check.sh` (all checks), `eval.sh` (end-to-end evals), `comments.py`, `commit_msg.py`, `android-env.sh`, git hooks |
| `.github/workflows/` | YAML | `ci.yml` (runs `scripts/check.sh`), `claude.yml` (`@claude` mentions), `claude-review.yml` (PR review) |
| `bench/` | Swift, Python | Measurements and simulations behind the design. Not part of the apps |

## Commands

Run everything from the repo root. The [development guide](docs/development.md) explains how to install the toolchains.

| Task | Command | Needs |
|---|---|---|
| Every check (the bar for "done") | `scripts/check.sh` | all toolchains |
| Quick checks while working | `scripts/check.sh fast` | Rust, Swift, Python |
| One area | `scripts/check.sh rust`, `swift`, `android` or `scripts` | that area's toolchain |
| Apply formatters and regenerate `link.h` | `scripts/check.sh fix` | all formatters, cbindgen |
| Mac pipeline end to end, no headset | `scripts/eval.sh mac` | the Mac's Screen Recording grant |
| Full pipeline on the Quest | `scripts/eval.sh quest` | a Quest on USB |
| Enable the git hooks | `scripts/install-hooks.sh` | |
| Build the Mac app | `mac/scripts/bundle.sh` | Xcode, Rust |
| Run a Mac command | `mac/scripts/run.sh <command>` | the built app |
| Build and install on the Quest | `xr/build.sh --install` | Android SDK, NDK 27, JDK 17 |
| Headset logs | `adb logcat -s skyscreens:V` | adb |

Rust comes from Homebrew's keg-only `rustup`, pinned by `rust-toolchain.toml`. The scripts add `/opt/homebrew/opt/rustup/bin` and `~/.cargo/bin` to `PATH` themselves. Your shell may not, so export that `PATH` before running `cargo` yourself.

## Definition of done

1. `scripts/check.sh` passes. CI runs the same script, so a green local run means a green CI run.
2. Every behavior change comes with a test that fails without it.
3. A change to capture, encoding, the streamer or the protocol also passes `scripts/eval.sh mac`. Put its JSON summary in the pull request.
4. A change to the headset app also passes `scripts/eval.sh quest` when a Quest is connected. If none is connected, say so in the pull request.
5. The docs match the code: this file, `docs/`, the README and `llms.txt`.

Report what you ran and what you could not run. Never claim a device result you did not measure.

## Evals

`scripts/eval.sh` writes everything to `target/eval/` and prints one JSON report. It exits non-zero on failure.

- **`mac`**: builds the app, checks the Screen Recording grant, and streams a moving test pattern for 8 seconds to `fake-headset`. It fails if any of these is wrong:
  - the screen list, with its sizes;
  - the first frame, which must be a decodable keyframe;
  - every keyframe, which must carry SPS, PPS and IDR;
  - the reply to a keyframe request;
  - fps, which must be ≥ 30;
  - round-trip p50, which must be ≤ 60 ms;
  - cleanup: the virtual screen must be gone afterwards.

  Reference run, MacBook Air M2: 57 fps, round trip p50 8.8 ms, keyframe request answered in 8 ms.
- **`quest`**: builds and installs both apps and keeps the headset awake without anyone wearing it. Then it streams the test pattern through `adb reverse`. It checks the stream summary and the headset log, and saves a headset screenshot when the device allows it.
- `EVAL_SECONDS`, `EVAL_MIN_FPS`, `EVAL_MAX_ROUND_TRIP_MS` and `EVAL_SKIP_BUILD=1` adjust a run.

## Code standards

These hold for Rust, Swift, C, shell and Python. The checks enforce every rule they can.

**Structure**
- One responsibility per module or type. Split a file before it mixes concerns. Clippy rejects functions over 80 lines.
- Keep platform code at the edges. Pure logic (protocol, layout, framing, stats, option parsing) lives in plain modules with no OS calls, so it can be tested anywhere.
- Inject what you need to test: a `TcpListener` rather than a port, options rather than globals, a closure rather than a callback type.
- Name constants. No magic numbers in logic: channels live in `link::channel`, limits in `link`, tuning values in named constants next to their use.

**No comments**
- Source files contain no comments or doc comments. `scripts/comments.py` fails the build on any. Shebangs and the `swift-tools-version` line are the only exceptions.
- Explain intent with names, types and tests instead. A function named `drain_input_to_stay_responsive` replaces a comment.
- Write down design reasons in [docs/architecture.md](docs/architecture.md), and traps in "Pitfalls already found" below. To silence a lint, use `#[expect(lint, reason = "…")]`; `#[allow]` is rejected.

**Errors and safety**
- Return errors; don't panic. Panics are only for broken invariants in constructors and tests. Poisoned mutexes are recovered with `PoisonError::into_inner`.
- No force unwraps, `try!` or implicitly unwrapped optionals in Swift (swift-format rules). No `unwrap` outside tests in new Rust code where an error can be returned.
- Keep `unsafe` small and behind a safe function: `handle()` and `slice()` in `ffi.rs` are the pattern. FFI functions check their pointers for null.
- Bound everything that comes off the network: fragment size, message size, screen count.

**Formatting and lints**
- Rust: rustfmt (`rustfmt.toml`), clippy `all` and `pedantic` as errors, edition 2024, toolchain pinned in `rust-toolchain.toml`.
- Swift: `swift format` in strict mode (`.swift-format`), compiled with warnings as errors.
- Python: ruff format and ruff lint (`ruff.toml`). Shell: bash with `set -euo pipefail`, shellcheck with all optional checks (`.shellcheckrc`).
- Dependencies: `cargo deny` checks licenses, advisories and sources (`deny.toml`). Add a dependency only when it removes real code or risk.

**Tests**
- Name a test after the behavior it proves: `a_missing_fragment_drops_only_that_message`.
- Tests are deterministic. Property tests use a fixed seed, network tests bind port 0, and waits poll with a deadline instead of sleeping a fixed time.
- Hardware-dependent tests skip themselves where the hardware is missing (`.enabled(if:)` in Swift) and run everywhere else.

## Rules

**Mac**
- Launch the app through the bundle (`mac/scripts/run.sh` or `open -n mac/.build/Skyscreens.app --args …`), never `.build/release/skyscreens` directly. macOS grants Screen Recording to the bundle, not to a raw binary.
- Sign with a stable identity. With one (`Skyscreens Dev` or Apple Development), the grant survives rebuilds and `scripts/eval.sh mac` runs unattended. With ad-hoc signing every rebuild drops it. See [Code signing](docs/development.md#code-signing).
- Virtual displays: create all of them once, up front, in the helper process. Creating a display after one was destroyed in the same process gives a dead display. Hide and show screens by mirroring (`park` and `unpark`), and change modes in place with `apply()`.
- Only `skyscreens-displays` owns displays. When it exits, macOS moves the windows back to the laptop, which is the intended safe failure.

**Protocol (`crates/link`)**
- The Mac and the headset share this crate, so they can't disagree about the format. Change messages in `message.rs` only, add a round-trip test, and bump `PROTOCOL_VERSION` on any incompatible change. `the_wire_format_is_stable` pins the bytes.
- `link.h` is generated. After changing `ffi.rs`, run `scripts/check.sh fix`; the check fails while the header is stale. Constants shared with Swift (`SK_*`) are asserted against their Rust sources at compile time.
- The Mac server binds `127.0.0.1` only. Keep it that way until pairing and encryption land (Phase 3). See [SECURITY.md](SECURITY.md).
- The cursor and control messages must never wait behind big frames. Use `Priority` and `Sender::replace` for "latest wins" streams.

**Headset (`xr/`)**
- Use plain OpenXR and Vulkan. Anything vendor-specific belongs in `platform.rs`, enabled only when the runtime reports the extension. Never branch on the device name.
- No engine, no Gradle, no Java code. The manifest sets `android:hasCode="false"`.
- Video goes from the network thread straight into the decoder, never through the frame loop.
- Stop the decoder before destroying the surface it draws into (`Shared::detach_window`).

**General**
- Measure, don't assume. Latency claims need a number from `scripts/eval.sh`, `skyscreens stream`, `skyscreens probe`, or `bench/`.
- Commit messages: one line in Conventional Commits style, such as `feat(xr): …`, `fix(mac): …` or `docs: …`, at most 100 characters. The `commit-msg` hook and CI check it.

## Testing on the device

1. `adb devices` must list the Quest as `device`. If it shows `unauthorized`, accept the USB debugging prompt inside the headset.
2. Run `scripts/eval.sh quest`. It builds and installs both apps, keeps the headset awake, streams the test pattern, and checks both sides.
3. For manual runs: `mac/scripts/run.sh stream` sets up `adb reverse tcp:7243 tcp:7243`, launches the headset app, and prints one line per second. Watch the headset with `adb logcat -s skyscreens:V`.
4. To run without anyone wearing the headset, send `adb shell am broadcast -a com.oculus.vrpowermanager.prox_close`. Restore worn detection afterwards with `adb shell am broadcast -a com.oculus.vrpowermanager.automation_disable`. The eval does both.

## Pitfalls already found

| Symptom | Cause and fix |
|---|---|
| `permission` prints "not allowed yet" although System Settings shows Skyscreens on | The grant belongs to an older ad-hoc build. Remove Skyscreens from the list with the minus button and run `permission` again. Stable signing stops this: [Code signing](docs/development.md#code-signing) |
| Capture fails with error -3801 | Same as above: Screen Recording isn't granted to this build |
| Quest session stays IDLE forever | OpenXR needs the Android *Activity*, but `android-activity` puts the *Application* into `ndk_context`. `android::use_activity_as_context` re-initializes it with `activity_as_ptr()` |
| Android says the app isn't responding | Input events must be drained every frame (`android::pump_events`) |
| Session misbehaves right after launch | The session must be created after `MainEvent::InitWindow` (`android::wait_for_window`). The manifest also needs `vr_only`, `focusaware` and a fullscreen theme |
| No `B8G8R8A8` swapchain format | The Quest doesn't offer it. The GPU path (Phase 2) must swizzle BGRA to RGBA |
| Surface swapchain creation fails | Format, samples, faces, array size and mips must all be 0 for `xrCreateSwapchainAndroidSurfaceKHR` |
| MediaCodec `configure` fails | It may reject `vendor.qti-ext-dec-low-latency.enable` or `operating-rate`. Drop them from `LOW_LATENCY_OPTIONS` in `xr/src/decoder.rs` and retry. They are the low-latency settings ALVR uses on Quest |
| A static virtual screen sends almost no frames | ScreenCaptureKit only delivers changed frames. Use `stream --test-pattern` to measure fps and latency |
| The Vulkan or OpenXR loader disappears mid-run | `ash::Entry` and `xr::Entry` unload the library when dropped. `runtime.rs` keeps both alive in `Runtime` and `Vulkan` |
| `sdkmanager` finishes but installs nothing | Use the `android` tool from the same folder (see the development guide) |
| Swift build fails in CI but not locally | Call arguments with a trailing comma need Swift 6.1. Keep `.swift-format` at `keptAsWritten` |

## Where things are going

Phase 2 replaces the surface swapchain with a GPU path, because surface swapchains have no GPU access and no mipmaps. MediaCodec will decode into `AImageReader`, and a Vulkan copy will move each frame into a persistent texture per screen. LZ4 lossless tiles will be uploaded into the same texture. Phase 2 also adds a cursor layer drawn on the headset, 3–4 screens, and one-shortcut On/Off. Phase 3 adds UDP with FEC, resend and LTR recovery, plus Noise encryption. The details and their reasons are in [docs/architecture.md](docs/architecture.md).
