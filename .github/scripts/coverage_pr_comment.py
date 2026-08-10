#!/usr/bin/env python3
"""Post or update sticky coverage comment on a pull request (GitHub CLI)."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

MARKER = "<!-- epochs-coverage -->"


def floor_value(path: Path) -> str:
    for line in path.read_text().splitlines():
        s = line.strip()
        if s and not s.startswith("#"):
            return s.split()[0]
    return "?"


def main() -> None:
    if not Path("coverage.json").is_file():
        print("No coverage.json; skip comment")
        return

    pct = json.loads(Path("coverage.json").read_text())["line_percent"]
    floor = floor_value(Path(".github/coverage-floor"))
    baseline = Path("baseline/coverage.json")
    if baseline.is_file():
        base = json.loads(baseline.read_text())["line_percent"]
        delta = round(pct - base, 2)
        base_line = f"| vs `main` | {base}% → **{pct}%** (Δ {delta}pp) |"
    else:
        base_line = (
            "| vs `main` | _(no baseline artifact yet — next main push establishes one)_ |"
        )

    body = "\n".join(
        [
            MARKER,
            "## Coverage report",
            "",
            "| | |",
            "|--|--|",
            f"| **Line coverage** | **{pct}%** |",
            f"| Floor | {floor}% |",
            base_line,
            "",
            "Source: `cargo llvm-cov` (LLVM instrumentation). "
            "Full table is in the **coverage** job summary.",
            "",
        ]
    )

    repo = os.environ["GITHUB_REPOSITORY"]
    pr = os.environ["PR_NUMBER"]
    list_out = subprocess.check_output(
        ["gh", "api", f"repos/{repo}/issues/{pr}/comments"],
        text=True,
    )
    comment_id = None
    for c in json.loads(list_out):
        if MARKER in c.get("body", ""):
            comment_id = c["id"]
            break

    if comment_id is not None:
        subprocess.check_call(
            [
                "gh",
                "api",
                "-X",
                "PATCH",
                f"repos/{repo}/issues/comments/{comment_id}",
                "-f",
                f"body={body}",
            ]
        )
        print(f"Updated comment {comment_id}")
    else:
        subprocess.check_call(["gh", "pr", "comment", pr, "--body", body])
        print("Created coverage comment")


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as e:
        print(f"comment failed: {e}", file=sys.stderr)
        raise SystemExit(e.returncode)
