use crate::parser::{Action, Event};
use chrono::{DateTime, Utc};
use serde::{Serialize, ser::SerializeMap};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use tokio::sync::Mutex;

pub type DB = Arc<Mutex<Database>>;

#[derive(Debug, Default, Serialize)]
pub struct Database {
    devices: BTreeMap<String, Device>,
}

#[derive(Debug, Default, Serialize)]
struct Device {
    access_points: BTreeSet<String>,

    last_associated: Option<DateTime<Utc>>,
    last_disassociated: Option<DateTime<Utc>>,
    last_observed: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct DeviceListItem<'a> {
    #[serde(rename = "hardware_ethernet")]
    mac: &'a str,

    #[serde(flatten)]
    device: &'a Device,

    online: bool,
}

#[derive(Debug)]
struct DeviceWithoutAccessPoints<'a>(&'a Device);

impl<'a> Serialize for DeviceWithoutAccessPoints<'a> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serde::ser::Serializer::serialize_map(serializer, Some(3))?;
        if let Some(last_associated) = self.0.last_associated {
            map.serialize_entry("last_associated", &last_associated)?;
        }
        if let Some(last_disassociated) = self.0.last_disassociated {
            map.serialize_entry("last_disassociated", &last_disassociated)?;
        }
        if let Some(last_observed) = self.0.last_observed {
            map.serialize_entry("last_observed", &last_observed)?;
        }
        map.end()
    }
}

#[derive(Debug, Serialize)]
pub struct DeviceMapItem<'a> {
    #[serde(rename = "hardware_ethernet")]
    mac: &'a str,

    #[serde(flatten)]
    device: DeviceWithoutAccessPoints<'a>,
}

pub enum DeviceQuery {
    All,
    Online,
    Offline,
    AccessPoint(String),
}

impl Device {
    fn associate(&mut self, timestamp: DateTime<Utc>, ap: String) {
        tracing::info!("associate {timestamp} {ap}");
        self.last_associated.replace(timestamp);
        self.access_points.insert(ap);
    }

    fn observe(&mut self, timestamp: DateTime<Utc>, ap: String) {
        tracing::info!("observe {timestamp} {ap}");
        self.last_observed.replace(timestamp);
        self.access_points.insert(ap);
    }

    fn disassociate(&mut self, timestamp: DateTime<Utc>, ap: &str) {
        tracing::info!("disassociate {timestamp} {ap}");
        self.last_disassociated.replace(timestamp);
        self.access_points.remove(ap);
    }

    fn list_item<'a>(&'a self, mac: &'a str) -> DeviceListItem<'a> {
        DeviceListItem {
            mac,
            device: self,
            online: !self.access_points.is_empty(),
        }
    }

    fn map_item<'a>(&'a self, mac: &'a str) -> DeviceMapItem<'a> {
        DeviceMapItem {
            mac,
            device: DeviceWithoutAccessPoints(self),
        }
    }
}

impl<'b, 'a: 'b> Database {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&'a self, mac: &'a str) -> Option<DeviceListItem<'b>> {
        if let Some(device) = self.devices.get(mac) {
            return Some(device.list_item(mac));
        }
        None
    }

    pub fn access_points(&self) -> BTreeSet<&str> {
        self.devices
            .iter()
            .flat_map(|(_, device)| device.access_points.iter())
            .map(std::string::String::as_str)
            .collect()
    }

    pub fn device_map(&'a self) -> BTreeMap<&'a str, Vec<DeviceMapItem<'a>>> {
        let mut map = BTreeMap::new();
        for (mac, device) in &self.devices {
            for ap in &device.access_points {
                map.entry(ap.as_str())
                    .or_insert_with(Vec::new)
                    .push(device.map_item(mac));
            }
        }
        map
    }

    pub fn device_list(&self, query: DeviceQuery) -> Vec<DeviceListItem> {
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
                    if device.access_points.is_empty() {
                        None
                    } else {
                        Some(device.list_item(mac))
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
                    .entry(mac)
                    .or_default()
                    .associate(timestamp, ap);
            }
            Action::Observed { mac } => {
                self.devices
                    .entry(mac)
                    .or_default()
                    .observe(timestamp, ap);
            }
            Action::Disassociated { mac } => {
                self.devices
                    .entry(mac)
                    .or_default()
                    .disassociate(timestamp, &ap);
            }
            Action::Junk(msg) => {
                tracing::error!("{timestamp} junk {ap} {msg}");
            }
            Action::Ignored => {}
        }
    }
}
