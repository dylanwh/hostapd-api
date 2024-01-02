#![warn(
    clippy::all,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::expect_used
)]

mod args;
mod db;
mod parser;

use args::Args;
use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use db::{Database, DB};
use linemux::MuxedLines;
use serde_json::{json, Value};
use std::{io::IsTerminal, sync::Arc};
use tokio::{net::TcpListener, signal, sync::Mutex};
use tower_http::trace::TraceLayer;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = Args::new();
    let db = Arc::new(Mutex::new(Database::new()));

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stdout);

    if args.json_logs {
        subscriber.json().init();
    } else if std::io::stdout().is_terminal() {
        subscriber.with_ansi(true).pretty().init();
    } else {
        subscriber.with_ansi(false).init();
    }

    let mut lines = MuxedLines::new()?;
    lines.add_file_from_start(args.file).await?;

    {
        let db = db.clone();
        tokio::spawn(async move {
            // Jan  1 09:42:46 den-ap hostapd: wl1.1: STA 32:42:fd:88:86:0c IEEE 802.11: associated
            // capture den-ap and 32:42:fd:88:86:0c
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(event) = parser::parse(line.line()) else {
                    tracing::error!("error parsing line: {}", line.line());
                    continue;
                };
                tracing::trace!("line: {}", line.line());
                db.lock().await.witness(event);
            }
        });
    }

    let router = Router::new()
        .route("/", get(route_index))
        .route("/mac/:mac", get(route_mac_get))
        .route("/ap", get(route_ap_index))
        .route("/ap/:ap", get(route_ap_get))
        .route("/online", get(route_online))
        .route("/offline", get(route_offline))
        .route("/map", get(route_map))
        .with_state(db)
        .layer(TraceLayer::new_for_http());
    let listener = TcpListener::bind(&args.listen).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn route_index(State(db): State<DB>) -> Json<Value> {
    let db = db.lock().await;

    Json(json!({
        "devices": &db.device_list(db::DeviceQuery::All),
    }))
}

async fn route_ap_index(State(db): State<DB>) -> Json<Value> {
    let db = db.lock().await;

    Json(json!({
        "access_points": db.access_points(),
    }))
}

async fn route_map(State(db): State<DB>) -> Json<Value> {
    let db = db.lock().await;

    Json(json!({
        "device_map": db.device_map(),
    }))
}

async fn route_mac_get(State(db): State<DB>, Path(mac): Path<String>) -> Json<Value> {
    let db = db.lock().await;

    Json(json!({
        "device": db.get(&mac),
    }))
}

async fn route_ap_get(State(db): State<DB>, Path(ap): Path<String>) -> Json<Value> {
    let db = db.lock().await;

    Json(json!({
        "devices": db.device_list(db::DeviceQuery::AccessPoint(ap)),
    }))
}

async fn route_online(State(db): State<DB>) -> Json<Value> {
    let db = db.lock().await;

    Json(json!({
        "devices": db.device_list(db::DeviceQuery::Online),
    }))
}

async fn route_offline(State(db): State<DB>) -> Json<Value> {
    let db = db.lock().await;

    Json(json!({
        "devices": db.device_list(db::DeviceQuery::Offline),
    }))
}

async fn shutdown_signal() {
    let ctrl_c = async {
        #[allow(clippy::expect_used)]
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        #[allow(clippy::expect_used)]
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
