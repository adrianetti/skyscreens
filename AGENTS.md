# AGENTS.md

This guide is for AI coding agents, and for anyone else, working on skyscreens. Read it before changing code. Then read [docs/architecture.md](docs/architecture.md), at least "Key findings" and "Phases".

## The project in one paragraph

skyscreens gives a Mac extra monitors inside a Meta Quest 3. A Mac helper creates virtual displays with the private `CGVirtualDisplay` API. The Mac app captures them with ScreenCaptureKit, encodes them with VideoToolbox, and hands each frame over a pipe to `skyscreens-link`, a Rust helper process that streams it with the `link` crate. A native headset app (Rust, OpenXR, Vulkan, MediaCodec, no engine) shows each display as an OpenXR compositor layer over passthrough. The current state is Phase 1 of 5: one screen over USB. The Mac half passes its end-to-end eval. The first test on the headset is pending. Nothing is released.

## Map

| Path | Language | Contents |
|---|---|---|
| `mac/Sources/skyscreens/main.swift` | Swift | CLI entry: `on`, `off`, `status`, `probe [hz]`, `stream [--seconds N] [--no-headset] [--test-pattern]`, `permission` |
| `mac/Sources/skyscreens-displays/main.swift` | Swift | Helper process that owns the virtual displays (`--ppd`, `--hz`, `--count`) |
| `mac/Sources/SkyscreensKit/` | Swift | `Session` (displays helper lifecycle), `VirtualDisplaySet`, `Layout`, `ScreenCapture`, `VideoEncoder`, `LosslessTile`, `PixelMemory` (the only pointer code), `Streamer` (main actor) with `FramePipeline` (capture → encode → send, off the main thread), `LinkHelper` and `BridgeCodec` (the pipe to `skyscreens-link`), `EventRelay`, `ConnectionTracker`, `StreamOptions`, `StreamStats`, `FixedFormat`, `Probe` with `ProbeRecorder`, `MovingBar`, `Adb` |
| `mac/Tests/SkyscreensKitTests/` | Swift | Swift Testing suites. The encoder suite runs on the real hardware encoder and skips where there is none |
| `mac/Sources/CVirtualDisplay/` | Obj-C header | Declarations for the private `CGVirtualDisplay` API |
| `mac/scripts/bundle.sh`, `run.sh` | bash | Build and sign `Skyscreens.app`; run a CLI command through the bundle |
| `crates/link/src/` | Rust | `message.rs` (wire format), `sender.rs` (priority fragment scheduler), `receiver.rs` (reassembly), `channel.rs` (channel numbers), `annexb.rs` (H.264 framing), `server.rs` (Mac endpoint and its events) |
| `crates/link-helper/` | Rust | `skyscreens-link`, the Mac's helper process: `bridge.rs` (the pipe protocol to Swift) and the forwarding loop. `tests/vectors.json` pins the bridge bytes for both languages |
| `crates/link/tests/properties.rs` | Rust | Seeded property tests for the protocol |
| `crates/fake-headset/` | Rust | Headless stand-in for the headset, used by the loopback tests and `scripts/eval.sh mac` |
| `xr/src/` | Rust | `app.rs` (session, test screen, video layer, frame loop), `runtime.rs` (OpenXR and Vulkan bootstrap), `android.rs`, `net.rs`, `decoder.rs`, `gpu.rs`, `platform.rs`, `layout.rs`, `pattern.rs` |
| `xr/android/AndroidManifest.xml`, `xr/build.sh` | XML, bash | Quest manifest; the APK build without Gradle |
| `scripts/` | bash, Python | `check.sh` (all checks), `eval.sh` and `eval_judge.py` (end-to-end evals), `escape_hatches.py` (the ledger), `agent_guard.py` (Claude Code hook), `limits.py`, `comments.py`, `swift_coverage.py`, `commit_msg.py`, `android-env.sh`, git hooks |
| `escape-hatches.json` | JSON | The ledger: every `unsafe`, lint `expect`, discarded result and other escape hatch, plus fingerprints of the rule files. Only a human updates it |
| `.github/CODEOWNERS`, `.claude/settings.json` | | Rule files that need the maintainer's approval; the agent guard |
| `.github/workflows/` | YAML | `ci.yml` (runs `scripts/check.sh`), `claude.yml` (`@claude` mentions), `claude-review.yml` (PR review) |
| `bench/` | Swift, Python | Measurements and simulations behind the design. Not part of the apps |

## Commands

Run everything from the repo root. The [development guide](docs/development.md) explains how to install the toolchains.

| Task | Command | Needs |
|---|---|---|
| Every check (the bar for "done") | `scripts/check.sh` | all toolchains |
| Quick checks while working | `scripts/check.sh fast` | Rust, Swift, Python |
| One area | `scripts/check.sh rust`, `swift`, `android` or `scripts` | that area's toolchain |
| Apply formatters | `scripts/check.sh fix` | all formatters |
| Mutation testing of the protocol | `scripts/check.sh mutants` | cargo-mutants |
| Mac pipeline end to end, no headset | `scripts/eval.sh mac` | the Mac's Screen Recording grant |
| Full pipeline on the Quest | `scripts/eval.sh quest` | a Quest on USB |
| Enable the git hooks | `scripts/install-hooks.sh` | |
| Build the Mac app | `mac/scripts/bundle.sh` | Xcode, Rust |
| Run a Mac command | `mac/scripts/run.sh <command>` | the built app |
| Build and install on the Quest | `xr/build.sh --install` | Android SDK, NDK 27, JDK 17 |
| Headset logs | `adb logcat -s skyscreens:V` | adb |

Rust comes from Homebrew's keg-only `rustup`, pinned by `rust-toolchain.toml`. The scripts add `/opt/homebrew/opt/rustup/bin` and `~/.cargo/bin` to `PATH` themselves. Your shell may not, so export that `PATH` before running `cargo` yourself.

## Definition of done

1. `scripts/check.sh` passes. CI runs the same script, so a green local run means a green CI run. Never weaken a check to make it pass: protected files (lint configs, gate scripts, workflows, `escape-hatches.json`) change only with the maintainer's approval. If the ledger check fails, remove the new escape hatch; if you truly need it, stop and ask.
2. Every behavior change comes with a test that fails without it.
3. A change to capture, encoding, the streamer or the protocol also passes `scripts/eval.sh mac`. Put its JSON summary in the pull request. A change to `crates/link` or `crates/link-helper` also passes `scripts/check.sh mutants`.
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

  Reference run, MacBook Air M2: 57 fps, round trip p50 8.0 ms (p95 12.9 ms), keyframe request answered in 14 ms.
- **`quest`**: builds and installs both apps and keeps the headset awake without anyone wearing it. Then it streams the test pattern through `adb reverse`. It checks the stream summary and the headset log, and saves a headset screenshot when the device allows it.
- `EVAL_SECONDS`, `EVAL_MIN_FPS`, `EVAL_MAX_ROUND_TRIP_MS` and `EVAL_SKIP_BUILD=1` adjust a run.

## Code standards

[docs/code-standards.md](docs/code-standards.md) is the full list: every rule, where it comes from, the tool and setting that enforce it, and the guardrails for AI-assisted work (the escape-hatch ledger, code owners, the agent guard, mutation testing). `scripts/check.sh` enforces every rule it can. The ones you will meet most often:

- **Size.** Files up to 400 lines (shell 150), functions up to 50, at most 6 parameters, cyclomatic complexity 10, nesting 5. Split before you hit a limit.
- **No comments.** Intent lives in names, types and tests; design reasons live in `docs/`. Silence a lint only with a stated reason: `#[expect(lint, reason = "…")]` in Rust, or a config entry explained in `docs/code-standards.md`.
- **Rust.** Edition 2024, clippy `all` and `pedantic` plus a curated restriction set, all as errors. No `unwrap`, `expect`, `panic!`, `as` casts, single-letter names or silently dropped results. No `unsafe` on the Mac side at all; on the headset only in modules that opt in, one operation per block, frozen by the ledger. Match enum variants by name, not `_`. Host Rust keeps at least 85% line coverage and every protocol mutant must be caught.
- **Swift.** Swift 6 language mode with strict memory safety, `ExistentialAny` and `MemberImportVisibility`; swift-format and SwiftLint in strict mode. Unsafe pointer work only in `PixelMemory.swift` and a few marked calls. UI is `@MainActor`; work on other threads lives in separate lock-protected types. No force unwraps, no `unowned`. Pure logic keeps 90% coverage.
- **Python.** Every ruff rule (a few documented exceptions), and mypy `--strict` for `scripts/` and `docs/images/`.
- **Shell.** bash with `set -euo pipefail`, every ShellCheck check, shfmt in Google style. Call functions as plain statements, never inside `&&` chains.
- **Tests.** Named after the behavior they prove, deterministic (fixed seeds, port 0, waits with deadlines). Hardware tests skip themselves where the hardware is missing.

## Rules

**Mac**
- Launch the app through the bundle (`mac/scripts/run.sh` or `open -n mac/.build/Skyscreens.app --args …`), never `.build/release/skyscreens` directly. macOS grants Screen Recording to the bundle, not to a raw binary.
- Sign with a stable identity. With one (`Skyscreens Dev` or Apple Development), the grant survives rebuilds and `scripts/eval.sh mac` runs unattended. With ad-hoc signing every rebuild drops it. See [Code signing](docs/development.md#code-signing).
- Virtual displays: create all of them once, up front, in the helper process. Creating a display after one was destroyed in the same process gives a dead display. Hide and show screens by mirroring (`park` and `unpark`), and change modes in place with `apply()`.
- Only `skyscreens-displays` owns displays. When it exits, macOS moves the windows back to the laptop, which is the intended safe failure.
- Only `skyscreens-link` owns the headset connection. Swift never links Rust: it writes bridge records to the helper's stdin and reads 10-byte events from its stdout. The helper exits when its stdin closes, so it cannot outlive the app.

**Protocol (`crates/link`)**
- The Mac and the headset share this crate, so they can't disagree about the format. Change messages in `message.rs` only, add a round-trip test, and bump `PROTOCOL_VERSION` on any incompatible change. `the_wire_format_is_stable` pins the bytes.
- The bridge between Swift and `skyscreens-link` reuses `link`'s message encoding. Change it in `crates/link-helper/src/bridge.rs` and `mac/Sources/SkyscreensKit/BridgeCodec.swift` together, and update `crates/link-helper/tests/vectors.json`; both test suites check it.
- `link`, `link-helper` and `fake-headset` forbid `unsafe` code. Parsers of untrusted bytes never index, slice or overflow.
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
| A check fails silently in a shell script | A function called inside `&&` or `\|\|` runs with `set -e` off. Call it as a plain statement (ShellCheck SC2310 catches this) |
| Swift 6: "sending … risks causing data races" | A non-Sendable value crosses actors. Keep UI state on `@MainActor` and move cross-thread state into a lock-protected type, as `FramePipeline` does |
| `skyscreens probe` reports a dirty area near 100% | ScreenCaptureKit's dirty rectangles vary with system state from run to run, in old and new builds alike. Compare builds by running them alternately |
| The Mac app dies when the link helper exits | Writing to a closed pipe raises SIGPIPE. `LinkHelper` ignores SIGPIPE and treats the write error as a lost headset |
| `cargo llvm-cov` warns that some functions have mismatched data | Harmless: `link` is compiled into several test binaries with different `cfg(test)`. The coverage totals are still correct |

## Where things are going

Phase 2 replaces the surface swapchain with a GPU path, because surface swapchains have no GPU access and no mipmaps. MediaCodec will decode into `AImageReader`, and a Vulkan copy will move each frame into a persistent texture per screen. LZ4 lossless tiles will be uploaded into the same texture. Phase 2 also adds a cursor layer drawn on the headset, 3–4 screens, and one-shortcut On/Off. Phase 3 adds UDP with FEC, resend and LTR recovery, plus Noise encryption. The details and their reasons are in [docs/architecture.md](docs/architecture.md).
