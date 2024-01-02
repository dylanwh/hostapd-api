# hostapd-api

## Overview
`hostapd-api` is a read-only API that interfaces with `hostapd` to monitor associated devices on one or more access points.

## Setup

### Syslog Configuration
For `hostapd-api` to function, access points need to send logs to a syslog server. This
syslog server should be configured to write logs as JSON lines to a file. A sample
configuration for `syslog-ng` compatible with the [linuxserver.io syslog-ng Docker
image](https://docs.linuxserver.io/images/docker-syslog-ng/) is provided in the
`example` directory.

### Docker Integration
Included in this repository is a `docker-compose` file to set up a `syslog-ng` container alongside a `hostapd-api` container. The root directory contains a `Dockerfile` that will work with this configuration.

## Building

### Docker Build
The primary method to build `hostapd-api` is using Docker:

```bash
docker build -t hostapd-api .
```

### Cargo Build
To manually build the project outside of Docker, use `cargo`:
```
cargo build --release
```
*Note: Rust must be installed for this method.*

## Running
### Docker Usage
When running in Docker, mount the directory containing `hostapd` log files to `/var/log/messages`. Use `-f` (`--file`) to change the log file path and `-l` (`--listen`) to alter the server's listening address and port. The default is `0.0.0.0:5580`.

## API
The API features several endpoints:
- `/`: Returns a list of all devices.
- `/mac/:mac`: Returns information for a specific device by MAC address.
- `/ap/:ap`: Returns devices associated with a specific access point.
- `/online`: Returns a list of online devices.
- `/offline`: Returns a list of offline devices.

### Example Responses

#### GET /

The root endpoint returns a list of all devices, including their MAC address, a list of access points they are associated with, and the last time they were observed, associated, and disassociated. The `online` field indicates whether the device is currently associated with an access point.


```json
{
  "devices": [
    {
      "hardware_ethernet": "00:00:00:00:00:01",
      "access_points": [],
      "last_associated": null,
      "last_disassociated": null,
      "last_observed": null,
      "online": false
    },
    {
      "hardware_ethernet": "00:00:00:00:00:02",
      "access_points": ["bedroom-ap"],
      "last_associated": "2024-01-02T12:34:56Z",
      "last_disassociated": null,
      "last_observed": "2024-01-02T12:35:56Z",
      "online": true
    }
  ]

```

#### GET /ap/:ap

The `/ap/:ap` endpoint returns a list of devices associated with a specific access point. The response is the same as the root endpoint, but only includes devices associated with the specified access point.

`GET /ap/bedroom-ap`
```json
{
  "devices": [
    {
      "hardware_ethernet": "00:00:00:00:00:02",
      "access_points": ["bedroom-ap"],
      "last_associated": "2024-01-02T12:34:56Z",
      "last_disassociated": null,
      "last_observed": "2024-01-02T12:35:56Z",
      "online": true
    }
  ]
}
```

#### GET /online

The `/online` endpoint returns a list of devices that are currently associated with an access point. The response is the same as the root endpoint, but only includes devices that are currently online.

There is also an `/offline` endpoint that returns a list of devices that are not currently associated with an access point.

#### GET /mac/:mac

The `/mac/:mac` endpoint returns information for a specific device by MAC address. The response is similar to the root endpoint, but the top level field is `device` instead of `devices`.

```json
{
  "device": {
    "hardware_ethernet": "00:00:00:00:00:02",
    "access_points": ["bedroom-ap"],
    "last_associated": "2024-01-02T12:34:56Z",
    "last_disassociated": null,
    "last_observed": "2024-01-02T12:35:56Z",
    "online": true
  }
}
```

### Integration with dhcpd-api
For enhanced functionality, `hostapd-api` can be combined with [dhcpd-api](https://github.com/dylanwh/dhcpd-api), providing a full view of connected devices, their IP addresses, and hostnames.

## Author
Dylan Hardison <dylan@hardison.net> 

## License
This project is licensed under the MIT License. See the LICENSE file for more details.