#!/usr/bin/env python3

import base64
import math
import random
import subprocess
import tempfile
from pathlib import Path

W, H = 1536, 1024
HERE = Path(__file__).resolve().parent
CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"

INK = "#0c1322"
PANEL = "#121a2c"
MUTED = "#5d6b85"
SYNTAX = ["#7cc4ff", "#c792ea", "#ffcb6b", "#c3e88d", "#f78c6c", "#89ddff"]

CODE = [(478, 186), (1058, 186), (1058, 548), (478, 548)]
CHAT = [(113, 256), (456, 230), (456, 505), (113, 478)]
EMAIL = [(1080, 230), (1423, 256), (1423, 478), (1080, 505)]
VIDEO = [(628, 18), (908, 18), (923, 170), (613, 170)]


class Screen:
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
        self.poly(
            [(cx + r * math.cos(a * math.pi / 12), cy + r * math.sin(a * math.pi / 12)) for a in range(24)],
            fill,
            opacity,
        )

    def svg(self):
        o = " ".join(f"{x:.1f},{y:.1f}" for x, y in self.quad)
        return "\n".join(
            [
                f'<polygon points="{o}" fill="#000000" opacity="0.45" filter="url(#shadow)" transform="translate(0 14)"/>',
                f'<polygon points="{o}" fill="#9cc7ff" opacity="0.30" filter="url(#glow)"/>',
                f'<polygon points="{o}" fill="{INK}"/>',
                *self.parts,
                f'<polygon points="{o}" fill="none" stroke="#ffffff" stroke-opacity="0.28" stroke-width="1.5" stroke-linejoin="round"/>',
            ]
        )


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
    s.rect(360, 124 + 9 * 38 - 6, 3, 24, "#ffffff")
    return s


def chat_screen(quad):
    s = Screen(1296, 816, quad)
    menu_bar(s)
    s.rect(0, 26, 250, 790, PANEL)
    for i in range(10):
        s.rect(30, 70 + i * 52, 140 + (i * 29) % 50, 14, MUTED, 1 if i == 2 else 0.5)
    y = 70
    for i, (w, h) in enumerate([(420, 70), (300, 54), (520, 96), (260, 54), (460, 70), (340, 54)]):
        if i % 2:
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


def scene(with_text, out_w, out_h):
    photo = base64.b64encode((HERE / "background.jpg").read_bytes()).decode()
    view_h = W // 2 if with_text else H
    out = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{out_w}" height="{out_h}" viewBox="0 0 {W} {view_h}">',
        "<defs>",
        '<filter id="glow" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur stdDeviation="16"/></filter>',
        '<filter id="shadow" x="-20%" y="-20%" width="140%" height="160%"><feGaussianBlur stdDeviation="18"/></filter>',
        '<radialGradient id="fade" cx="0" cy="0" r="0.75"><stop offset="0" stop-color="#05070d" stop-opacity="0.8"/>'
        '<stop offset="1" stop-color="#05070d" stop-opacity="0"/></radialGradient>',
        "</defs>",
        f'<image href="data:image/jpeg;base64,{photo}" x="0" y="0" width="{W}" height="{H}"/>',
        f'<rect width="{W}" height="{H}" fill="#05070d" opacity="0.12"/>',
    ]
    screens = [video_screen(VIDEO), chat_screen(CHAT), email_screen(EMAIL), code_screen(CODE)]
    out += [s.svg() for s in screens]
    if with_text:
        font = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif"
        out.append('<rect x="0" y="0" width="900" height="520" fill="url(#fade)"/>')
        out.append(
            f'<text x="52" y="104" font-family="{font}" font-size="74" font-weight="700" fill="#ffffff" letter-spacing="-1.5">skyscreens</text>'
        )
        out.append(
            f'<text x="55" y="152" font-family="{font}" font-size="29" fill="#eef3fb">Extra monitors for your Mac,</text>'
        )
        out.append(
            f'<text x="55" y="190" font-family="{font}" font-size="29" fill="#eef3fb">inside a Meta Quest 3.</text>'
        )
    out.append("</svg>")
    return "\n".join(out) + "\n"


def render(svg, out_w, out_h, target):
    with tempfile.TemporaryDirectory() as tmp:
        src, png = Path(tmp) / "scene.svg", Path(tmp) / "scene.png"
        src.write_text(svg)
        subprocess.run(
            [
                CHROME,
                "--headless=new",
                "--hide-scrollbars",
                f"--user-data-dir={tmp}/chrome",
                f"--window-size={out_w},{out_h}",
                f"--screenshot={png}",
                src.as_uri(),
            ],
            check=True,
            capture_output=True,
        )
        subprocess.run(
            ["sips", "-s", "format", "jpeg", "-s", "formatOptions", "86", str(png), "--out", str(target)],
            check=True,
            capture_output=True,
        )
    print(f"wrote {target.name}")


if __name__ == "__main__":
    render(scene(False, W, H), W, H, HERE / "banner.jpg")
    render(scene(True, 1280, 640), 1280, 640, HERE / "social-preview.jpg")
