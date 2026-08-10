# epochs-client

Async **Epoch Protocol (EPX)** client for [`epochs-server`](../epochs-server).

```rust
use epochs_client::{Client, Pool};

// Single connection
let mut client = Client::connect("epochs://127.0.0.1:7420").await?;
client.hello().await?;
client.refs().await?;
client
    .query("MATCH (c:Commit) SELECT c.hash, c.message;")
    .await?;

// Pool
let pool = Pool::builder("epochs://127.0.0.1:7420").max_size(8).build()?;
let mut c = pool.get().await?;
c.query("COMMIT { status: \"ok\" } MESSAGE \"hi\";").await?;
```

Protocol: [`epochs-server/docs/PROTOCOL.md`](../epochs-server/docs/PROTOCOL.md).

## License

MIT OR Apache-2.0
