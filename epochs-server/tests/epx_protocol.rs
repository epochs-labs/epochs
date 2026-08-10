//! EPX protocol integration tests — framed JSON over TCP.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use epochs_server::epx;
use epochs_server::state::{open_or_init, AppState};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("epochs_server_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

async fn start_server(
    data: &std::path::Path,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let store = open_or_init(data, "main").expect("open_or_init");
    let state = AppState {
        store: Arc::new(Mutex::new(store)),
    };
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral");
    let addr = listener.local_addr().expect("local_addr");
    let handle = tokio::spawn(async move {
        let _ = epx::serve_listener(listener, state).await;
    });
    // Brief settle so accept loop is ready.
    tokio::time::sleep(Duration::from_millis(20)).await;
    (addr, handle)
}

async fn rpc(stream: &mut TcpStream, id: u64, method: &str, params: Value) -> Value {
    let req = json!({ "id": id, "method": method, "params": params });
    let body = serde_json::to_vec(&req).expect("serialize");
    stream
        .write_all(&(body.len() as u32).to_be_bytes())
        .await
        .expect("write len");
    stream.write_all(&body).await.expect("write body");
    stream.flush().await.expect("flush");

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await.expect("read len");
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await.expect("read body");
    serde_json::from_slice(&buf).expect("parse response")
}

#[tokio::test]
async fn hello_refs_query_get_roundtrip() {
    let dir = temp_dir("roundtrip");
    let (addr, _server) = start_server(&dir).await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    let hello = rpc(&mut stream, 1, "hello", json!({})).await;
    assert_eq!(hello["ok"], true);
    assert_eq!(hello["result"]["protocol"], "epx");
    assert_eq!(hello["result"]["version"], 1);
    assert!(hello["result"]["methods"]
        .as_array()
        .expect("methods")
        .iter()
        .any(|m| m == "get"));

    let refs = rpc(&mut stream, 2, "refs", json!({})).await;
    assert_eq!(refs["ok"], true);
    let tip = refs["result"]["branches"][0]["tip"]
        .as_str()
        .expect("tip")
        .to_string();
    assert_eq!(tip.len(), 64);

    let commit = rpc(
        &mut stream,
        3,
        "query",
        json!({ "sql": "COMMIT { status: \"ok\" } MESSAGE \"from test\";" }),
    )
    .await;
    assert_eq!(commit["ok"], true);
    assert_eq!(commit["result"]["results"][0]["type"], "mutation");
    let new_hash = commit["result"]["results"][0]["hash"]
        .as_str()
        .expect("hash")
        .to_string();

    let got = rpc(&mut stream, 4, "get", json!({ "hash": new_hash })).await;
    assert_eq!(got["ok"], true);
    assert_eq!(got["result"]["hash"], new_hash);
    assert_eq!(got["result"]["type"], "commit");
    assert!(got["result"]["len"].as_u64().unwrap_or(0) > 0);
    assert!(!got["result"]["payload_base64"]
        .as_str()
        .unwrap_or("")
        .is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn unknown_method_and_missing_params_are_errors() {
    let dir = temp_dir("errors");
    let (addr, _server) = start_server(&dir).await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    let bad = rpc(&mut stream, 1, "nope", json!({})).await;
    assert_eq!(bad["ok"], false);
    assert!(bad["error"]
        .as_str()
        .unwrap_or("")
        .contains("unknown method"));

    let missing_sql = rpc(&mut stream, 2, "query", json!({})).await;
    assert_eq!(missing_sql["ok"], false);
    assert!(missing_sql["error"]
        .as_str()
        .unwrap_or("")
        .contains("params.sql"));

    let missing_hash = rpc(&mut stream, 3, "get", json!({})).await;
    assert_eq!(missing_hash["ok"], false);
    assert!(missing_hash["error"]
        .as_str()
        .unwrap_or("")
        .contains("params.hash"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn reopen_existing_repo() {
    let dir = temp_dir("reopen");
    {
        let _ = open_or_init(&dir, "main").expect("init");
    }
    let (addr, _server) = start_server(&dir).await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let refs = rpc(&mut stream, 1, "refs", json!({})).await;
    assert_eq!(refs["ok"], true);
    assert_eq!(refs["result"]["branches"][0]["name"], "main");
    let _ = std::fs::remove_dir_all(&dir);
}
