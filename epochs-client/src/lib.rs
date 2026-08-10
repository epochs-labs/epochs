//! Async Epoch Protocol (EPX) client for [`epochs-server`](https://github.com/epochs-labs/epochs).
//!
//! ```no_run
//! # async fn demo() -> Result<(), epochs_client::Error> {
//! use epochs_client::Client;
//!
//! let mut client = Client::connect("epochs://127.0.0.1:7420").await?;
//! let hello = client.hello().await?;
//! assert_eq!(hello.protocol, "epx");
//! # Ok(())
//! # }
//! ```

#![deny(missing_docs)]

mod client;
mod error;
mod frame;
mod pool;
mod url;

pub use client::{BranchTip, CasObject, Client, Hello, QueryResponse};
pub use error::{Error, Result};
pub use pool::{Pool, PoolBuilder, PooledClient};
pub use url::EpochsUrl;
