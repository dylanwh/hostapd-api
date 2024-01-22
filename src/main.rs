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
use chrono::{Duration, Utc};
use db::{Database, DB};
use linemux::{Line, MuxedLines};
use parser::Event;
use serde_json::{json, Value};
use std::{io::IsTerminal, sync::Arc};
use tokio::{net::TcpListener, signal, sync::Mutex, time::interval};
use tokio_util::sync::CancellationToken;
use tower_http::trace::TraceLayer;

use crate::db::StationQuery;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("error parsing message: {0}")]
    Parse(String),

    #[error("no files were added to the file reader")]
    NoFilesAdded,
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
                            Ok(Some(event)) => {
                                db.lock().await.witness(event);
                            }
                            Ok(None) => {
                                continue;
                            }
                            Err(e) => {
                                shutdown.cancel();
                                tracing::error!("error parsing log: {}", e);
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
        .route("/stations", get(route_station_index))
        .route("/ap", get(route_ap_index))
        .route("/ap/:ap", get(route_ap_get))
        .route("/ap/:ap/:interface", get(route_ap_interface_get))
        .route("/interface/:interface", get(route_interface_get))
        .route("/online", get(route_online))
        .route("/offline", get(route_offline))
        .route("/map", get(route_map))
        .route("/map/stations", get(route_map_stations))
        .with_state(db.clone())
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

    if let Some(ref watchdog_url) = args.watchdog_url {
        let db = db.clone();
        let shutdown = shutdown.clone();
        let watchdog_url = watchdog_url.clone();
        tracker.spawn(async move {
            watchdog_loop(&watchdog_url, db, shutdown).await;
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

fn process(next_line: Result<Option<Line>, std::io::Error>) -> Result<Option<Event>, Error> {
    match next_line {
        Ok(Some(line)) => match parser::parse(line.line()) {
            Ok(Some(event)) => Ok(Some(event)),
            Ok(None) => Ok(None),
            Err(e) => {
                tracing::debug!("error parsing message: {}", e);
                Ok(None)
            }
        },
        Ok(None) => Err(Error::NoFilesAdded),
        Err(e) => Err(e.into()),
    }
}

async fn route_index(State(db): State<DB>) -> Json<Value> {
    let db = db.lock().await;

    Json(json!({
        "devices": &db.device_list(db::DeviceQuery::All),
    }))
}

async fn route_station_index(State(db): State<DB>) -> Json<Value> {
    let db = db.lock().await;

    Json(json!({
        "stations": db.stations(),
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

async fn route_map_stations(State(db): State<DB>) -> Json<Value> {
    let db = db.lock().await;

    Json(json!({
        "station_map": db.station_map(),
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
    let devices = db.device_list(db::DeviceQuery::Station(StationQuery::Hostname(ap)));

    Json(json!({
        "devices": devices,
    }))
}

async fn route_ap_interface_get(
    State(db): State<DB>,
    Path((ap, interface)): Path<(String, String)>,
) -> Json<Value> {
    let db = db.lock().await;
    let devices = db.device_list(db::DeviceQuery::Station(StationQuery::HostnameInterface(
        ap, interface,
    )));

    Json(json!({
        "devices": devices
    }))
}

async fn route_interface_get(State(db): State<DB>, Path(interface): Path<String>) -> Json<Value> {
    let db = db.lock().await;
    let devices = db.device_list(db::DeviceQuery::Station(StationQuery::Interface(interface)));

    Json(json!({
        "devices": devices
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

async fn watchdog_loop(watchdog_url: &str, db: DB, shutdown: CancellationToken) {
    let client = reqwest::Client::new();
    let mut watchdog = interval(std::time::Duration::from_secs(60));
    let mut watchdog_fired = false;
    let watchdog_started = Utc::now();
    let watchdog_period = Duration::minutes(30);
    loop {
        tokio::select! {
            () = shutdown.cancelled() => {
                break;
            }
            _ = watchdog.tick() => {
                let now = Utc::now();
                match db.lock().await.last_event_timestamp {
                    Some(t) if now - t < watchdog_period && !watchdog_fired => {
                        tracing::warn!("watchdog: no events in 30 minutes, sending notification");
                        watchdog_alert(&client, watchdog_url, now - t).await;
                        watchdog_fired = true;
                    }
                    None if now - watchdog_started > watchdog_period && !watchdog_fired => {
                        tracing::warn!("watchdog: never seen any events in 30 minutes, sending notification");
                        watchdog_alert(&client, watchdog_url, now - watchdog_started).await;
                        watchdog_fired = true;
                    }
                    _ => { }
                }



            }
        }
    }
}

#[derive(Debug, serde::Serialize)]
struct WatchdogBody {
    text: String,
}

async fn watchdog_alert(client: &reqwest::Client, url: &str, period: Duration) {
    let resp = client
        .post(url)
        .json(&WatchdogBody {
            text: format!("No hostapd events in {} minutes", period.num_minutes()),
        })
        .send()
        .await;

    if let Err(e) = resp {
        tracing::error!("error sending watchdog alert: {}", e);
    }
}
