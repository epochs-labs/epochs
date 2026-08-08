#!/usr/bin/env python3
"""Render scale-ladder SVG charts from epochs-bench CSV (stdlib only)."""

from __future__ import annotations

import argparse
import math
from collections import defaultdict
from pathlib import Path

COLORS = {
    "epochs": "#0B6E4F",
    "sqlite": "#3D5A80",
    "postgres": "#9B2226",
    "mysql": "#BB3E03",
}


def parse_rows(path: Path) -> list[dict]:
    rows = []
    text = path.read_text().strip()
    if not text:
        return rows
    header = None
    for line in text.splitlines():
        if line.startswith("engine,"):
            header = line.split(",")
            continue
        if not header or not line.strip():
            continue
        parts = line.split(",")
        if len(parts) < len(header):
            continue
        row = dict(zip(header, parts))
        try:
            # new schema
            if "live_keys" in row:
                row["live_keys"] = int(float(row["live_keys"]))
            row["commits"] = int(float(row["commits"]))
            row["commit_per_s"] = float(row["commit_per_s"])
            row["w1_p50_us"] = float(row["w1_p50_us"])
            row["r1_p50_us"] = float(row["r1_p50_us"])
            row["r2_p50_us"] = float(row["r2_p50_us"])
            row["disk_bytes"] = float(row.get("disk_bytes") or 0)
            row["rss_bytes"] = float(row.get("rss_bytes") or 0)
            row.setdefault("shape", "deep")
        except ValueError:
            continue
        # Prefer deep shape for published charts
        if row.get("shape", "deep") not in ("deep", ""):
            continue
        rows.append(row)
    latest: dict[tuple, dict] = {}
    for r in rows:
        latest[(r["engine"], r["commits"])] = r
    return sorted(latest.values(), key=lambda r: (r["commits"], r["engine"]))


def nice_num(x: float) -> str:
    if x >= 1e9:
        return f"{x/1e9:.1f}B"
    if x >= 1e6:
        return f"{x/1e6:.1f}M"
    if x >= 1e3:
        return f"{x/1e3:.0f}k"
    return f"{x:.0f}"


def svg_line_chart(
    title: str,
    series: dict[str, list[tuple[float, float]]],
    ylab: str,
    log_y: bool = True,
    width: int = 720,
    height: int = 420,
) -> str:
    pad_l, pad_r, pad_t, pad_b = 72, 24, 48, 56
    plot_w = width - pad_l - pad_r
    plot_h = height - pad_t - pad_b

    xs = [p[0] for pts in series.values() for p in pts]
    ys = [p[1] for pts in series.values() for p in pts if p[1] > 0]
    if not xs or not ys:
        return (
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}">'
            f'<text x="20" y="40" font-family="sans-serif">no data yet — run ./benches/run-ladder.sh</text></svg>'
        )

    xmin, xmax = min(xs), max(xs)
    ymin, ymax = min(ys), max(ys)
    if log_y:
        ymin = max(ymin, 1e-9)
        ymax = max(ymax, ymin * 10)

    def xmap(x: float) -> float:
        if xmax == xmin:
            return pad_l + plot_w / 2
        lx0, lx1 = math.log10(max(xmin, 1)), math.log10(max(xmax, 1))
        return pad_l + (math.log10(max(x, 1)) - lx0) / (lx1 - lx0) * plot_w

    def ymap(y: float) -> float:
        if log_y:
            ly0, ly1 = math.log10(ymin), math.log10(ymax)
            t = (math.log10(max(y, ymin)) - ly0) / (ly1 - ly0)
        else:
            t = (y - ymin) / (ymax - ymin) if ymax != ymin else 0.5
        return pad_t + plot_h * (1 - t)

    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">',
        '<rect width="100%" height="100%" fill="#FAFAF8"/>',
        f'<text x="{pad_l}" y="28" font-family="IBM Plex Sans, Helvetica, Arial, sans-serif" font-size="17" font-weight="600" fill="#1a1a1a">{title}</text>',
        f'<text x="16" y="{pad_t + plot_h/2}" font-family="IBM Plex Sans, Helvetica, Arial, sans-serif" font-size="12" fill="#555" transform="rotate(-90 16,{pad_t + plot_h/2})">{ylab}</text>',
        f'<text x="{pad_l + plot_w/2}" y="{height - 12}" text-anchor="middle" font-family="IBM Plex Sans, Helvetica, Arial, sans-serif" font-size="12" fill="#555">commits (log) — deep shape</text>',
        f'<rect x="{pad_l}" y="{pad_t}" width="{plot_w}" height="{plot_h}" fill="none" stroke="#ddd"/>',
    ]

    for c in sorted({p[0] for pts in series.values() for p in pts}):
        x = xmap(c)
        parts.append(f'<line x1="{x:.1f}" y1="{pad_t}" x2="{x:.1f}" y2="{pad_t+plot_h}" stroke="#eee"/>')
        parts.append(
            f'<text x="{x:.1f}" y="{pad_t+plot_h+18}" text-anchor="middle" font-family="IBM Plex Sans, Helvetica, Arial, sans-serif" font-size="11" fill="#666">{nice_num(c)}</text>'
        )

    legend_x, legend_y = pad_l + 8, pad_t + 16
    for eng, pts in sorted(series.items()):
        pts = sorted(pts)
        if not pts:
            continue
        color = COLORS.get(eng, "#333")
        d = "M " + " L ".join(f"{xmap(x):.1f},{ymap(y):.1f}" for x, y in pts)
        parts.append(f'<path d="{d}" fill="none" stroke="{color}" stroke-width="2.5"/>')
        for x, y in pts:
            parts.append(f'<circle cx="{xmap(x):.1f}" cy="{ymap(y):.1f}" r="3.5" fill="{color}"/>')
        parts.append(
            f'<rect x="{legend_x}" y="{legend_y-8}" width="12" height="12" fill="{color}"/>'
            f'<text x="{legend_x+18}" y="{legend_y+2}" font-family="IBM Plex Sans, Helvetica, Arial, sans-serif" font-size="12" fill="#333">{eng}</text>'
        )
        legend_y += 18

    parts.append("</svg>")
    return "\n".join(parts)


def build_series(rows: list[dict], field: str) -> dict[str, list[tuple[float, float]]]:
    out: dict[str, list[tuple[float, float]]] = defaultdict(list)
    for r in rows:
        v = r[field]
        if v <= 0:
            continue
        out[r["engine"]].append((float(r["commits"]), float(v)))
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--csv", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    rows = parse_rows(Path(args.csv))

    charts = [
        ("commit_throughput.svg", "Commit throughput vs history depth", "commit_per_s", "commits / s"),
        ("w1_latency.svg", "W1 commit latency (p50) vs history depth", "w1_p50_us", "p50 µs"),
        ("r1_history.svg", "R1 history walk (p50) — should stay flat", "r1_p50_us", "p50 µs"),
        ("r2_checkout.svg", "R2 tip checkout (p50) — SQL replay vs HAMT", "r2_p50_us", "p50 µs"),
        ("disk.svg", "Disk footprint vs history depth", "disk_bytes", "bytes"),
        ("memory.svg", "Memory (cgroup) vs history depth", "rss_bytes", "bytes"),
    ]
    for fname, title, field, ylab in charts:
        svg = svg_line_chart(title, build_series(rows, field), ylab, log_y=True)
        (out / fname).write_text(svg)
        print(f"wrote {out / fname}")


if __name__ == "__main__":
    main()
