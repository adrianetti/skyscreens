## What and why

<!-- What this changes, and the problem it solves. Link the issue if there is one. -->

## How it was tested

- [ ] `cargo test -p link`
- [ ] `cargo build --release -p link && (cd mac && swift build -c release)`
- [ ] `xr/build.sh`
- [ ] On the device (paste the latency line from `mac/scripts/run.sh stream` if the change affects streaming)

## Checklist

- [ ] Wire format changes have a round-trip test, and `PROTOCOL_VERSION` is bumped if older apps can't read them
- [ ] `crates/link/src/ffi.rs` and `mac/Sources/CLink/include/link.h` still match
- [ ] Docs updated (README, `docs/`, `AGENTS.md`) if behavior or commands changed
