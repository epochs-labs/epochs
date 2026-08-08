#!/usr/bin/env bash
# Fair Docker bench (2 CPU / 2 GiB), sequential. Default shape=deep.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

TIER="${1:-smoke}"
SHAPE="${SHAPE:-deep}"
PROGRESS_EVERY="${PROGRESS_EVERY:-1000}"
EXTRA_ARGS="${EXTRA_ARGS:-}"
ENGINES="${ENGINES:-epochs sqlite postgres mysql}"

mkdir -p benches/out
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
SNAPSHOT="benches/out/results-${TIER}-${SHAPE}-${STAMP}.csv"
: >benches/out/results.csv

export TIER SHAPE PROGRESS_EVERY EXTRA_ARGS

echo "=== epochs-bench fair Docker ==="
echo "tier=$TIER shape=$SHAPE limits=2CPU/2GiB engines=$ENGINES"
echo

docker compose -f benches/docker-compose.yml build

for eng in $ENGINES; do
  echo
  echo "──────── $eng / $TIER ($SHAPE) ────────"
  docker compose -f benches/docker-compose.yml run --rm --no-deps \
    -e TIER="$TIER" \
    -e SHAPE="$SHAPE" \
    -e PROGRESS_EVERY="$PROGRESS_EVERY" \
    -e EXTRA_ARGS="$EXTRA_ARGS" \
    -e RESULTS_CSV=/results/results.csv \
    "$eng"
done

cp benches/out/results.csv "$SNAPSHOT"
echo
echo "wrote benches/out/results.csv → $SNAPSHOT"
