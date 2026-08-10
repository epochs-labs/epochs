#!/usr/bin/env python3
"""Render ClickBench-style grouped bar charts from epochs-bench CSV (stdlib only)."""

from __future__ import annotations

import argparse
import math
from collections import defaultdict
from pathlib import Path

ENGINES = ["epochs", "sqlite", "postgres", "mysql"]
COLORS = {
    "epochs": "#0B6E4F",
    "sqlite": "#3D5A80",
    "postgres": "#9B2226",
    "mysql": "#BB3E03",
}


def parse_rows(path: Path) -> list[dict]:
    text = path.read_text().strip()
    if not text:
        return []
    header = None
    rows = []
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
            if row.get("shape", "deep") not in ("deep", ""):
                continue
            row["commits"] = int(float(row["commits"]))
            row["commit_per_s"] = float(row["commit_per_s"])
            row["w1_p50_us"] = float(row["w1_p50_us"])
            row["r1_p50_us"] = float(row["r1_p50_us"])
            row["r2_p50_us"] = float(row["r2_p50_us"])
            row["disk_bytes"] = float(row.get("disk_bytes") or 0)
            row["rss_bytes"] = float(row.get("rss_bytes") or 0)
        except ValueError:
            continue
        rows.append(row)
    latest: dict[tuple, dict] = {}
    for r in rows:
        latest[(r["engine"], r["commits"])] = r
    return sorted(latest.values(), key=lambda r: (r["commits"], r["engine"]))


def nice_num(x: float) -> str:
    if x >= 1e9:
        return f"{x / 1e9:.1f}B"
    if x >= 1e6:
        return f"{x / 1e6:.1f}M"
    if x >= 1e3:
        return f"{x / 1e3:.0f}k"
    if x >= 10:
        return f"{x:.0f}"
    return f"{x:.1f}"


def fmt_latency_us(us: float) -> str:
    if us >= 1_000_000:
        return f"{us / 1_000_000:.1f}s"
    if us >= 1000:
        return f"{us / 1000:.1f}ms"
    return f"{us:.0f}µs"


def svg_grouped_bars(
    title: str,
    ylab: str,
    groups: list[str],
    series: dict[str, list[float]],
    *,
    log_y: bool = False,
    value_fmt=None,
    width: int = 860,
    height: int = 440,
) -> str:
    """groups = x labels (e.g. commit counts); series[engine] = values aligned to groups."""
    pad_l, pad_r, pad_t, pad_b = 80, 24, 52, 88
    plot_w = width - pad_l - pad_r
    plot_h = height - pad_t - pad_b
    min_bar_h = 6.0  # always draw a stub so the lowest series stays visible

    engines = [e for e in ENGINES if e in series and any(v > 0 for v in series[e])]
    if not groups or not engines:
        return (
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}">'
            f'<text x="24" y="40" font-family="sans-serif">no data</text></svg>'
        )

    vals = [v for e in engines for v in series[e] if v > 0]
    vmax = max(vals)
    if log_y:
        # Floor below the smallest sample so the min value is not mapped to y=0
        # (that made epochs bars disappear on latency / R2 charts).
        ymin = max(min(vals) / 10.0, 1e-9)
        ymax = max(vmax * 1.2, ymin * 10)
    else:
        ymin = 0.0
        ymax = vmax * 1.15 if vmax > 0 else 1.0

    def ymap(y: float) -> float:
        if log_y:
            t = (math.log10(max(y, ymin)) - math.log10(ymin)) / (
                math.log10(ymax) - math.log10(ymin)
            )
        else:
            t = y / ymax if ymax else 0.0
        return pad_t + plot_h * (1 - t)

    n_groups = len(groups)
    n_eng = len(engines)
    cluster = plot_w / n_groups
    gap = cluster * 0.18
    usable = cluster - gap
    bar_w = usable / n_eng
    label_fmt = value_fmt or nice_num

    font = "IBM Plex Sans, Helvetica, Arial, sans-serif"
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">',
        '<rect width="100%" height="100%" fill="#FAFAF8"/>',
        f'<text x="{pad_l}" y="30" font-family="{font}" font-size="18" font-weight="600" fill="#1a1a1a">{title}</text>',
        f'<text x="18" y="{pad_t + plot_h / 2}" font-family="{font}" font-size="12" fill="#555" '
        f'transform="rotate(-90 18,{pad_t + plot_h / 2})">{ylab}</text>',
        f'<rect x="{pad_l}" y="{pad_t}" width="{plot_w}" height="{plot_h}" fill="#fff" stroke="#e5e5e5"/>',
    ]

    # horizontal grid
    for i in range(5):
        frac = i / 4
        if log_y:
            gy = ymin * (ymax / ymin) ** frac
        else:
            gy = ymax * frac
        y = ymap(gy)
        parts.append(
            f'<line x1="{pad_l}" y1="{y:.1f}" x2="{pad_l + plot_w}" y2="{y:.1f}" stroke="#eee"/>'
        )
        label = label_fmt(gy)
        parts.append(
            f'<text x="{pad_l - 8}" y="{y + 4:.1f}" text-anchor="end" font-family="{font}" '
            f'font-size="11" fill="#666">{label}</text>'
        )

    baseline = pad_t + plot_h
    for gi, gname in enumerate(groups):
        cx = pad_l + gi * cluster + gap / 2
        for ei, eng in enumerate(engines):
            v = series[eng][gi] if gi < len(series[eng]) else 0.0
            if v <= 0:
                continue
            x = cx + ei * bar_w
            bw = max(bar_w - 2, 1)
            y = ymap(v)
            h = max(baseline - y, min_bar_h)
            y = baseline - h
            color = COLORS.get(eng, "#333")
            parts.append(
                f'<rect x="{x:.1f}" y="{y:.1f}" width="{bw:.1f}" height="{h:.1f}" '
                f'fill="{color}" rx="2"/>'
            )
            # Value on / above the bar so tiny winners (epochs) stay readable.
            lbl = label_fmt(v)
            if h >= 22:
                ty = y + 14
                fill = "#fff"
            else:
                ty = y - 4
                fill = "#1a1a1a"
            parts.append(
                f'<text x="{x + bw / 2:.1f}" y="{ty:.1f}" text-anchor="middle" '
                f'font-family="{font}" font-size="10" font-weight="600" fill="{fill}">{lbl}</text>'
            )
        parts.append(
            f'<text x="{cx + usable / 2:.1f}" y="{baseline + 22}" text-anchor="middle" '
            f'font-family="{font}" font-size="12" fill="#444">{gname}</text>'
        )

    # legend
    lx, ly = pad_l, height - 22
    for eng in engines:
        color = COLORS[eng]
        parts.append(f'<rect x="{lx}" y="{ly - 10}" width="12" height="12" fill="{color}" rx="1"/>')
        parts.append(
            f'<text x="{lx + 18}" y="{ly}" font-family="{font}" font-size="12" fill="#333">{eng}</text>'
        )
        lx += 90

    parts.append("</svg>")
    return "\n".join(parts)


def build_grouped(rows: list[dict], field: str) -> tuple[list[str], dict[str, list[float]]]:
    commits = sorted({r["commits"] for r in rows})
    by = {(r["engine"], r["commits"]): r[field] for r in rows}
    groups = [nice_num(c) + " commits" for c in commits]
    series: dict[str, list[float]] = {}
    for eng in ENGINES:
        series[eng] = [float(by.get((eng, c), 0.0)) for c in commits]
    return groups, series


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--csv", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    rows = parse_rows(Path(args.csv))

    charts = [
        (
            "commit_throughput.svg",
            "Commit throughput (higher is better)",
            "commits / s",
            "commit_per_s",
            False,
            nice_num,
        ),
        (
            "w1_latency.svg",
            "W1 commit latency p50 (lower is better)",
            "p50",
            "w1_p50_us",
            True,
            fmt_latency_us,
        ),
        (
            "r1_history.svg",
            "R1 history walk p50 — should stay flat",
            "p50",
            "r1_p50_us",
            True,
            fmt_latency_us,
        ),
        (
            "r2_checkout.svg",
            "R2 tip checkout p50 — HAMT vs SQL replay",
            "p50",
            "r2_p50_us",
            True,
            fmt_latency_us,
        ),
        (
            "disk.svg",
            "Disk footprint",
            "bytes",
            "disk_bytes",
            True,
            nice_num,
        ),
        (
            "memory.svg",
            "Memory (cgroup RSS)",
            "bytes",
            "rss_bytes",
            True,
            nice_num,
        ),
    ]

    for fname, title, ylab, field, log_y, fmt in charts:
        groups, series = build_grouped(rows, field)
        svg = svg_grouped_bars(title, ylab, groups, series, log_y=log_y, value_fmt=fmt)
        (out / fname).write_text(svg)
        print(f"wrote {out / fname}")


if __name__ == "__main__":
    main()
