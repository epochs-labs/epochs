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

Use [`epochs-client`](../epochs-client):

```rust
use epochs_client::Client;

let mut client = Client::connect("epochs://127.0.0.1:7420").await?;
client.query("MATCH (c:Commit) SELECT c.hash;").await?;
```

Or open TCP to `:7420` and speak length-prefixed JSON (`hello` / `query` / `refs` / `get`) — see [PROTOCOL.md](docs/PROTOCOL.md).

## Scaling

- **One writer** per data volume.
- Scale out by **many repos** (tenant/workspace), not by sharding one DAG — yet.
- Put behind a private network / gateway for auth.

## License

MIT OR Apache-2.0
