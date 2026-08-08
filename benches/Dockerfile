# Fair, reproducible epochs-bench images.
# Build from repo root: docker compose -f benches/docker-compose.yml build
#
# Every engine runs under the same cpus / mem_limit. For SQL engines the
# server and bench client share that one cgroup (not "unlimited host client").

FROM rust:1-bookworm AS builder
WORKDIR /src
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev \
  && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY epochs-core ./epochs-core
COPY epochql ./epochql
COPY epochs-cli ./epochs-cli
COPY epochs-bench ./epochs-bench
RUN cargo build --release -p epochs-bench \
  && strip target/release/epochs-bench

# epochs + sqlite (in-process)
FROM debian:bookworm-slim AS embedded
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/epochs-bench /usr/local/bin/epochs-bench
COPY benches/docker/entrypoint-embedded.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh
WORKDIR /data
ENTRYPOINT ["/entrypoint.sh"]

# Postgres server + bench client in ONE container (shared cgroup)
FROM postgres:16-bookworm AS postgres
COPY --from=builder /src/target/release/epochs-bench /usr/local/bin/epochs-bench
COPY benches/docker/entrypoint-postgres.sh /entrypoint-bench.sh
RUN chmod +x /entrypoint-bench.sh
ENTRYPOINT ["/entrypoint-bench.sh"]

# MySQL-protocol server (MariaDB on Debian) + bench client in ONE cgroup.
# Same base OS family as embedded/postgres images for glibc consistency.
FROM debian:bookworm-slim AS mysql
RUN apt-get update && apt-get install -y --no-install-recommends \
    mariadb-server mariadb-client ca-certificates \
  && rm -rf /var/lib/apt/lists/* \
  && mysql_install_db --user=mysql --datadir=/var/lib/mysql >/dev/null 2>&1 || true
COPY --from=builder /src/target/release/epochs-bench /usr/local/bin/epochs-bench
COPY benches/docker/entrypoint-mysql.sh /entrypoint-bench.sh
RUN chmod +x /entrypoint-bench.sh
ENTRYPOINT ["/entrypoint-bench.sh"]
