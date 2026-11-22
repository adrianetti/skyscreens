# Contributing to skyscreens

Thanks for helping. skyscreens is early, so even a short test report on your own hardware is valuable.

## Ways to help

- **Test on your hardware.** Another Mac model, a Quest 3S, a different cable or macOS version. Run the [Phase 1 stream](docs/development.md#stream-to-the-headset-phase-1) and open an issue with the latency line it prints.
- **Build the next phase.** The [roadmap](README.md#roadmap) and [docs/architecture.md](docs/architecture.md) describe what comes next and why. Open an issue before starting something large, so we can agree on the approach.
- **Bring up another headset.** Android XR and Steam Frame are next after the Quest. The [architecture](docs/architecture.md#other-headsets-later) explains what a new platform needs.
- **Improve the docs.** If a step in the [development guide](docs/development.md) didn't work for you, fix it.

## Getting set up

Follow the [development guide](docs/development.md), then enable the git hooks with `scripts/install-hooks.sh`. Before you open a pull request, this must pass:

```sh
scripts/check.sh
```

CI runs the same script on every pull request. If your change affects capture, encoding or the protocol, run `scripts/eval.sh mac` too. If it affects the headset, run `scripts/eval.sh quest` with a Quest connected. Paste the JSON summary into the pull request.

## How we work

- **Measure first.** Performance claims need numbers from `skyscreens stream`, `skyscreens probe` or `bench/`, with the hardware they came from.
- **Keep it small.** Small, focused pull requests, direct code, few dependencies.
- **Code standards.** No comments in code, size limits, strict lints, no `unsafe` on the Mac side, a test for every behavior change. [docs/code-standards.md](docs/code-standards.md) lists every rule and its source, and `scripts/check.sh` enforces them.
- **Escape hatches need approval.** A new `unsafe` block, lint exception or rule change shows up in `escape-hatches.json`. Explain it in the pull request; the maintainer regenerates the ledger if it is accepted.
- **Protocol changes** go in `crates/link/src/message.rs`, with a round-trip test and a `PROTOCOL_VERSION` bump if older apps can't read the new format. Bridge changes between Swift and `skyscreens-link` update `crates/link-helper/tests/vectors.json`, which both sides test.
- **Headset code** is plain OpenXR and Vulkan. Vendor-specific code goes in `xr/src/platform.rs` and is chosen by detected extensions, never by device name.
- **Commit messages** are one line in [Conventional Commits](https://www.conventionalcommits.org) style: `feat(xr): decode into AImageReader`, `fix(mac): …`, `docs: …`.

## Using an AI agent

Agents are welcome here. [AGENTS.md](AGENTS.md) gives yours the commands, the rules and the known pitfalls. Its definition of done is concrete: `scripts/check.sh` passes, and the evals pass for the parts the change touches. An agent can check all of that itself. Review what the agent produces as carefully as your own code.

Maintainers can also mention `@claude` in an issue or pull request, and the Claude workflow picks it up. Pull requests from this repository get an automatic Claude review. Both need a `CLAUDE_CODE_OAUTH_TOKEN` or `ANTHROPIC_API_KEY` repository secret, and they skip quietly without one.

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
