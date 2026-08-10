//! EPX TCP client.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::net::TcpStream;

use crate::error::{Error, Result};
use crate::frame::{read_json, write_json};
use crate::url::EpochsUrl;

#[derive(Serialize)]
struct Request<'a> {
    id: u64,
    method: &'a str,
    params: Value,
}

#[derive(Deserialize)]
struct Response {
    id: u64,
    ok: bool,
    result: Option<Value>,
    error: Option<String>,
}

/// Async EPX client over a single TCP connection.
pub struct Client {
    stream: TcpStream,
    next_id: u64,
    url: EpochsUrl,
}

/// Result of `hello`.
#[derive(Debug, Clone, Deserialize)]
pub struct Hello {
    /// Protocol name (`"epx"`).
    pub protocol: String,
    /// Protocol version.
    pub version: u64,
    /// Server package version string.
    #[serde(default)]
    pub server: String,
    /// Advertised methods.
    #[serde(default)]
    pub methods: Vec<String>,
}

/// Branch name + tip commit hash.
#[derive(Debug, Clone, Deserialize)]
pub struct BranchTip {
    /// Branch name.
    pub name: String,
    /// Tip commit hash (hex).
    pub tip: String,
}

/// Response from `query`.
#[derive(Debug, Clone, Deserialize)]
pub struct QueryResponse {
    /// EpochQL exec results (JSON), as returned by the server.
    pub results: Vec<Value>,
}

/// CAS object from `get`.
#[derive(Debug, Clone)]
pub struct CasObject {
    /// Content hash (hex).
    pub hash: String,
    /// Record type name (`commit`, `hamt_bitmap`, `hamt_leaf`).
    pub type_name: String,
    /// Raw payload bytes.
    pub payload: Vec<u8>,
}

impl Client {
    /// Connect to an `epochs://` URL (or `host:port`).
    pub async fn connect(url: &str) -> Result<Self> {
        let url = EpochsUrl::parse(url)?;
        let addr = url.to_socket_addr()?;
        let stream = TcpStream::connect(addr).await?;
        Ok(Self {
            stream,
            next_id: 1,
            url,
        })
    }

    /// Connection target.
    pub fn url(&self) -> &EpochsUrl {
        &self.url
    }

    /// `hello` handshake.
    pub async fn hello(&mut self) -> Result<Hello> {
        let v = self.rpc("hello", json!({})).await?;
        Ok(serde_json::from_value(v)?)
    }

    /// List branch tips.
    pub async fn refs(&mut self) -> Result<Vec<BranchTip>> {
        let v = self.rpc("refs", json!({})).await?;
        let branches = v
            .get("branches")
            .cloned()
            .ok_or_else(|| Error::Unexpected("missing branches".into()))?;
        Ok(serde_json::from_value(branches)?)
    }

    /// Run EpochQL (`query` method).
    pub async fn query(&mut self, sql: &str) -> Result<QueryResponse> {
        let v = self.rpc("query", json!({ "sql": sql })).await?;
        Ok(serde_json::from_value(v)?)
    }

    /// Fetch a CAS object by hex hash.
    pub async fn get(&mut self, hash_hex: &str) -> Result<CasObject> {
        let v = self.rpc("get", json!({ "hash": hash_hex })).await?;
        let hash = v
            .get("hash")
            .and_then(|x| x.as_str())
            .ok_or_else(|| Error::Unexpected("missing hash".into()))?
            .to_string();
        let type_name = v
            .get("type")
            .and_then(|x| x.as_str())
            .ok_or_else(|| Error::Unexpected("missing type".into()))?
            .to_string();
        let b64 = v
            .get("payload_base64")
            .and_then(|x| x.as_str())
            .ok_or_else(|| Error::Unexpected("missing payload_base64".into()))?;
        let payload = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)
            .map_err(|e| Error::Base64(e.to_string()))?;
        Ok(CasObject {
            hash,
            type_name,
            payload,
        })
    }

    async fn rpc(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let req = Request { id, method, params };
        write_json(&mut self.stream, &req).await?;
        let resp: Response = read_json(&mut self.stream).await?;
        if resp.id != id {
            return Err(Error::Unexpected(format!(
                "response id {} != request id {id}",
                resp.id
            )));
        }
        if !resp.ok {
            return Err(Error::Server(
                resp.error.unwrap_or_else(|| "unknown error".into()),
            ));
        }
        resp.result
            .ok_or_else(|| Error::Unexpected("ok response missing result".into()))
    }
}
