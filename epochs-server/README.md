# epochs-server

**Epoch Protocol (EPX)** TCP server for [epochs](https://github.com/epochs-labs/epochs).

| | |
|--|--|
| Listen | `:7420` (default) |
| URL | `epochs://host:7420` |
| Spec | [docs/PROTOCOL.md](docs/PROTOCOL.md) |

Single-writer `DiskStore` behind a mutex — many TCP clients queue on the lock.

## Run

```bash
cargo run -p epochs-server -- --data-dir /tmp/epochs-data

# Docker
docker compose -f epochs-server/docker-compose.yml up --build
```

Env: `EPOCHS_DATA`, `EPOCHS_BIND` (or `--data-dir` / `--bind`).

## Connect

Open TCP to `:7420`, send length-prefixed JSON frames (`hello` / `query` / `refs` / `get`). See PROTOCOL.md. A typed `epochs-client` crate (with optional pooling) can wrap this later.

## Scaling

- **One writer** per data volume.
- Scale out by **many repos** (tenant/workspace), not by sharding one DAG — yet.
- Put behind a private network / gateway for auth.

## License

MIT OR Apache-2.0
