import bisect
import heapq
import json
import math
import random
import sys
from pathlib import Path

P = json.loads(Path(sys.argv[1] if len(sys.argv) > 1 else "params.json").read_text())
PKT = 1200
DUR = 60.0
SEEDS = 4
FPS = 60
DISPLAY_MS = (1000 / 90) / 2 + 3
CAPTURE_MS = (1000 / 60) / 2 + 2

LINKS = {
    "USB-C cable": {
        "bw": 250,
        "base": 0.7,
        "jit": 0.1,
        "loss": 0.0,
        "burst": (0, 0, 0),
        "spikes": 0.0,
        "spike": (0, 0),
    },
    "Wi-Fi router": {
        "bw": 350,
        "base": 2.5,
        "jit": 0.8,
        "loss": 0.002,
        "burst": (0.0005, 0.7, 0.3),
        "spikes": 0.05,
        "spike": (15, 40),
    },
    "S23 hotspot": {
        "bw": 180,
        "base": 4.0,
        "jit": 1.5,
        "loss": 0.003,
        "burst": (0.001, 0.8, 0.4),
        "spikes": 0.1,
        "spike": (15, 50),
    },
    "hotspot, bad day": {
        "bw": 120,
        "base": 5.0,
        "jit": 3.0,
        "loss": 0.01,
        "burst": (0.003, 0.85, 0.5),
        "spikes": 1.0,
        "spike": (20, 80),
    },
}

STRATS = {
    "A: one TCP stream, all video": {
        "tcp": True,
        "fec": 0,
        "nack": False,
        "ltr": False,
        "prio": False,
        "hybrid": False,
        "local_cursor": False,
    },
    "B: UDP, keyframe on loss (ALVR-style)": {
        "tcp": False,
        "fec": 0,
        "nack": False,
        "ltr": False,
        "prio": False,
        "hybrid": False,
        "local_cursor": False,
    },
    "C: UDP + FEC + resend + LTR recovery": {
        "tcp": False,
        "fec": 0.1,
        "nack": True,
        "ltr": True,
        "prio": False,
        "hybrid": False,
        "local_cursor": False,
    },
    "D: C + lossless text lane + priority + cursor": {
        "tcp": False,
        "fec": 0.1,
        "nack": True,
        "ltr": True,
        "prio": True,
        "hybrid": True,
        "local_cursor": True,
    },
}


def sample(rng, kind):
    p50, p99 = P[kind]["p50"], P[kind]["p99"]
    sigma = max(0.05, math.log(p99 / p50) / 2.326)
    return rng.lognormvariate(math.log(p50), sigma)


def workload(rng):
    ev = [(0.0, s, "key") for s in range(4)]
    t = 0.0
    while t < DUR:
        t += 1 / 30
        ev.append((t, 0, "video"))
    t = 1.0
    while t < DUR:
        if rng.random() < 0.25:
            ev.extend((t + i / FPS, 1, "scroll") for i in range(int(rng.uniform(0.6, 2.0) * FPS)))
            t += 2.5
        else:
            end = t + rng.uniform(2, 8)
            while t < end:
                t += rng.expovariate(9)
                ev.append((t, 1, "typing"))
            t += rng.uniform(0.5, 3)
    for s, gap in ((2, 8), (3, 25)):
        t = rng.uniform(1, gap)
        while t < DUR:
            ev.extend((t + i / FPS, s, "typing" if rng.random() < 0.7 else "scroll") for i in range(rng.randint(2, 12)))
            t += rng.expovariate(1 / gap)
    ev += [(30.0 + s * 0.001, s, "key") for s in range(4)]
    return sorted(e for e in ev if e[0] < DUR)


class Link:
    def __init__(self, cfg, rng):
        self.c, self.rng, self.bad, self.last = cfg, rng, False, 0.0
        self.st, self.en = [], []
        t = 0.0
        while cfg["spikes"] and t < DUR + 2:
            t += rng.expovariate(cfg["spikes"])
            self.st.append(t)
            self.en.append(t + rng.uniform(*cfg["spike"]) / 1000)

    def send(self, t):
        c = self.c
        enter, stay, bl = c["burst"]
        self.bad = self.rng.random() < (stay if self.bad else enter)
        lost = self.rng.random() < (bl if self.bad else c["loss"])
        d = (c["base"] + self.rng.expovariate(1 / c["jit"])) / 1000
        i = bisect.bisect_right(self.st, t) - 1
        if i >= 0 and t < self.en[i]:
            d += self.en[i] - t
        arr = max(t + d, self.last)
        self.last = arr
        return arr, lost


def run(link_name, strategy_name, seed):
    link_params, strategy = LINKS[link_name], STRATS[strategy_name]
    rng = random.Random(seed)
    link = Link(link_params, random.Random(seed + 1000))
    rtt = 2 * link_params["base"] / 1000
    frames, pending, seq = [], [], 0

    def push(t, pkt):
        nonlocal seq
        seq += 1
        pkt["seq"] = seq
        heapq.heappush(pending, (t, seq, pkt))

    def add_frame(t, s, kind, lane, prio_class):
        lossless = lane == "lossless"
        size = sample(rng, "patch" if lossless else kind)
        enc = (P["lossless_enc_ms"] if lossless else sample(rng, "encode")) / 1000
        dec = P["lossless_dec_ms"] if lossless else P["decode_ms"]
        n = max(1, math.ceil(size / PKT))
        par = math.ceil(n * strategy["fec"]) if strategy["fec"] else 0
        f = {
            "id": len(frames),
            "t": t,
            "screen": s,
            "kind": kind,
            "lane": lane,
            "n": n,
            "par": par,
            "sent": 0,
            "lost": 0,
            "done": None,
            "arrivals": [],
            "dec": dec,
            "prio": prio_class,
            "refine": prio_class == 9,
        }
        frames.append(f)
        for _ in range(n + par):
            push(t + enc, {"frame": f["id"], "screen": s, "retx": False, "size": PKT})

    for t, s, kind in workload(rng):
        lane = "lossless" if strategy["hybrid"] and kind == "typing" else "video"
        add_frame(t, s, kind, lane, {1: 1, 0: 2}.get(s, 3) if strategy["prio"] else 0)
        if strategy["hybrid"] and kind in ("key", "scroll"):
            add_frame(t + 0.25, s, "refine", "refine", 9)
    if strategy["local_cursor"]:
        t = 0.0
        while t < DUR:
            t += 1 / 120
            if (t % 4) < 2:
                push(t, {"frame": None, "screen": "cursor", "retx": False, "size": 40, "t0": t})

    ready, now, cursor_lat, sent_bytes, delivered = [], 0.0, [], 0, []
    while pending or ready:
        while pending and pending[0][0] <= now:
            rt, sq, pk = heapq.heappop(pending)
            pr = -1 if pk["retx"] else (0 if pk["screen"] == "cursor" else frames[pk["frame"]]["prio"])
            heapq.heappush(ready, (pr, rt, sq, pk))
        if not ready:
            now = pending[0][0]
            continue
        _, rt, sq, pk = heapq.heappop(ready)
        now = max(now, rt) + pk["size"] * 8 / (link_params["bw"] * 1e6)
        sent_bytes += pk["size"]
        arr, lost = link.send(now)
        if pk["screen"] == "cursor":
            if not lost:
                cursor_lat.append((arr - pk["t0"]) * 1000 + DISPLAY_MS)
            continue
        f = frames[pk["frame"]]
        if strategy["tcp"]:
            if lost:
                push(
                    now + (rtt + 0.002 if (ready or pending) else 0.2),
                    dict(pk, retx=True, orig=pk.get("orig", pk["seq"])),
                )
            else:
                delivered.append((pk.get("orig", pk["seq"]), arr, f["id"]))
            continue
        if pk["retx"]:
            if lost:
                f["broken"] = True
            else:
                f["arrivals"].append(arr)
                f["retx_ok"] = f.get("retx_ok", 0) + 1
                if f["retx_ok"] >= f["need"]:
                    f["done"] = max(f["arrivals"])
            continue
        f["sent"] += 1
        if lost:
            f["lost"] += 1
        else:
            f["arrivals"].append(arr)
        if f["sent"] == f["n"] + f["par"]:
            missing = f["lost"] - f["par"]
            if missing <= 0:
                f["done"] = max(f["arrivals"])
            elif strategy["nack"]:
                f["need"] = missing
                at = max(f["arrivals"] or [now]) + 0.001 + link_params["base"] / 1000
                for _ in range(missing):
                    push(at, {"frame": f["id"], "screen": f["screen"], "retx": True, "size": PKT})
            else:
                f["broken"] = True

    if strategy["tcp"]:
        delivered.sort()
        hol, done = 0.0, {}
        for _, arr, fid in delivered:
            hol = max(hol, arr)
            done[fid] = hol
        for f in frames:
            f["done"] = done.get(f["id"])

    lat, late = {s: [] for s in range(4)}, 0
    for s in range(4):
        prev, skip_until, need_key = 0.0, -1.0, False
        for f in (g for g in frames if g["screen"] == s and not g["refine"]):
            if f["lane"] == "lossless":
                if f.get("broken") or f["done"] is None:
                    late += 1
                    continue
                shown = f["done"] + f["dec"] / 1000
            else:
                if f.get("broken") or f["done"] is None:
                    late += 1
                    if strategy["ltr"]:
                        skip_until = f["t"] + rtt + 0.005
                    else:
                        need_key = True
                    continue
                if f["t"] < skip_until:
                    late += 1
                    continue
                done = f["done"]
                if need_key:
                    done += rtt + P["key"]["p50"] * 8 / (link_params["bw"] * 1e6)
                    need_key = False
                shown = max(done, prev) + f["dec"] / 1000
                prev = shown
            ms = (shown - f["t"]) * 1000 + CAPTURE_MS + DISPLAY_MS
            lat[s].append(ms)
            late += ms > 100
    if not strategy["local_cursor"]:
        cursor_lat = lat[1][:]
    return lat, cursor_lat, late, sent_bytes * 8 / DUR / 1e6


def pct(a, p):
    a = sorted(a)
    return a[min(len(a) - 1, int(p * len(a)))] if a else float("nan")


print(
    f"{'link':<17} {'design':<47} {'editor ms p50/p99':>18} {'video ms p50/p99':>17} {'cursor ms p50/p99':>18} {'late/min':>9} {'Mbps':>6}"
)
for ln in LINKS:
    for sn in STRATS:
        F, V, C, ST, BW = [], [], [], 0, 0
        for seed in range(SEEDS):
            lat, cur, st, bw = run(ln, sn, seed)
            F += lat[1]
            V += lat[0]
            C += cur
            ST += st
            BW += bw
        print(
            f"{ln:<17} {sn:<47} {pct(F, 0.5):7.1f} / {pct(F, 0.99):6.1f} {pct(V, 0.5):7.1f} / {pct(V, 0.99):6.1f} {pct(C, 0.5):8.1f} / {pct(C, 0.99):6.1f} {ST / SEEDS:9.1f} {BW / SEEDS:6.1f}"
        )
    print()
