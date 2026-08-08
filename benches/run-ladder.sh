#!/usr/bin/env bash
# Deep-history scale ladder (fair Docker). Default shape=deep.
#
#   ./benches/run-ladder.sh           # smoke + dev all; mid embedded; large epochs
#   ./benches/run-ladder.sh --quick   # smoke + dev only
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

QUICK=0
[[ "${1:-}" == "--quick" ]] && QUICK=1

SHAPE="${SHAPE:-deep}"
mkdir -p benches/out benches/charts
CSV="benches/out/ladder.csv"
: >"$CSV"

run_one() {
  local eng="$1" tier="$2" extra="${3:-}"
  echo
  echo "════════ $eng / $tier ($SHAPE) $extra ════════"
  # shellcheck disable=SC2086
  docker compose -f benches/docker-compose.yml run --rm --no-deps \
    -e TIER="$tier" \
    -e SHAPE="$SHAPE" \
    -e PROGRESS_EVERY="${PROGRESS_EVERY:-10000}" \
    -e RESULTS_CSV=/results/ladder.csv \
    -e EXTRA_ARGS="$extra" \
    "$eng"
}

echo "=== deep-history scale ladder (fair Docker) ==="
docker compose -f benches/docker-compose.yml build

for tier in smoke dev; do
  for eng in epochs sqlite postgres mysql; do
    run_one "$eng" "$tier"
  done
done

if [[ "$QUICK" -eq 0 ]]; then
  # mid: all engines (1M commits × 10k keys — SQL tip replay is the story)
  for eng in epochs sqlite postgres mysql; do
    run_one "$eng" mid
  done
  # large: embedded (5M × 100k keys)
  for eng in epochs sqlite; do
    run_one "$eng" large
  done
fi

if [[ "${INCLUDE_HEAVY:-0}" == "1" ]]; then
  run_one epochs heavy "--force"
fi

python3 benches/charts/render.py --csv "$CSV" --out benches/charts
echo "=== done → $CSV + benches/charts/ ==="
