#!/bin/bash
# epochs or sqlite under the container cgroup.
set -euo pipefail

ENGINE="${ENGINE:?ENGINE=epochs|sqlite required}"
RESULTS_CSV="${RESULTS_CSV:-/results/results.csv}"
TIER="${TIER:-smoke}"
SHAPE="${SHAPE:-deep}"
PROGRESS_EVERY="${PROGRESS_EVERY:-1000}"
EXTRA_ARGS="${EXTRA_ARGS:-}"

mkdir -p /results /data
rm -rf /data/*
echo "→ $ENGINE ready (tier=$TIER shape=$SHAPE, cgroup-limited)"
# shellcheck disable=SC2086
exec epochs-bench \
  --engine "$ENGINE" \
  --tier "$TIER" \
  --shape "$SHAPE" \
  --data-dir /data \
  --csv "$RESULTS_CSV" \
  --progress-every "$PROGRESS_EVERY" \
  $EXTRA_ARGS
