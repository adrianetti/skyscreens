# Architecture

skyscreens streams virtual Mac displays to a Meta Quest 3 so they feel like real monitors: sharp text, low latency, no account, one shortcut to start.

Every decision below comes from measurements on a MacBook Air M2 (macOS 26.6) and from simulations in [`bench/`](../bench). Anything that is an estimate rather than a measurement says so.

## Overview

```
MAC (Swift + Rust link)                                  HEADSET (Rust, Quest 3 first)

skyscreens-displays  4 virtual displays, kept alive, never restarted
        │
ScreenCaptureKit ── dirty rects ──┬─ small change ─► LZ4 lossless tile ─┐
                                  ├─ motion ───────► H.264 low latency ─┤
                                  └─ settled ──────► lossless refresh ──┤
                                                                        ▼
cursor position + shape ───────────────────────────────────────► link (Rust, shared)
                                                                  USB: TCP over adb
                                                                  Wi-Fi: UDP + FEC + resend
                                                                        │
                                       ┌────────────────────────────────┘
                                       ▼
                   LZ4 tiles ──► texture upload ──┐
                   H.264 ──► MediaCodec ──► GPU copy ──► persistent screen texture ──► OpenXR layer
                   cursor ─────────────────────────────────────────────────────────► cursor layer
```

## Components

| Part | Language | Job |
|---|---|---|
| `mac/` menu-bar app + CLI | Swift | On/Off, capture, encode, cursor, launches the Quest app over adb |
| `skyscreens-displays` | Swift | Tiny helper process that owns the virtual displays |
| `crates/link` | Rust | Wire format, encryption, FEC, resend, priorities, pacing. Used by both sides (C ABI on the Mac) |
| `xr/` | Rust | Headset app, OpenXR + Vulkan: decode, compose, show layers, passthrough. Quest 3 first, other headsets later (see [Other headsets](#other-headsets-later)) |

Swift on the Mac because every heavy step runs in Apple hardware through Apple frameworks. Rust on the headset because the app is small and native: no engine and no Java UI. The protocol lives in one Rust crate so the sender and the receiver can never disagree, and it can be tested end to end with simulated packet loss.

## Key findings and the decisions they drove

### 1. Encode time is the bottleneck, not bandwidth

Measured with the M2's hardware H.264 encoder in low-latency mode:

| Frame size | Encode p50 | Encode p99 |
|---|---|---|
| 1280×720 | 4.6 ms | 7.9 ms |
| 1920×1080 | 7.5 ms | 9.3 ms |
| 2560×1440 | 9.7 ms | 11.7 ms |
| 3840×2160 | 19.1 ms | 21.0 ms |
| 4 × 2560×1440 changing at once | 33.6 ms | 37.6 ms |

- Cost scales with pixels, not with how much changed: typing one character costs as much encode time as a full video frame.
- The base M2 has a single encode engine, shared by every screen.
- Bandwidth is small: typing about 0.7 Mbps, scrolling about 1.4 Mbps, full-screen video about 20 Mbps at 1440p60.

**Decisions:**
- Small changes skip the video encoder (see 2).
- Screens are sized to the headset, not to monitor resolutions (see 3).
- An encoder scheduler encodes the screen under the cursor first, keeps the video screen at about 720p30, and throttles background screens when several change at once.

### 2. A lossless lane for text

LZ4 on raw pixels, measured on the M2:

| Content | Raw | LZ4 | Encode | Decode |
|---|---|---|---|---|
| Typing patch, 320×24 | 30 KB | 2.6 KB | 0.01 ms | under 0.01 ms |
| Full 2560×1440 code screen | 14.4 MB | 483 KB | 2.8 ms | 1.1 ms |

**Decision:** three lanes per screen, chosen frame by frame from ScreenCaptureKit's dirty rectangles.
- **Small change** (typing, a new chat message): an LZ4 lossless tile. No video encode or decode, and the pixels are exact.
- **Motion** (video, scrolling, dragging a window): H.264 low latency.
- **Settled** (about 250 ms after motion stops): a background lossless refresh of the area last drawn by video. Static text ends up pixel-perfect, including colored syntax highlighting that 4:2:0 video would blur.

The synthetic test screen has flat backgrounds, so real content will compress less. 10–30× is a fair expectation.

### 3. Screens sized to the headset

The Quest 3 shows about 25 pixels per degree. A sharpness model ([`bench/sharpness.swift`](../bench/sharpness.swift)) renders text through the compositor under small head movements and compares it with ideal text drawn directly at headset resolution:

| Source pixels per point | Plain bilinear | Supersampled |
|---|---|---|
| 1× | 25.8 dB | 25.7 dB |
| 1.5× | 26.8 dB | 27.1 dB |
| 2× | 24.9 dB | 25.8 dB |

(PSNR at 1 headset pixel per point. Higher is closer to ideal.)

- More than 1.5× gains nothing. 2× without filtering is worse than 1× because of aliasing.
- Sharpness is limited by how many headset pixels each point gets, so readable text needs about 0.85–1 headset pixel per point.

**Decisions:**
- Virtual displays run at 1× (points = pixels). Capture scale is a setting (1× or 1.5×) to be tuned by eye in Phase 1.
- Default layout for 4 screens:

| Screen | Size (points) | Headset px per point | Width |
|---|---|---|---|
| Code, center | 1600×1000 | 0.9 | about 58° |
| Chat, left | 1280×800 | 0.85 | about 44° |
| Email, right | 1280×800 | 0.85 | about 44° |
| Video, above center | 1280×720 | 0.6 | about 31° |

The span is about 145° wide, like a three-monitor desk with one screen above.

### 4. Transport: resend lost pieces and never request a keyframe

Packet-level simulation ([`bench/netsim.py`](../bench/netsim.py)) of a 60-second workday: video, focused editor, chat, email, a Space switch at 30 s, and the cursor. Frame sizes and encode times are the M2 measurements above. Link models: USB measured; hotspot hop measured (see 5); router and loss rates estimated.

Late or lost frames per minute:

| Design | USB | Router | S23 hotspot | Hotspot, bad day |
|---|---|---|---|---|
| A: one TCP stream | 0 | 0 | 0 | 28 |
| B: UDP, keyframe on loss (how ALVR works) | 0 | 141 | 229 | 702 |
| C: UDP + FEC + resend + LTR recovery | 0 | 0.5 | 1.8 | 26 |
| D: C + lossless lane + priorities + local cursor | 0 | 0 | 0.8 | 27 |

Latency for design D with headset-sized screens, p50 / p99:

| Link | Editor | Video | Cursor |
|---|---|---|---|
| USB-C cable | 21 / 34 ms | 31 / 37 ms | 9 / 10 ms |
| Wi-Fi router | 25 / 40 ms | 35 / 42 ms | 12 / 16 ms |
| S23 hotspot | 29 / 45 ms | 40 / 49 ms | 14 / 22 ms |
| Hotspot, bad day | 37 / 92 ms | 47 / 96 ms | 18 / 66 ms |

For comparison, the same pipeline sending everything as video, with the cursor inside the video, is about 35 ms for both editor and cursor on USB.

**Decisions:**
- **USB:** one TCP connection over `adb forward` (adb carries TCP only). The link is lossless, so head-of-line blocking doesn't matter. Our own priority queue sends small writes and sets `TCP_NOTSENT_LOWAT`, so a big refresh can never sit in front of the cursor.
- **Wi-Fi:**
  - UDP with about 10% FEC per frame and fast resend of what FEC can't repair.
  - If a frame is still lost, the encoder references the last frame the Quest confirmed (LTR, supported in VideoToolbox low-latency mode). No keyframe storms.
  - Sends are paced so the access point isn't flooded with bursts.
- **Priority order:** resends, cursor, focused screen, video screen, other screens, background refresh.
- **Cursor:** captured without the cursor. The Mac sends position at 120 Hz and the cursor image only when it changes, and the Quest draws it as its own layer.
- **Temporal layers** (`BaseLayerFrameRateFraction`, supported) mark half the video frames as droppable under congestion.

A bad day (1 airtime stall per second, 1% loss) hurts every design equally. Software can't fix the air: use the cable, a router in the same room, or stay close to the phone.

### 5. The phone hotspot is better than expected

Measured from the Mac to the S23 (one hop, 5 GHz, 80 MHz, 802.11ax, -52 dBm), with 1200-byte pings during extra UDP load:

| Extra load | Loss | RTT p50 | RTT p99 | Max |
|---|---|---|---|---|
| none | 0% | 4.7 ms | 10.1 ms | 11.2 ms |
| 60 Mbps | 0% | 4.8 ms | 12.7 ms | 16.8 ms |
| 250 Mbps | 0% | 4.5 ms | 14.1 ms | 15.9 ms |

The Quest-to-phone hop is not measured yet, so the simulation assumes it is similar. The 4-screen workday needs about 15 Mbps.

### 6. Virtual displays on the base M2

Measured with `CGVirtualDisplay`:
- 4 × 2560×1440 at 120 Hz, 8 × 1920×1080 at 60 Hz, and HiDPI modes are all accepted.
- Changing mode or refresh rate in place with `apply()` works live.
- **Quirk:** creating a display in a process that has already destroyed one produces a dead display with no mode.
- Parking a display by mirroring it onto the built-in screen works and removes it from the desktop. Unmirroring brings it back.

**Decisions:**
- A tiny helper process owns the displays for the whole session. Off means the helper exits, and macOS moves the windows back to the laptop. If the helper crashes, the same happens: a safe failure.
- Adding or removing screens mid-session uses parking (mirroring), not destroy and recreate.
- The streamer runs in a separate process, so it can restart without moving any windows.
- Avoid HiDPI mode switches. Reportedly they can make a display unremovable.
- Default to 60 Hz. 120 Hz is accepted, but its real cadence needs measuring (BetterDisplay's developer reports it can misbehave).

### 7. Quest side: one GPU copy instead of zero-copy

- `XR_KHR_android_surface_swapchain` is supported on Quest 3, but those swapchains have no mipmaps and no GPU access. That makes it impossible to merge the lossless lane with the video lane.
- ALVR and WiVRn also decode into `AImageReader` and then Vulkan.

**Decisions:**
- **Decode path:** MediaCodec → `AImageReader` (AHardwareBuffer) → Vulkan import → copy the changed region into a persistent texture per screen → quad or cylinder layer.
  - The copy costs an estimated 0.3 ms.
  - Persistent textures keep the last image on screen while reconnecting, so there's no black flash.
- **Decoder settings:** `vendor.qti-ext-dec-low-latency.enable=1`, `operating-rate` set to max, `priority=0`, as in ALVR. Avoid `KEY_LOW_LATENCY`, which ALVR found crashes some devices.
- **Decoder pool:** sized by `getMaxSupportedInstances()`. A screen gets a decoder only while it has motion, and taking one over starts with a keyframe. That way the screen count is not limited by the decoder count. Virtual Desktop caps Quest 3 at 3 monitors, which hints at a limit.
- **Layer filtering:** `XR_META_automatic_layer_filter`, falling back to `XR_FB_composition_layer_settings`.
- **Layer budget:** Quest allows 16 layers in total, so up to about 14 screens plus the cursor.
- **Wi-Fi lock:** `WIFI_MODE_FULL_LOW_LATENCY` while the app is in the foreground.
- **Refresh:** the app requests 90 Hz, and 120 Hz is available.

### 8. Security

The stream contains email and chat, so it is always encrypted.
- **Pairing:** the first connection happens over USB, or by confirming a short code shown in the headset. It exchanges X25519 keys.
- **Encryption:** every packet uses ChaCha20-Poly1305 or AES-GCM (Noise protocol, IK pattern). At 15–30 Mbps the cost is negligible on both devices.
- **No accounts and no cloud:** traffic never leaves the local link.

### 9. Audio and input

- **Audio:** stays on the Mac speakers, or on headphones paired with the Mac. That's zero audio latency, and video arrives about 31 ms later, inside lip-sync tolerance (about 45 ms).
- **Input:** the Mac keyboard and trackpad work natively, because macOS treats the virtual screens as real monitors. Quest controllers and keyboard come later.

## Daily use

1. Plug in the cable and press the shortcut (Shortcuts app, a global hotkey, or the menu-bar item). The CLI is `skyscreens on` / `skyscreens off`.
2. The Mac starts the displays helper and the streamer, then launches the Quest app over adb.
3. Put on the headset. The screens are already there.
4. Press the same shortcut to turn it off. Windows move back to the laptop.

Over Wi-Fi only, open the app on the Quest once. It reconnects by itself after that. Pulling the cable switches to Wi-Fi without closing the screens.

## Targets

| Metric | Target |
|---|---|
| Typing latency (key to visible), USB | ≤ 25 ms p50 |
| Cursor latency, USB | ≤ 12 ms p50 |
| Late frames on a normal Wi-Fi or hotspot day | < 2 per minute |
| Mac CPU while streaming | < 10% |
| App size, each side | < 5 MB |
| Idle cost | about 0: nothing changes, nothing is sent |

A latency readout (timestamps on both sides) ships in Phase 1, so every target is measured, not assumed.

## Not measured yet

Each of these gets measured in Phase 0 or 1 on the real devices:
- MediaCodec decode latency on Quest 3, and whether the Qualcomm low-latency key takes effect.
- Maximum concurrent decoders on Quest 3.
- ScreenCaptureKit capture latency from a virtual display, and whether 120 Hz really delivers 120 frames.
- The second hotspot hop (phone to Quest).
- Getting the cursor image for every cursor shape (`NSCursor.currentSystem` is deprecated).

## Other headsets (later)

Quest 3 comes first. These rules apply from day one, so other devices stay cheap to add.

- **Plain OpenXR + Vulkan.** The headset app uses nothing else directly. Everything vendor-specific lives behind two small modules:
  - `platform`: passthrough, layer filters, refresh rate, layer limit, Wi-Fi lock.
  - `media`: hardware decode and frame import (Android: MediaCodec + `AImageReader`; other operating systems later).
- **Features are detected, not assumed.** They're chosen from the extensions the OpenXR runtime reports at startup, never from the device name.
- **The headset describes itself when it connects:** resolution and pixels per degree, refresh rates, codecs, decoder count, layer limit, passthrough. The Mac sizes the screens from that, so a sharper headset automatically gets more workspace per screen with no code changes.
- **The protocol is versioned from the first release,** so older and newer apps can still talk to each other.

| Device | Platform | What it takes | Notes |
|---|---|---|---|
| Quest 3 / 3S | Horizon OS (Android) | the target | 3S has softer Fresnel lenses, about 20 pixels per degree |
| Samsung Galaxy XR | Android XR | a new `platform` module | Micro-OLED at about 3.5K per eye: the closest to a real monitor |
| XREAL Project Aura | Android XR glasses (2026) | the same Android XR module | Optical see-through glasses with a tethered compute puck. Early specs list OpenXR |
| Steam Frame | SteamOS on Snapdragon | possibly the same Android build | Valve says it runs Android APKs natively, including Quest-style VR apps. To verify on the device |
| Pico 4 / 4 Ultra, Vive Focus Vision | Android | a new `platform` module | OpenXR with vendor passthrough |
| Apple Vision Pro | visionOS | a new client in Swift + Metal that reuses `link` | Already has Mac Virtual Display built in, so lowest priority |
| XREAL One Pro | display glasses over USB-C | a different mode (below) | |

**Display glasses such as the XREAL One Pro:**
- They plug in as a regular monitor, so nothing has to be encoded or streamed.
- A possible mode: the Mac draws the whole desk itself with Metal, following the glasses' head tracking, and sends it out as one monitor signal. It reuses the displays helper, capture and layout code.
- It needs the glasses' head-tracking data over USB. Open-source Linux drivers already read it for earlier XREAL models; check whether the One Pro allows it.
- At about 57° wide, the glasses show roughly one screen at a time. With about 33 pixels per degree, text is sharper than on a Quest 3.

## Phases

0. **Toolchains and an empty Quest app** (Rust, OpenXR, Vulkan, passthrough, one test layer). On-device checks: decoder instances, the low-latency key, refresh rates.
1. **One screen over USB:** displays helper, capture, H.264 lane, decode into a layer, latency readout.
2. **Daily-use features:** the lossless lane, local cursor, 3–4 screens, and one-shortcut On/Off.
3. **Wi-Fi and hotspot:** UDP + FEC + resend + LTR, pacing, pairing and encryption.
4. **Later:** moving and resizing screens, Quest controllers and keyboard, settings.
5. **Other headsets:** Android XR (Galaxy XR, Project Aura) and Steam Frame first, since they reuse most of the app.

## Repo layout

```
mac/           Swift: menu bar, CLI, displays helper, capture, encode, cursor
xr/            Rust: OpenXR + Vulkan headset app (Quest 3 first)
  platform/    one module per vendor: Quest first
  media/       hardware decode per operating system: Android first
crates/link/   Rust: wire protocol, encryption, FEC, priorities (shared)
bench/         measurements and simulations behind this document
docs/          architecture and protocol
```
