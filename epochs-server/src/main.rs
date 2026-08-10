//! epochs-server — Epoch Protocol (EPX) over a single-writer DiskStore.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tokio::sync::Mutex;
use tracing::info;

use epochs_server::epx;
use epochs_server::state::{open_or_init, AppState};

#[derive(Parser, Debug)]
#[command(
    name = "epochs-server",
    about = "epochs Epoch Protocol (EPX) server for DiskStore + EpochQL"
)]
struct Cli {
    /// Repository data directory (created if missing).
    #[arg(long, env = "EPOCHS_DATA", default_value = "/data")]
    data_dir: PathBuf,

    /// EPX TCP listen address (`epochs://host:7420`).
    #[arg(long, env = "EPOCHS_BIND", default_value = "0.0.0.0:7420")]
    bind: SocketAddr,

    /// Default branch when initializing a new repo.
    #[arg(long, default_value = "main")]
    branch: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "epochs_server=info".into()),
        )
        .init();

    let cli = Cli::parse();
    let store = open_or_init(&cli.data_dir, &cli.branch)?;
    let state = AppState {
        store: Arc::new(Mutex::new(store)),
    };

    info!(
        data = %cli.data_dir.display(),
        bind = %cli.bind,
        "epochs-server ready (EPX, single-writer)"
    );

    tokio::select! {
        res = epx::serve(cli.bind, state) => {
            res?;
        }
        _ = shutdown_signal() => {
            info!("shutdown");
        }
    }
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
