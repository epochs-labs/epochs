#!/usr/bin/env python3
"""Parse cargo-llvm-cov summary, write baseline JSON, gate on floor / regression."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


def parse_line_percent(summary_text: str) -> float:
    pct = None
    for line in summary_text.splitlines():
        if not line.strip().startswith("TOTAL"):
            continue
        # First % on the TOTAL row is region/line coverage from llvm-cov summary.
        m = re.search(r"([\d.]+)%", line)
        if m:
            pct = float(m.group(1))
    if pct is None:
        raise SystemExit("could not parse TOTAL coverage % from llvm-cov summary")
    return pct


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--summary", type=Path, required=True)
    ap.add_argument("--out-json", type=Path, required=True)
    ap.add_argument("--floor-file", type=Path, default=Path(".github/coverage-floor"))
    ap.add_argument("--baseline-json", type=Path, default=None)
    ap.add_argument(
        "--max-drop",
        type=float,
        default=1.0,
        help="Max allowed drop in percentage points vs main baseline",
    )
    ap.add_argument("--commit", default="")
    args = ap.parse_args()

    summary = args.summary.read_text()
    pct = parse_line_percent(summary)
    payload = {"line_percent": pct, "commit": args.commit}
    args.out_json.write_text(json.dumps(payload, indent=2) + "\n")
    print(f"coverage.line_percent={pct}")

    floor = 0.0
    if args.floor_file.is_file():
        raw = None
        for line in args.floor_file.read_text().splitlines():
            s = line.strip()
            if not s or s.startswith("#"):
                continue
            raw = s.split()[0]
            break
        if raw is None:
            raise SystemExit(f"no coverage floor value in {args.floor_file}")
        floor = float(raw)
        print(f"coverage.floor={floor}")
        if pct + 1e-9 < floor:
            print(
                f"::error::Line coverage {pct:.2f}% is below floor {floor:.2f}% "
                f"(see {args.floor_file})"
            )
            raise SystemExit(1)

    if args.baseline_json and args.baseline_json.is_file():
        base = json.loads(args.baseline_json.read_text())
        base_pct = float(base["line_percent"])
        drop = base_pct - pct
        print(f"coverage.baseline={base_pct}")
        print(f"coverage.drop={drop:.2f}")
        if drop > args.max_drop + 1e-9:
            print(
                f"::error::Line coverage dropped {drop:.2f}pp "
                f"({base_pct:.2f}% → {pct:.2f}%; max allowed {args.max_drop:.2f}pp)"
            )
            raise SystemExit(1)
    else:
        print("coverage.baseline=none")


if __name__ == "__main__":
    main()
