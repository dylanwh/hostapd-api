# hostapd-api

## Overview
`hostapd-api` is a read-only API that interfaces with `hostapd` to monitor associated devices on one or more access points. It's designed to provide insights into device connections and activities on your network.

## Setup
### Syslog Configuration
For `hostapd-api` to function, access points need to log to a syslog server. This syslog server should be configured to write logs as JSON lines to a file. A sample configuration for `syslog-ng` compatible with the [linuxserver.io syslog-ng Docker image](https://docs.linuxserver.io/images/docker-syslog-ng/) is provided in the `example` directory.

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
The current API is straightforward, featuring a single endpoint `/`. It returns a JSON object with `devices` fields:
- `devices`: Maps MAC addresses to the hostnames/IPs of associated access points.

### Example Response
```json
{
  "devices": {
    "00:00:00:00:00:01": [],
    "00:00:00:00:00:02": ["bedroom-ap"],
    "00:00:00:00:00:03": ["living-room-ap", "bedroom-ap"]
  }
}
```

### Integration with dhcpd-api
For enhanced functionality, `hostapd-api` can be combined with [dhcpd-api](https://github.com/dylanwh/dhcpd-api), providing a full view of connected devices, their IP addresses, and hostnames. My intention is to combine these two in a simple interface.

## Author
Dylan Hardison <dylan@hardison.net> 

## License

This project is licensed under the MIT License. See the LICENSE file for more details.
