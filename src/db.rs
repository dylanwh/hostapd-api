use crate::parser::{Action, Event};
use chrono::{DateTime, Utc};
use serde::{ser::SerializeMap, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use tokio::sync::Mutex;

pub type DB = Arc<Mutex<Database>>;

#[derive(Debug, Default, Serialize)]
pub struct Database {
    devices: BTreeMap<String, Device>,
    pub last_event_timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Station {
    pub hostname: String,
    pub interface: String,
}

impl std::fmt::Display for Station {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.hostname, self.interface)
    }
}

#[derive(Debug, Default, Serialize)]
struct Device {
    stations: BTreeSet<Station>,

    last_associated: Option<DateTime<Utc>>,
    last_disassociated: Option<DateTime<Utc>>,
    last_observed: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct DeviceListItem<'a> {
    #[serde(rename = "hardware_ethernet")]
    mac: &'a str,

    access_points: BTreeSet<&'a str>,

    #[serde(flatten)]
    device: &'a Device,

    online: bool,
}

#[derive(Debug)]
struct DeviceWithoutStations<'a>(&'a Device);

impl<'a> Serialize for DeviceWithoutStations<'a> {
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
    device: DeviceWithoutStations<'a>,
}

pub enum StationQuery {
    Hostname(String),
    Interface(String),
    HostnameInterface(String, String),
}

pub enum DeviceQuery {
    All,
    Online,
    Offline,
    Station(StationQuery),
}

impl Device {
    fn access_points(&self) -> BTreeSet<&str> {
        self.stations.iter().map(|s| s.hostname.as_str()).collect()
    }

    fn associate(&mut self, timestamp: DateTime<Utc>, ap: Station) {
        tracing::info!("associate {timestamp} {ap}");
        self.last_associated.replace(timestamp);
        self.stations.insert(ap);
    }

    fn observe(&mut self, timestamp: DateTime<Utc>, ap: Station) {
        tracing::info!("observe {timestamp} {ap}");
        self.last_observed.replace(timestamp);
        self.stations.insert(ap);
    }

    fn disassociate(&mut self, timestamp: DateTime<Utc>, ap: &Station) {
        tracing::info!("disassociate {timestamp} {ap}");
        self.last_disassociated.replace(timestamp);
        self.stations.remove(ap);
    }

    fn list_item<'a>(&'a self, mac: &'a str) -> DeviceListItem<'a> {
        DeviceListItem {
            mac,
            device: self,
            access_points: self.access_points(),
            online: !self.stations.is_empty(),
        }
    }

    fn map_item<'a>(&'a self, mac: &'a str) -> DeviceMapItem<'a> {
        DeviceMapItem {
            mac,
            device: DeviceWithoutStations(self),
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
            .flat_map(|(_, device)| device.stations.iter())
            .map(|s| s.hostname.as_str())
            .collect()
    }

    pub fn stations(&self) -> BTreeMap<String, BTreeSet<&str>> {
        let mut map = BTreeMap::new();
        for device in self.devices.values() {
            for sta in &device.stations {
                map.entry(sta.hostname.clone())
                    .or_insert_with(BTreeSet::new)
                    .insert(sta.interface.as_str());
            }
        }
        map
    }

    pub fn device_map(&'a self) -> BTreeMap<&'a str, Vec<DeviceMapItem<'a>>> {
        let mut map = BTreeMap::new();
        for (mac, device) in &self.devices {
            for ap in &device.stations {
                map.entry(ap.hostname.as_str())
                    .or_insert_with(Vec::new)
                    .push(device.map_item(mac));
            }
        }
        map
    }

    pub fn station_map(&'a self) -> BTreeMap<&'a str, BTreeMap<&'a str, Vec<DeviceMapItem<'a>>>> {
        let mut map = BTreeMap::new();
        for (mac, device) in &self.devices {
            for ap in &device.stations {
                map.entry(ap.hostname.as_str())
                    .or_insert_with(BTreeMap::new)
                    .entry(ap.interface.as_str())
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
            DeviceQuery::Station(StationQuery::Hostname(ap)) => self
                .devices
                .iter()
                .filter_map(|(mac, device)| {
                    if device.stations.iter().any(|s| s.hostname == ap) {
                        Some(device.list_item(mac))
                    } else {
                        None
                    }
                })
                .collect(),
            DeviceQuery::Station(StationQuery::Interface(ap)) => self
                .devices
                .iter()
                .filter_map(|(mac, device)| {
                    if device.stations.iter().any(|s| s.interface == ap) {
                        Some(device.list_item(mac))
                    } else {
                        None
                    }
                })
                .collect(),
            DeviceQuery::Station(StationQuery::HostnameInterface(ap, interface)) => self
                .devices
                .iter()
                .filter_map(|(mac, device)| {
                    if device
                        .stations
                        .iter()
                        .any(|s| s.hostname == ap && s.interface == interface)
                    {
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
                    if device.stations.is_empty() {
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
                    if device.stations.is_empty() {
                        Some(device.list_item(mac))
                    } else {
                        None
                    }
                })
                .collect(),
        }
    }

    pub fn witness(
        &mut self,
        Event {
            timestamp,
            hostname,
            interface,
            mac,
            action,
            ..
        }: Event,
    ) {
        let station = Station {
            hostname,
            interface,
        };
        self.last_event_timestamp.replace(timestamp);
        match action {
            Action::Associated => {
                self.devices
                    .entry(mac)
                    .or_default()
                    .associate(timestamp, station);
            }
            Action::Observed => {
                self.devices
                    .entry(mac)
                    .or_default()
                    .observe(timestamp, station);
            }
            Action::Disassociated => {
                self.devices
                    .entry(mac)
                    .or_default()
                    .disassociate(timestamp, &station);
            }
        }
    }
}
