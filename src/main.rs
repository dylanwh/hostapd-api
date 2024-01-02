mod args;
mod parser;

use args::Args;
use axum::{extract::State, routing::get, Json, Router};
use chrono::{DateTime, Utc};
use dashmap::{DashMap, DashSet};
use linemux::MuxedLines;
use parser::{Action, Event};
use serde_json::{json, Value};
use std::{sync::Arc, io::IsTerminal};
use tokio::{net::TcpListener, signal};
use tower_http::trace::TraceLayer;

type LocationMap = DashMap<String, DashSet<String>>;
type LastSeen = DashMap<String, DateTime<Utc>>;

#[derive(Debug, Clone)]
struct Database {
    location: Arc<LocationMap>,
    last_seen: Arc<LastSeen>,
}

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
    let location = Arc::new(DashMap::new());
    let last_seen = Arc::new(DashMap::new());
    let db = Database {
        location: location.clone(),
        last_seen: last_seen.clone(),
    };

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
                process_event(&db, event);
            }
        });
    }

    let router = Router::new()
        .route("/", get(index))
        .with_state(db)
        .layer(TraceLayer::new_for_http());
    let listener = TcpListener::bind(&args.listen).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn process_event(db: &Database, event: Event) {
    let timestamp = event.timestamp;
    let host = event.host;
    match event.action {
        Action::Associated { mac } | Action::Observed { mac } => {
            tracing::info!("{timestamp} add {host} {mac}");
            db.last_seen.insert(mac.clone(), timestamp);
            db.location.entry(mac).or_default().insert(host);
        }
        Action::Disassociated { mac } => {
            tracing::info!("{timestamp} remove {host} {mac}");
            if let Some(hosts) = db.location.get_mut(&mac) {
                hosts.remove(&host);
            }
        }
        Action::Junk(msg) => {
            tracing::error!("{timestamp} junk {host} {msg}");
        }
        Action::Ignored => {}
    }
}

async fn index(State(db): State<Database>) -> Json<Value> {
    Json(json!({
        "location": *(db.location),
        "last_seen": *(db.last_seen),
    }))
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
