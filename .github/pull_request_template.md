## What and why

<!-- What this changes, and the problem it solves. Link the issue if there is one. -->

## How it was tested

- [ ] `scripts/check.sh` passes
- [ ] `scripts/eval.sh mac` passes (capture, encoding, streamer or protocol changes). Paste its JSON summary
- [ ] `scripts/eval.sh quest` passes (headset changes), or say why it could not run
- [ ] A new or changed test covers the behavior change

## Checklist

- [ ] Wire format changes have a round-trip test, and `PROTOCOL_VERSION` is bumped if older apps can't read them
- [ ] Docs updated (README, `docs/`, `AGENTS.md`, `llms.txt`) if behavior or commands changed
