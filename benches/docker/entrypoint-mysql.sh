#!/bin/bash
# All-in-one MariaDB (MySQL protocol) + epochs-bench under a single cgroup.
set -euo pipefail

DATADIR="${MYSQL_DATADIR:-/var/lib/mysql-bench}"
RESULTS_CSV="${RESULTS_CSV:-/results/results.csv}"
TIER="${TIER:-smoke}"
PROGRESS_EVERY="${PROGRESS_EVERY:-1000}"
EXTRA_ARGS="${EXTRA_ARGS:-}"
SOCKET="${MYSQL_UNIX_PORT:-/run/mysqld/mysqld.sock}"

rm -rf "$DATADIR"
mkdir -p "$DATADIR" /run/mysqld /results
chown -R mysql:mysql "$DATADIR" /run/mysqld

mysql_install_db --user=mysql --datadir="$DATADIR" >/tmp/mysql-init.log 2>&1

mysqld --user=mysql --datadir="$DATADIR" --socket="$SOCKET" \
  --bind-address=127.0.0.1 --port=3306 \
  --innodb-flush-log-at-trx-commit=1 \
  --innodb-buffer-pool-size=64M \
  --key-buffer-size=8M \
  --max-connections=20 \
  >/tmp/mysqld.log 2>&1 &

for i in $(seq 1 90); do
  if mysqladmin --socket="$SOCKET" ping -uroot --silent 2>/dev/null; then
    break
  fi
  sleep 1
done
mysqladmin --socket="$SOCKET" ping -uroot --silent

mysql --socket="$SOCKET" -uroot <<SQL
CREATE DATABASE bench;
CREATE USER 'bench'@'localhost' IDENTIFIED BY 'bench';
CREATE USER 'bench'@'127.0.0.1' IDENTIFIED BY 'bench';
GRANT ALL PRIVILEGES ON bench.* TO 'bench'@'localhost';
GRANT ALL PRIVILEGES ON bench.* TO 'bench'@'127.0.0.1';
FLUSH PRIVILEGES;
SQL

cleanup() {
  mysqladmin --socket="$SOCKET" -uroot shutdown >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "→ mysql/mariadb stack ready (tier=$TIER shape=${SHAPE:-deep}, cgroup-limited with server+bench)"
# shellcheck disable=SC2086
epochs-bench \
  --engine mysql \
  --tier "$TIER" \
  --shape "${SHAPE:-deep}" \
  --mysql-url "mysql://bench:bench@127.0.0.1:3306/bench" \
  --data-dir /data \
  --csv "$RESULTS_CSV" \
  --progress-every "$PROGRESS_EVERY" \
  $EXTRA_ARGS
