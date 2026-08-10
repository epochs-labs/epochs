# Build epochs-server (EPX) from repo root.
#   docker build -f epochs-server/Dockerfile -t epochs-server .

FROM rust:1-bookworm AS builder
WORKDIR /src
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev \
  && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY epochs-core ./epochs-core
COPY epochql ./epochql
COPY epochs-cli ./epochs-cli
COPY epochs-bench ./epochs-bench
COPY epochs-server ./epochs-server
RUN cargo build --release -p epochs-server && strip target/release/epochs-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/* \
  && useradd -r -u 10001 -d /data epochs
COPY --from=builder /src/target/release/epochs-server /usr/local/bin/epochs-server
RUN mkdir -p /data && chown epochs:epochs /data
USER epochs
WORKDIR /data
ENV EPOCHS_DATA=/data
ENV EPOCHS_BIND=0.0.0.0:7420
EXPOSE 7420
VOLUME ["/data"]
ENTRYPOINT ["epochs-server"]
