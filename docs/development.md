# Development guide

This guide takes you from a fresh Mac to a Mac screen streaming into a Quest 3. Commands run from the repo root unless a step says otherwise. If you work with an AI agent, it should read [AGENTS.md](../AGENTS.md) as well.

- [What you need](#what-you-need)
- [Install the toolchains](#install-the-toolchains)
- [Checks, tests and evals](#checks-tests-and-evals)
- [Code signing](#code-signing)
- [Run the Mac app](#run-the-mac-app)
- [Set up the Quest](#set-up-the-quest)
- [Stream to the headset (Phase 1)](#stream-to-the-headset-phase-1)
- [Logs and debugging](#logs-and-debugging)
- [Troubleshooting](#troubleshooting)

## What you need

- A Mac with Apple Silicon and macOS 14 or later, plus [Homebrew](https://brew.sh).
- Xcode, or only its command-line tools, for Swift.
- A Meta Quest 3 and a USB-C data cable. You need these only for testing on the headset.

## Install the toolchains

**Swift.** Install Xcode from the App Store, or only the command-line tools:

```sh
xcode-select --install
```

**Rust**, with the Android target for the headset app. Homebrew's `rustup` is keg-only, so add it to your `PATH`. The build scripts do this themselves, but your shell needs it too:

```sh
brew install rustup
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
rustup toolchain install
```

Add the `export` line to `~/.zshrc` too. `rustup toolchain install` reads `rust-toolchain.toml` and installs the pinned Rust version, with rustfmt, clippy and the Android target.

**Check tools.** `scripts/check.sh` needs these. Ruff, mypy and zizmor run through `uvx` at pinned versions, so they need no separate install:

```sh
brew install cargo-deny cargo-llvm-cov cargo-shear cargo-mutants swiftlint shellcheck shfmt actionlint typos-cli uv
```

**Android SDK, NDK and JDK** for the headset app. The headset app has no Java code and needs no Gradle, only these packages:

```sh
brew install openjdk@17
brew install --cask android-commandlinetools
SDK=/opt/homebrew/share/android-commandlinetools
JAVA_HOME=/opt/homebrew/opt/openjdk@17 $SDK/cmdline-tools/latest/bin/android --no-metrics --sdk=$SDK \
  sdk install platform-tools build-tools/34.0.0 platforms/android-32 ndk/27.2.12479018
```

Use the `android` tool as shown. `sdkmanager` from the same folder can finish without installing anything. To check what is installed, run the same tool with `sdk list` instead of `sdk install …`.

`xr/build.sh` expects these exact versions (they live in `scripts/android-env.sh`). If your SDK lives somewhere else, set `ANDROID_HOME` and `JAVA_HOME` before running it. On its first run, the script also downloads the Khronos OpenXR loader (version 1.1.65) from Maven Central, checks its SHA-256, and caches it in `target/apk/`.

## Checks, tests and evals

One script runs every check, and CI runs the same script. The rules behind each check are listed in [code-standards.md](code-standards.md).

```sh
scripts/check.sh          # everything; must pass before a change is done
scripts/check.sh fast     # formatting, lints and Rust tests
scripts/check.sh fix      # apply the formatters
scripts/check.sh mutants  # mutation testing of the protocol crates, a few minutes
```

| Stage | What it checks |
|---|---|
| `scripts` | File limits and whitespace, no comments, the escape-hatch ledger, spelling (typos), script tests, ruff, mypy, shfmt, ShellCheck, actionlint, zizmor |
| `rust` | rustfmt, clippy (pedantic plus restriction lints, warnings are errors), unit, property and loopback tests with at least 85% line coverage, `cargo deny`, `cargo shear` |
| `swift` | `swift format` and SwiftLint (strict), Swift 6 build with strict memory safety and warnings as errors, Swift Testing suites with a 90% coverage floor for pure logic, bench typecheck |
| `mutants` | `cargo mutants` on `link` and `link-helper`; fails if any mutant survives. CI runs it as its own job |
| `android` | clippy for `aarch64-linux-android`, then the APK |

Enable the git hooks once. Then `pre-commit` runs `scripts/check.sh fast`, and `commit-msg` checks the Conventional Commits format:

```sh
scripts/install-hooks.sh
```

The evals run the real apps end to end and print a JSON report. They also save their logs in `target/eval/`:

```sh
scripts/eval.sh mac       # Mac pipeline against a fake headset; needs the Screen Recording grant
scripts/eval.sh quest     # full pipeline on a Quest connected over USB
```

`scripts/eval.sh mac` adds a virtual screen for about 12 seconds and streams a moving test pattern. Then it fails on a wrong screen list, an undecodable keyframe, an unanswered keyframe request, fewer than 30 fps, a round trip over 60 ms, or a virtual screen left behind. [AGENTS.md](../AGENTS.md#evals) has the details.

### Guardrails for agents and contributors

- `escape-hatches.json` lists every `unsafe` block, lint exception, discarded result and other escape hatch, and fingerprints every rule file. `scripts/check.sh` fails when the code differs from it. After you review an intended change, regenerate it with `scripts/escape_hatches.py --update`.
- `.github/CODEOWNERS` names the rule files. To make it binding, add a branch ruleset on GitHub (Settings → Rules → Rulesets) for `main` with "Require a pull request before merging" and "Require review from Code Owners".
- `.claude/settings.json` makes Claude Code ask before editing a protected file and blocks shell commands that would rewrite one or regenerate the ledger.

## Code signing

macOS grants Screen Recording to an app's code signature, not to its name.

- **Ad-hoc signature (the default with no certificate).** The signature changes on every build, so macOS forgets the grant after each `mac/scripts/bundle.sh`. System Settings may still show Skyscreens switched on, but that entry belongs to the old build.
- **Stable identity.** With a signing certificate, you grant the permission once and it survives every rebuild.

Create an identity once, with either option:

- **Self-signed certificate (no Apple ID needed).**
  1. Open Keychain Access.
  2. Choose Keychain Access → Certificate Assistant → Create a Certificate….
  3. Name it `Skyscreens Dev`, set Identity Type to *Self Signed Root* and Certificate Type to *Code Signing*.
  4. Click Create.
- **Apple Development certificate (free Apple ID).**
  1. Open Xcode → Settings → Accounts and add your Apple ID.
  2. Click Manage Certificates.
  3. Click + and choose Apple Development.

`mac/scripts/bundle.sh` uses the first identity in your keychain whose name contains `Apple Development` or `Skyscreens`. To pick another one, set `SKYSCREENS_SIGN_ID` to its name or SHA-1 hash. Without an identity it signs ad-hoc and prints a warning. To see your identities:

```sh
security find-identity -p codesigning
```

After you switch to a stable identity, rebuild once and grant the permission again (next section). From then on, the grant stays.

## Run the Mac app

Build the app bundle:

```sh
mac/scripts/bundle.sh          # prints the path of mac/.build/Skyscreens.app
```

Always run commands through the bundle, so macOS checks Screen Recording for Skyscreens and not for your terminal. `mac/scripts/run.sh` does this and shows the output. Ctrl-C stops it.

| Command | What it does |
|---|---|
| `mac/scripts/run.sh permission` | Asks for Screen Recording the first time, then reports whether it's allowed |
| `mac/scripts/run.sh on` | Creates the four virtual screens (the layout is in [architecture.md](architecture.md#3-screens-sized-to-the-headset)) |
| `mac/scripts/run.sh status` | Prints the running screens as JSON, or `off` |
| `mac/scripts/run.sh off` | Removes the screens. macOS moves their windows back to the laptop |
| `mac/scripts/run.sh probe [hz]` | Measures capture and encode latency on a test screen. Your screen may flicker while the test screen is added |
| `mac/scripts/run.sh stream` | Phase 1: streams one screen to the Quest over USB. `--seconds N` stops after N seconds and prints a JSON summary, `--no-headset` skips adb, `--test-pattern` animates a bar on the screen |

The first `permission` adds Skyscreens to System Settings → Privacy & Security → Screen & System Audio Recording. Switch it on there, then run `permission` again. It should print `screen recording: allowed`. To open that settings pane directly:

```sh
open "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
```

## Set up the Quest

1. Turn on Developer Mode. You need a Meta developer account in a verified organization. Then open the Meta Horizon phone app and go to Devices → your headset → Headset settings → Developer Mode. Meta's [device setup guide](https://developers.meta.com/vr/documentation/native/android/mobile-device-setup/) has the current steps.
2. Plug the Quest into the Mac with a data cable. Inside the headset, allow USB debugging and choose *Always allow from this computer*.
3. Check the connection:

   ```sh
   /opt/homebrew/share/android-commandlinetools/platform-tools/adb devices
   ```

   The Quest must show as `device`. `unauthorized` means the prompt in the headset hasn't been accepted yet. An `emulator-…` line is an Android emulator, not the Quest.

4. Install the headset app:

   ```sh
   xr/build.sh --install
   ```

   Without the Mac it shows a test screen over passthrough: a grid, color bars, and boxes of 1-pixel patterns for judging sharpness.

## Stream to the headset (Phase 1)

```sh
mac/scripts/run.sh stream
```

The streamer:

1. creates one virtual screen (1616×1008 at 60 Hz by default);
2. starts the link server on `127.0.0.1:7243`;
3. runs `adb reverse tcp:7243 tcp:7243`, so the headset's `localhost:7243` reaches the Mac;
4. launches the headset app with `adb shell am start`;
5. captures, encodes, and sends low-latency H.264.

The new screen appears in System Settings → Displays like a monitor. Drag a window onto it. Once per second the streamer prints a line like this:

```
connected     60 fps   12.3 Mbps  encode  6.1 ms  capture->decoded on Quest p50  18.0  p95  24.0 ms
```

(The numbers above are an example, not a measurement.) The round trip runs from the moment ScreenCaptureKit delivers a frame to the moment the Quest's decoder outputs it, plus one USB hop back. Ctrl-C stops the stream, prints a `summary {…}` JSON line and removes the screen.

ScreenCaptureKit sends a frame only when something on the screen changes. To measure with steady motion, add `--test-pattern`. `scripts/eval.sh quest` does this for you.

## Logs and debugging

| What | How |
|---|---|
| Headset app log | `adb logcat -s skyscreens:V` (add `-d` to print it once and exit) |
| Is the USB tunnel up? | `adb reverse --list` should show `tcp:7243 tcp:7243` |
| Who holds the port on the Mac? | `lsof -nP -iTCP:7243` |
| Start the headset app by hand | `adb shell am start -n io.github.adrianetti.skyscreens/android.app.NativeActivity` |
| Run without wearing the headset | `adb shell am broadcast -a com.oculus.vrpowermanager.prox_close` |
| Restore worn detection | `adb shell am broadcast -a com.oculus.vrpowermanager.automation_disable` |

## Troubleshooting

| Symptom | Fix |
|---|---|
| `permission` says "not allowed yet" although Settings shows Skyscreens on | The grant belongs to an older build. Select Skyscreens in Settings, remove it with the minus button, run `permission` again and switch it on. Set up [code signing](#code-signing) so this stops happening |
| `capture failed: … -3801` | Screen Recording isn't granted to this build. Same fix as above |
| `bundle.sh` warns about an ad-hoc signature | No signing identity was found. See [Code signing](#code-signing) |
| `USB link: failed` or `headset app: failed` | adb can't reach the Quest. Check `adb devices` and the cable |
| `port 7243 is busy` | Another stream is running. Stop it, or find it with `lsof -nP -iTCP:7243` |
| The streamer prints `waiting...` forever | The headset app isn't connected. Check `adb reverse --list` and the headset log |
| The headset log shows a decoder error | MediaCodec may reject a low-latency key. Remove `operating-rate` or `vendor.qti-ext-dec-low-latency.enable` in `xr/src/decoder.rs` and rebuild |
| `xr/build.sh` can't find a tool | Install the exact SDK package versions listed above, or set `ANDROID_HOME` and `JAVA_HOME`. The NDK must be `27.2.12479018` under `$ANDROID_HOME/ndk` |
| `check.sh` says the escape hatches changed | You added an `unsafe` block, a lint `expect`, a discarded result, or touched a rule file. Remove it; if it is truly needed, the maintainer reviews it and runs `scripts/escape_hatches.py --update` |
| Claude Code asks before editing a config file | The file is protected (`.claude/settings.json`); approve only changes you intend |
| `check.sh` reports a comment | Code carries no comments. Rename until the code explains itself, and move design reasons to `docs/architecture.md` |
| `cargo: command not found` | Add rustup to your `PATH` (see [Install the toolchains](#install-the-toolchains)) |

Something else? [Open an issue](https://github.com/adrianetti/skyscreens/issues/new/choose) with your Mac model, macOS version, headset and OS version, and the logs from both sides.
