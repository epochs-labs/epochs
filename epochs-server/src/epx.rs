//! Epoch Protocol (EPX) — TCP length-prefixed JSON for Merkle-DAG ops.
//!
//! Framing: `u32` big-endian payload length, then UTF-8 JSON.
//! Connection string shape: `epochs://host:7420` (not HTTP).
//!
//! Methods are content-address / VCS shaped — not SQL row CRUD:
//! `hello`, `query`, `refs`, `get` (CAS object by hash).

use std::net::SocketAddr;

use epochs_core::{Hash, RecordType};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, warn};

use crate::state::{list_branch_tips, run_query, AppState, JsonExecResult};

const MAX_FRAME: u32 = 16 * 1024 * 1024;

#[derive(Deserialize)]
struct Request {
    id: u64,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Serialize)]
struct Response {
    id: u64,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Bind `addr` and serve EPX until the listener fails.
pub async fn serve(addr: SocketAddr, state: AppState) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    serve_listener(listener, state).await
}

/// Serve EPX on an already-bound listener (useful for tests with port `0`).
pub async fn serve_listener(listener: TcpListener, state: AppState) -> std::io::Result<()> {
    let addr = listener.local_addr()?;
    info!(%addr, "EPX (Epoch Protocol) listening — epochs://host:port");
    loop {
        let (socket, peer) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_conn(socket, state).await {
                warn!(%peer, error = %e, "EPX connection closed with error");
            }
        });
    }
}

async fn handle_conn(mut socket: TcpStream, state: AppState) -> Result<(), String> {
    loop {
        let req = match read_frame(&mut socket).await {
            Ok(None) => return Ok(()),
            Ok(Some(bytes)) => bytes,
            Err(e) => return Err(e),
        };
        let parsed: Request = serde_json::from_slice(&req).map_err(|e| e.to_string())?;
        let resp = dispatch(&state, parsed).await;
        write_frame(&mut socket, &resp).await?;
    }
}

async fn dispatch(state: &AppState, req: Request) -> Response {
    let id = req.id;
    let result = match req.method.as_str() {
        "hello" => Ok(json!({
            "protocol": "epx",
            "version": 1,
            "server": env!("CARGO_PKG_VERSION"),
            "methods": ["hello", "query", "refs", "get"],
        })),
        "query" => {
            let sql = req
                .params
                .get("sql")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if sql.is_empty() {
                Err("params.sql required".into())
            } else {
                let mut guard = state.store.lock().await;
                run_query(&mut guard, &sql)
                    .map(|results| json!({ "results": results_to_json(results) }))
            }
        }
        "refs" => {
            let mut guard = state.store.lock().await;
            list_branch_tips(&mut guard).map(|branches| {
                json!({
                    "branches": branches.into_iter().map(|(name, tip)| {
                        json!({ "name": name, "tip": tip })
                    }).collect::<Vec<_>>()
                })
            })
        }
        "get" => {
            let hash_hex = req
                .params
                .get("hash")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if hash_hex.is_empty() {
                Err("params.hash required".into())
            } else {
                get_cas_object(state, hash_hex).await
            }
        }
        other => Err(format!("unknown method '{other}'")),
    };

    match result {
        Ok(value) => Response {
            id,
            ok: true,
            result: Some(value),
            error: None,
        },
        Err(error) => Response {
            id,
            ok: false,
            result: None,
            error: Some(error),
        },
    }
}

async fn get_cas_object(state: &AppState, hash_hex: &str) -> Result<serde_json::Value, String> {
    let hash = Hash::from_hex(hash_hex).map_err(|e| e.to_string())?;
    let mut guard = state.store.lock().await;
    let (ty, payload) = guard.get_object(&hash).map_err(|e| e.to_string())?;
    Ok(json!({
        "hash": hash.to_string(),
        "type": record_type_name(ty),
        "payload_base64": base64_encode(&payload),
        "len": payload.len(),
    }))
}

fn record_type_name(ty: RecordType) -> &'static str {
    match ty {
        RecordType::Commit => "commit",
        RecordType::HamtBitmap => "hamt_bitmap",
        RecordType::HamtLeaf => "hamt_leaf",
    }
}

fn results_to_json(results: Vec<JsonExecResult>) -> serde_json::Value {
    match serde_json::to_value(results) {
        Ok(v) => v,
        Err(_) => json!([]),
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    // Minimal base64 (stdlib) — avoid extra dep for v1.
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0] as u32;
        let b = chunk.get(1).copied().unwrap_or(0) as u32;
        let c = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (a << 16) | (b << 8) | c;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(T[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(T[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

async fn read_frame(socket: &mut TcpStream) -> Result<Option<Vec<u8>>, String> {
    let mut len_buf = [0u8; 4];
    match socket.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.to_string()),
    }
    let len = u32::from_be_bytes(len_buf);
    if len == 0 {
        return Ok(Some(Vec::new()));
    }
    if len > MAX_FRAME {
        return Err(format!("frame too large: {len}"));
    }
    let mut buf = vec![0u8; len as usize];
    socket
        .read_exact(&mut buf)
        .await
        .map_err(|e| e.to_string())?;
    Ok(Some(buf))
}

async fn write_frame(socket: &mut TcpStream, resp: &Response) -> Result<(), String> {
    let body = serde_json::to_vec(resp).map_err(|e| e.to_string())?;
    let len = body.len() as u32;
    if len > MAX_FRAME {
        return Err("response too large".into());
    }
    socket
        .write_all(&len.to_be_bytes())
        .await
        .map_err(|e| e.to_string())?;
    socket.write_all(&body).await.map_err(|e| e.to_string())?;
    socket.flush().await.map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::base64_encode;

    #[test]
    fn base64_empty() {
        assert_eq!(base64_encode(b""), "");
    }

    #[test]
    fn base64_known_vectors() {
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }
}
