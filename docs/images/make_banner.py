#!/usr/bin/env python3
"""Draws the README banner and the GitHub social preview as SVG.

The picture is the default four-screen layout from docs/architecture.md, seen from the
headset: code in the center, chat on the left, email on the right, video above. Side
screens and the video screen are drawn in perspective, the way they curve around you.

    python3 docs/images/make_banner.py      # writes banner.svg and social-preview.svg here

The PNG for GitHub (Settings > Social preview) is social-preview.svg rendered by Chrome:

    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new \\
        --hide-scrollbars --window-size=1280,640 --screenshot=docs/images/social-preview.png \\
        "file://$PWD/docs/images/social-preview.svg"
"""

import math
import random
from pathlib import Path

W, H = 1280, 640
HERE = Path(__file__).resolve().parent

INK = "#0c1322"
PANEL = "#121a2c"
LINE = "#2a3550"
MUTED = "#5d6b85"
BLUE = "#6aa8ff"
SYNTAX = ["#7cc4ff", "#c792ea", "#ffcb6b", "#c3e88d", "#f78c6c", "#89ddff"]


class Screen:
    """A screen in local pixels (w, h), drawn onto a quad given as TL, TR, BR, BL."""

    def __init__(self, w, h, quad):
        self.w, self.h, self.quad = w, h, quad
        self.parts = []

    def at(self, x, y):
        (tlx, tly), (trx, try_), (brx, bry), (blx, bly) = self.quad
        u, v = x / self.w, y / self.h
        top = (tlx + u * (trx - tlx), tly + u * (try_ - tly))
        bottom = (blx + u * (brx - blx), bly + u * (bry - bly))
        return top[0] + v * (bottom[0] - top[0]), top[1] + v * (bottom[1] - top[1])

    def poly(self, points, fill, opacity=1.0):
        pts = " ".join(f"{px:.1f},{py:.1f}" for px, py in (self.at(x, y) for x, y in points))
        op = "" if opacity == 1 else f' opacity="{opacity}"'
        self.parts.append(f'<polygon points="{pts}" fill="{fill}"{op}/>')

    def rect(self, x, y, w, h, fill, opacity=1.0):
        self.poly([(x, y), (x + w, y), (x + w, y + h), (x, y + h)], fill, opacity)

    def circle(self, cx, cy, r, fill, opacity=1.0):
        self.poly([(cx + r * math.cos(a * math.pi / 12), cy + r * math.sin(a * math.pi / 12)) for a in range(24)], fill, opacity)

    def outline(self):
        return " ".join(f"{x:.1f},{y:.1f}" for x, y in self.quad)

    def svg(self):
        o = self.outline()
        return "\n".join([
            f'<polygon points="{o}" fill="{BLUE}" opacity="0.35" filter="url(#glow)"/>',
            f'<polygon points="{o}" fill="{INK}"/>',
            *self.parts,
            f'<polygon points="{o}" fill="none" stroke="#ffffff" stroke-opacity="0.22" stroke-width="1.5" stroke-linejoin="round"/>',
        ])


def menu_bar(s):
    s.rect(0, 0, s.w, 26, "#1a2236")
    s.circle(22, 13, 6, MUTED)
    for i, w in enumerate([46, 38, 52, 40]):
        s.rect(50 + i * 70, 9, w, 8, MUTED, 0.7)


def code_screen(quad):
    s = Screen(1616, 1008, quad)
    menu_bar(s)
    s.rect(40, 56, 1536, 912, "#0f1626")
    s.rect(40, 56, 1536, 38, "#151e31")
    for i, c in enumerate(["#ff5f57", "#febc2e", "#28c840"]):
        s.circle(66 + i * 24, 75, 7, c)
    s.rect(40, 94, 250, 874, PANEL)
    for i in range(16):
        s.rect(64 + (i % 4 > 0) * 22, 124 + i * 40, 130 + (i * 37) % 70, 12, MUTED, 0.55 if i != 5 else 1)
    rng = random.Random(7)
    indent = 0
    for row in range(22):
        y = 124 + row * 38
        s.rect(306, y, 22, 10, MUTED, 0.4)
        if row in (6, 13, 19):
            indent = 0
            continue
        x = 360 + indent * 44
        for _ in range(rng.randint(1, 4)):
            w = rng.choice([40, 64, 90, 120, 160, 210])
            if x + w > 1500:
                break
            s.rect(x, y, w, 12, rng.choice(SYNTAX))
            x += w + 14
        indent = max(0, min(3, indent + rng.choice([-1, 0, 0, 1])))
    s.rect(360, 124 + 9 * 38 - 6, 3, 24, "#ffffff")  # text cursor
    return s


def chat_screen(quad):
    s = Screen(1296, 816, quad)
    menu_bar(s)
    s.rect(0, 26, 250, 790, PANEL)
    for i in range(10):
        s.rect(30, 70 + i * 52, 140 + (i * 29) % 50, 14, MUTED, 1 if i == 2 else 0.5)
    y = 70
    for i, (w, h) in enumerate([(420, 70), (300, 54), (520, 96), (260, 54), (460, 70), (340, 54)]):
        mine = i % 2 == 1
        if mine:
            s.rect(1296 - 40 - w, y, w, h, "#2f6feb")
        else:
            s.circle(300, y + 22, 20, SYNTAX[i % len(SYNTAX)])
            s.rect(340, y, w, h, "#1f2a40")
        y += h + 34
    s.rect(290, 740, 966, 50, "#1a2338")
    return s


def email_screen(quad):
    s = Screen(1296, 816, quad)
    menu_bar(s)
    s.rect(0, 26, 210, 790, PANEL)
    for i in range(8):
        s.rect(28, 70 + i * 48, 110 + (i * 23) % 50, 14, MUTED, 0.5)
    s.rect(210, 26, 420, 790, "#10182a")
    for i in range(9):
        y = 46 + i * 84
        if i == 1:
            s.rect(214, y - 12, 412, 80, "#2f6feb", 0.85)
        s.rect(240, y, 200 + (i * 41) % 120, 14, "#e6edf3", 0.9 if i < 3 else 0.6)
        s.rect(240, y + 28, 330, 10, MUTED, 0.8)
        s.rect(240, y + 46, 260, 10, MUTED, 0.5)
    s.rect(680, 70, 480, 22, "#e6edf3", 0.9)
    for i in range(12):
        s.rect(680, 130 + i * 34, 560 - (i * 53) % 180, 11, MUTED, 0.7)
    return s


def video_screen(quad):
    s = Screen(1296, 736, quad)
    for i, c in enumerate(["#1d3b6e", "#2b5591", "#3f72b0", "#e7a977"]):
        s.rect(0, i * 130, 1296, 130 if i < 3 else 210, c)
    s.circle(930, 330, 70, "#ffd58a")
    s.poly([(0, 736), (0, 470), (260, 300), (520, 520), (760, 340), (1040, 560), (1296, 420), (1296, 736)], "#203252")
    s.poly([(0, 736), (0, 600), (300, 500), (640, 640), (980, 520), (1296, 640), (1296, 736)], "#141f36")
    s.circle(648, 368, 70, "#000000", 0.45)
    s.poly([(625, 330), (625, 406), (688, 368)], "#ffffff")
    s.rect(40, 690, 1216, 8, "#ffffff", 0.3)
    s.rect(40, 690, 470, 8, "#ff5f57")
    return s


def scene(with_text):
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}" role="img" '
           f'aria-label="Four Mac screens floating in a Meta Quest 3: code in the center, chat on the left, email on the right, video above, with the MacBook below">',
           "<defs>",
           '<linearGradient id="sky" x1="0" y1="0" x2="0" y2="1">'
           '<stop offset="0" stop-color="#050a18"/><stop offset="0.55" stop-color="#11284d"/>'
           '<stop offset="0.85" stop-color="#2c4f86"/><stop offset="1" stop-color="#6b6f9a"/></linearGradient>',
           '<radialGradient id="dawn" cx="0.5" cy="1" r="0.6"><stop offset="0" stop-color="#f2a65a" stop-opacity="0.55"/>'
           '<stop offset="1" stop-color="#f2a65a" stop-opacity="0"/></radialGradient>',
           '<filter id="glow" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur stdDeviation="14"/></filter>',
           "</defs>",
           f'<rect width="{W}" height="{H}" fill="url(#sky)"/>',
           f'<rect width="{W}" height="{H}" fill="url(#dawn)"/>']
    rng = random.Random(3)
    for _ in range(90):
        x, y = rng.uniform(0, W), rng.uniform(0, H * 0.6)
        out.append(f'<circle cx="{x:.0f}" cy="{y:.0f}" r="{rng.uniform(0.5, 1.6):.1f}" fill="#ffffff" opacity="{rng.uniform(0.2, 0.8):.2f}"/>')

    dy = -18
    screens = [
        video_screen([(525, 62 + dy), (755, 62 + dy), (770, 190 + dy), (510, 190 + dy)]),
        chat_screen([(112, 290 + dy), (382, 262 + dy), (382, 492 + dy), (112, 468 + dy)]),
        email_screen([(898, 262 + dy), (1168, 290 + dy), (1168, 468 + dy), (898, 492 + dy)]),
        code_screen([(400, 208 + dy), (880, 208 + dy), (880, 508 + dy), (400, 508 + dy)]),
    ]
    out += [s.svg() for s in screens]

    # The MacBook the screens come from.
    out.append('<polygon points="578,534 702,534 706,602 574,602" fill="#1b2232" stroke="#ffffff" stroke-opacity="0.25" stroke-width="1.5"/>')
    out.append('<polygon points="584,540 696,540 699,596 581,596" fill="#2a4a80" opacity="0.8"/>')
    out.append('<polygon points="552,602 728,602 744,616 536,616" fill="#2a3245" stroke="#ffffff" stroke-opacity="0.2" stroke-width="1"/>')

    if with_text:
        font = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif"
        out.append(f'<text x="60" y="104" font-family="{font}" font-size="60" font-weight="700" fill="#ffffff" letter-spacing="-1">skyscreens</text>')
        out.append(f'<text x="62" y="146" font-family="{font}" font-size="24" fill="#c9d6ee">Extra monitors for your Mac,</text>')
        out.append(f'<text x="62" y="178" font-family="{font}" font-size="24" fill="#c9d6ee">inside a Meta Quest 3.</text>')
        out.append(f'<text x="1220" y="604" text-anchor="end" font-family="{font}" font-size="18" fill="#c9d6ee" opacity="0.85">Open source · No account · USB-C</text>')
    out.append("</svg>")
    return "\n".join(out) + "\n"


if __name__ == "__main__":
    (HERE / "banner.svg").write_text(scene(with_text=False))
    (HERE / "social-preview.svg").write_text(scene(with_text=True))
    print("wrote banner.svg and social-preview.svg")
