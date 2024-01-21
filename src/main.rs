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
use linemux::{Line, MuxedLines};
use parser::Event;
use serde_json::{json, Value};
use std::{io::IsTerminal, sync::Arc};
use tokio::{net::TcpListener, signal, sync::Mutex};
use tokio_util::sync::CancellationToken;
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
    let tracker = tokio_util::task::TaskTracker::new();
    let shutdown = CancellationToken::new();

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
        let shutdown = shutdown.clone();
        tracker.spawn(async move {
            loop {
                tokio::select! {
                    next_line = lines.next_line() => {
                        match process(next_line) {
                            Action::Witness(event) => {
                                db.lock().await.witness(event);
                            }
                            Action::Continue => {
                                continue;
                            }
                            Action::Shutdown => {
                                shutdown.cancel();
                                break;
                            }
                        }
                    }
                    () = shutdown.cancelled() => {
                        break;
                    }
                }
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
    {
        let shutdown = shutdown.clone();
        tracker.spawn(async move {
            let serve = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    shutdown.cancelled().await;
                })
                .await;
            if let Err(e) = serve {
                tracing::error!("server error: {}", e);
            }
        });
    }

    tracker.close();

    tokio::select! {
        () = shutdown_signal() => {
            tracing::info!("shutting down");
            shutdown.cancel();
        }
        () = shutdown.cancelled() => {
            tracing::info!("got shutdown event");
        }
    }

    tracker.wait().await;

    Ok(())
}

enum Action {
    Witness(Event),
    Continue,
    Shutdown,
}

fn process(next_line: Result<Option<Line>, std::io::Error>) -> Action {
    match next_line {
        Ok(Some(line)) => {
            if let Ok(event) = parser::parse(line.line()) {
                Action::Witness(event)
            } else {
                tracing::error!("error parsing line: {}", line.line());
                Action::Continue
            }
        }
        Ok(None) => {
            tracing::error!("no files were ever added, exiting");
            Action::Shutdown
        }
        Err(e) => {
            tracing::error!("error reading line: {}", e);
            Action::Shutdown
        }
    }
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
        "name": ap,
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
