# Contributing to skyscreens

Thanks for helping. skyscreens is early, so even a short test report on your own hardware is valuable.

## Ways to help

- **Test on your hardware.** Another Mac model, a Quest 3S, a different cable or macOS version. Run the [Phase 1 stream](docs/development.md#stream-to-the-headset-phase-1) and open an issue with the latency line it prints.
- **Build the next phase.** The [roadmap](README.md#roadmap) and [docs/architecture.md](docs/architecture.md) describe what comes next and why. Open an issue before starting something large, so we can agree on the approach.
- **Bring up another headset.** Android XR and Steam Frame are next after the Quest. The [architecture](docs/architecture.md#other-headsets-later) explains what a new platform needs.
- **Improve the docs.** If a step in the [development guide](docs/development.md) didn't work for you, fix it.

## Getting set up

Follow the [development guide](docs/development.md). Before you open a pull request, these must pass:

```sh
cargo test -p link
cargo build --release -p link && (cd mac && swift build -c release)
xr/build.sh
```

If your change affects what runs on the headset, test it there too. Put the latency line from `mac/scripts/run.sh stream` in the pull request.

## How we work

- **Measure first.** Performance claims need numbers from `skyscreens stream`, `skyscreens probe` or `bench/`, with the hardware they came from.
- **Keep it small.** Small, focused pull requests, direct code, few dependencies. Comments explain why, not what.
- **Protocol changes** go in `crates/link/src/message.rs`, with a round-trip test and a `PROTOCOL_VERSION` bump if older apps can't read the new format. Keep `crates/link/src/ffi.rs` and `mac/Sources/CLink/include/link.h` in sync.
- **Headset code** is plain OpenXR and Vulkan. Vendor-specific code goes in `xr/src/platform.rs` and is chosen by detected extensions, never by device name.
- **Commit messages** are one line in [Conventional Commits](https://www.conventionalcommits.org) style: `feat(xr): decode into AImageReader`, `fix(mac): …`, `docs: …`.

## Using an AI agent

Agents are welcome here. [AGENTS.md](AGENTS.md) gives yours the commands, the rules and the known pitfalls. Review what the agent produces as carefully as your own code, and test it on the device when the change touches the device.

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
