//! EPX client integration tests against a live `epochs-server`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::sync::Mutex;

use epochs_client::{Client, Error, Pool};
use epochs_server::epx;
use epochs_server::state::{open_or_init, AppState};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("epochs_client_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

async fn start_server(data: &std::path::Path) -> (String, tokio::task::JoinHandle<()>) {
    let store = open_or_init(data, "main").expect("open_or_init");
    let state = AppState {
        store: Arc::new(Mutex::new(store)),
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let handle = tokio::spawn(async move {
        let _ = epx::serve_listener(listener, state).await;
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    (format!("epochs://{addr}"), handle)
}

#[tokio::test]
async fn client_hello_refs_query_get() {
    let dir = temp_dir("roundtrip");
    let (url, _server) = start_server(&dir).await;

    let mut client = Client::connect(&url).await.expect("connect");
    let hello = client.hello().await.expect("hello");
    assert_eq!(hello.protocol, "epx");
    assert_eq!(hello.version, 1);
    assert!(hello.methods.iter().any(|m| m == "get"));

    let refs = client.refs().await.expect("refs");
    assert_eq!(refs[0].name, "main");
    assert_eq!(refs[0].tip.len(), 64);

    let q = client
        .query("COMMIT { status: \"ok\" } MESSAGE \"from client\";")
        .await
        .expect("query");
    assert!(!q.results.is_empty());
    let hash = q.results[0]
        .get("hash")
        .and_then(|v| v.as_str())
        .expect("hash")
        .to_string();

    let obj = client.get(&hash).await.expect("get");
    assert_eq!(obj.hash, hash);
    assert_eq!(obj.type_name, "commit");
    assert!(!obj.payload.is_empty());

    let err = client.query("").await.expect_err("empty sql");
    assert!(matches!(err, Error::Server(_)));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn pool_reuses_connections() {
    let dir = temp_dir("pool");
    let (url, _server) = start_server(&dir).await;

    let pool = Pool::builder(&url).max_size(2).build().expect("pool");
    {
        let mut a = pool.get().await.expect("get a");
        a.hello().await.expect("hello a");
    }
    {
        let mut b = pool.get().await.expect("get b");
        let refs = b.refs().await.expect("refs");
        assert_eq!(refs[0].name, "main");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn pool_max_size_waits() {
    let dir = temp_dir("pool_cap");
    let (url, _server) = start_server(&dir).await;
    let pool = Pool::builder(&url).max_size(1).build().expect("pool");

    let held = pool.get().await.expect("held");
    let pool2 = pool.clone();
    let join = tokio::spawn(async move {
        let mut c = pool2.get().await.expect("second");
        c.hello().await.expect("hello");
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!join.is_finished());
    drop(held);
    join.await.expect("join");

    let _ = std::fs::remove_dir_all(&dir);
}
