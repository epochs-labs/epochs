#!/bin/bash
# All-in-one Postgres + epochs-bench under a single cgroup (fair vs embedded).
set -euo pipefail

# Official image sets PGDATA=/var/lib/postgresql/data (often a VOLUME). Use a
# fresh path so we can wipe between runs without "Device or resource busy".
PGDATA=/var/lib/postgresql/bench-data
RESULTS_CSV="${RESULTS_CSV:-/results/results.csv}"
TIER="${TIER:-smoke}"
PROGRESS_EVERY="${PROGRESS_EVERY:-1000}"
EXTRA_ARGS="${EXTRA_ARGS:-}"

export PGDATA
rm -rf "$PGDATA"
mkdir -p "$PGDATA" /results /var/run/postgresql
chown -R postgres:postgres "$PGDATA" /var/run/postgresql

# Init + start as postgres user (official image layout).
gosu postgres initdb -D "$PGDATA" --auth-host=trust --auth-local=trust >/dev/null
cat >> "$PGDATA/postgresql.conf" <<EOF
listen_addresses = '127.0.0.1'
port = 5432
shared_buffers = 64MB
effective_cache_size = 128MB
work_mem = 4MB
max_connections = 20
fsync = on
synchronous_commit = on
EOF

gosu postgres pg_ctl -D "$PGDATA" -o "-c listen_addresses=127.0.0.1" -w start
gosu postgres psql -v ON_ERROR_STOP=1 -d postgres <<SQL
CREATE USER bench WITH PASSWORD 'bench' SUPERUSER;
CREATE DATABASE bench OWNER bench;
SQL

cleanup() {
  gosu postgres pg_ctl -D "$PGDATA" -m fast -w stop >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "→ postgres stack ready (tier=$TIER shape=${SHAPE:-deep}, cgroup-limited with server+bench)"
# shellcheck disable=SC2086
epochs-bench \
  --engine postgres \
  --tier "$TIER" \
  --shape "${SHAPE:-deep}" \
  --postgres-url "postgres://bench:bench@127.0.0.1:5432/bench" \
  --data-dir /data \
  --csv "$RESULTS_CSV" \
  --progress-every "$PROGRESS_EVERY" \
  $EXTRA_ARGS
