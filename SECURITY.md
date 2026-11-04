# Security

skyscreens streams whatever is on your screens, including email and chat, so we treat the stream as sensitive.

## Current state (Phase 1)

- The stream travels only over USB. The Mac listens on `127.0.0.1:7243`, and `adb reverse` forwards that port to the plugged-in headset. Nothing listens on your network.
- **There is no pairing or encryption yet.** Until Phase 3, any process on your Mac, and any app on the plugged-in headset, could connect to that port and receive the stream.
- There are no accounts, no telemetry, and no cloud services.

Phase 3 adds pairing (X25519 keys exchanged over USB or confirmed with a short code), and Noise encryption of every packet, before any Wi-Fi mode ships. The design is in [docs/architecture.md](docs/architecture.md#8-security).

## Reporting a vulnerability

Please don't open a public issue. Report it privately through GitHub: on the repository's **Security** tab, choose **Report a vulnerability**. Include the steps to reproduce it and the versions you used. You'll get an answer as soon as possible. This is a one-person project for now, so please allow a few days.
