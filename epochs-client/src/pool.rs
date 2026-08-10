//! Simple async connection pool for [`Client`](crate::Client).

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

use crate::client::Client;
use crate::error::{Error, Result};
use crate::url::EpochsUrl;

struct PoolInner {
    url: String,
    max_size: usize,
    idle: Mutex<Vec<Client>>,
    permits: Arc<Semaphore>,
}

/// Builder for [`Pool`].
#[derive(Debug, Clone)]
pub struct PoolBuilder {
    url: String,
    max_size: usize,
}

impl PoolBuilder {
    /// Start a builder from an `epochs://` URL.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            max_size: 8,
        }
    }

    /// Maximum concurrent connections (default **8**).
    pub fn max_size(mut self, n: usize) -> Self {
        self.max_size = n.max(1);
        self
    }

    /// Validate the URL and build a pool (does not pre-connect).
    pub fn build(self) -> Result<Pool> {
        let parsed = EpochsUrl::parse(&self.url)?;
        let _ = parsed.to_socket_addr()?;
        Ok(Pool {
            inner: Arc::new(PoolInner {
                url: self.url,
                max_size: self.max_size,
                idle: Mutex::new(Vec::new()),
                permits: Arc::new(Semaphore::new(self.max_size)),
            }),
        })
    }
}

/// Bounded pool of EPX clients.
#[derive(Clone)]
pub struct Pool {
    inner: Arc<PoolInner>,
}

impl Pool {
    /// Builder entry point.
    pub fn builder(url: impl Into<String>) -> PoolBuilder {
        PoolBuilder::new(url)
    }

    /// Connection URL string.
    pub fn url(&self) -> &str {
        &self.inner.url
    }

    /// Configured max size.
    pub fn max_size(&self) -> usize {
        self.inner.max_size
    }

    /// Checkout a client (reuses idle or opens a new connection).
    pub async fn get(&self) -> Result<PooledClient> {
        let permit = self
            .inner
            .permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| Error::Unexpected("pool closed".into()))?;

        let client = {
            let mut idle = self.inner.idle.lock().await;
            idle.pop()
        };

        let client = match client {
            Some(c) => c,
            None => match Client::connect(&self.inner.url).await {
                Ok(c) => c,
                Err(e) => {
                    drop(permit);
                    return Err(e);
                }
            },
        };

        Ok(PooledClient {
            client: Some(client),
            pool: self.inner.clone(),
            permit: Some(permit),
            broken: false,
        })
    }
}

/// Client checked out from a [`Pool`]. Drop returns it unless marked broken.
pub struct PooledClient {
    client: Option<Client>,
    pool: Arc<PoolInner>,
    permit: Option<OwnedSemaphorePermit>,
    broken: bool,
}

impl PooledClient {
    /// Mark the connection as unusable (will not be returned to the pool).
    pub fn mark_broken(&mut self) {
        self.broken = true;
    }

    /// Run `f`; on I/O failure mark the connection broken so it is discarded.
    pub async fn with_reconnect_guard<T, F, Fut>(&mut self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Client) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        match f(self.deref_mut()).await {
            Ok(v) => Ok(v),
            Err(e) if matches!(e, Error::Io(_) | Error::Frame(_)) => {
                self.broken = true;
                Err(e)
            }
            Err(e) => Err(e),
        }
    }
}

impl Deref for PooledClient {
    type Target = Client;

    fn deref(&self) -> &Self::Target {
        self.client.as_ref().expect("pooled client taken")
    }
}

impl DerefMut for PooledClient {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.client.as_mut().expect("pooled client taken")
    }
}

impl Drop for PooledClient {
    fn drop(&mut self) {
        let permit = self.permit.take();
        let Some(client) = self.client.take() else {
            return;
        };
        if self.broken {
            drop(client);
            drop(permit);
            return;
        }
        let pool = self.pool.clone();
        if let Ok(mut idle) = pool.idle.try_lock() {
            idle.push(client);
            drop(idle);
            drop(permit);
            return;
        }
        tokio::spawn(async move {
            pool.idle.lock().await.push(client);
            drop(permit);
        });
    }
}
