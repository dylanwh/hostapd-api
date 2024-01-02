use crate::parser::{Action, Event};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use tokio::sync::Mutex;

pub type DB = Arc<Mutex<Database>>;
#[derive(Debug, Clone, Default, Serialize)]
pub struct Database {
    devices: BTreeMap<String, Device>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Device {
    pub access_points: BTreeSet<String>,

    pub last_associated: Option<DateTime<Utc>>,
    pub last_disassociated: Option<DateTime<Utc>>,
    pub last_observed: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceListItem {
    #[serde(rename = "hardware_ethernet")]
    mac: String,

    #[serde(flatten)]
    device: Device,

    online: bool,
}

pub enum DeviceQuery {
    All,
    Online,
    Offline,
    AccessPoint(String),
}

impl Device {
    pub fn associate(&mut self, timestamp: DateTime<Utc>, ap: String) {
        tracing::info!("associate {timestamp} {ap}");
        self.last_associated.replace(timestamp);
        self.access_points.insert(ap);
    }

    pub fn observe(&mut self, timestamp: DateTime<Utc>, ap: String) {
        tracing::info!("observe {timestamp} {ap}");
        self.last_observed.replace(timestamp);
        self.access_points.insert(ap);
    }

    pub fn disassociate(&mut self, timestamp: DateTime<Utc>, ap: String) {
        tracing::info!("disassociate {timestamp} {ap}");
        self.last_disassociated.replace(timestamp);
        self.access_points.remove(&ap);
    }

    pub fn list_item<M>(&self, mac: M) -> DeviceListItem
    where
        M: AsRef<str>,
    {
        DeviceListItem {
            mac: mac.as_ref().to_string(),
            device: self.clone(),
            online: !self.access_points.is_empty(),
        }
    }
}

impl Database {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get<S>(&self, mac: S) -> Option<DeviceListItem>
    where
        S: AsRef<str>,
    {
        if let Some(device) = self.devices.get(mac.as_ref()) {
            return Some(device.list_item(mac));
        }
        None
    }

    pub fn access_points(&self) -> BTreeSet<String> {
        self.devices
            .iter()
            .flat_map(|(_, device)| device.access_points.iter())
            .cloned()
            .collect()
    }

    pub fn list(&self, query: DeviceQuery) -> Vec<DeviceListItem> {
        match query {
            DeviceQuery::All => self
                .devices
                .iter()
                .map(|(mac, device)| device.list_item(mac))
                .collect(),
            DeviceQuery::AccessPoint(ap) => self
                .devices
                .iter()
                .filter_map(|(mac, device)| {
                    if device.access_points.contains(&ap) {
                        Some(device.list_item(mac))
                    } else {
                        None
                    }
                })
                .collect(),

            DeviceQuery::Online => self
                .devices
                .iter()
                .filter_map(|(mac, device)| {
                    if !device.access_points.is_empty() {
                        Some(device.list_item(mac))
                    } else {
                        None
                    }
                })
                .collect(),

            DeviceQuery::Offline => self
                .devices
                .iter()
                .filter_map(|(mac, device)| {
                    if device.access_points.is_empty() {
                        Some(device.list_item(mac))
                    } else {
                        None
                    }
                })
                .collect(),
        }
    }

    pub fn witness(&mut self, event: Event) {
        let timestamp = event.timestamp;
        let ap = event.access_point;
        match event.action {
            Action::Associated { mac } => {
                self.devices
                    .entry(mac.clone())
                    .or_default()
                    .associate(timestamp, ap);
            }
            Action::Observed { mac } => {
                self.devices
                    .entry(mac.clone())
                    .or_default()
                    .observe(timestamp, ap);
            }
            Action::Disassociated { mac } => {
                self.devices
                    .entry(mac.clone())
                    .or_default()
                    .disassociate(timestamp, ap);
            }
            Action::Junk(msg) => {
                tracing::error!("{timestamp} junk {ap} {msg}");
            }
            Action::Ignored => {}
        }
    }
}
