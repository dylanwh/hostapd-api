use chrono::{DateTime, Utc};
use nom::IResult;
use nom::character::complete::{one_of, char, space1};
use nom::multi::count;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until},
    combinator::map,
    sequence::terminated,
};
use serde::Deserialize;

use crate::Error;

#[derive(Debug, PartialEq)]
pub struct Event {
    pub timestamp: DateTime<Utc>,
    pub host: String,
    pub action: Action,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    Associated { mac: String },

    Disassociated { mac: String },

    Observed { mac: String },

    Junk(String),

    Ignored
}

/// This matches the syslog-ng format
/// template("$(format-json host=$HOST program=$PROGRAM timestamp=$ISODATE message=$MESSAGE)");
#[derive(Debug, PartialEq, Deserialize)]
struct Log {
    host: String,
    program: String,
    timestamp: DateTime<Utc>,
    message: String,
}

pub fn parse(input: &str) -> Result<Event, Error> {
    let log: Log = serde_json::from_str(input)?;
    if log.program != "hostapd" {
        return Ok(Event {
            host: log.host,
            timestamp: log.timestamp,
            action: Action::Ignored,
        });
    }

    let action = match parse_action(&log.message) {
        Ok((_, action)) => action,
        Err(_) => Action::Junk(log.message),
    };

    Ok(Event {
        host: log.host,
        timestamp: log.timestamp,
        action,
    })
}


// wl1.1: STA 32:42:fd:88:86:0c IEEE 802.11: associated
// wl1.1: STA 32:42:fd:88:86:0c IEEE 802.11: disassociated
// wl1.1: STA 32:42:fd:88:86:0c WPA: pairwise key handshake completed (RSN)
// eth10: STA 04:17:b6:37:96:dc WPA: group key handshake completed (RSN)

fn parse_action(input: &str) -> IResult<&str, Action> {
    let (input, _nic) = terminated(take_until(": "), tag(": "))(input)?;
    let (input, _) = tag("STA ")(input)?;
    let (input, mac) = terminated(val_macaddr, space1)(input)?;
    let (input, action) = alt((
        map(tag("IEEE 802.11: associated"), |_| {
            Action::Associated { mac: mac.clone() }
        }),
        map(tag("IEEE 802.11: disassociated"), |_| {
            Action::Disassociated { mac: mac.clone() }
        }),
        map(tag("WPA: pairwise key handshake completed (RSN)"), |_| {
            Action::Observed { mac: mac.clone() }
        }),
        map(tag("WPA: group key handshake completed (RSN)"), |_| {
            Action::Observed { mac: mac.clone() }
        }),
        map(tag("RADIUS: starting accounting session"), |_| {
            Action::Ignored
        }),
    ))(input)?;

    Ok((input, action))

}

const HEX: &str = "0123456789abcdefABCDEF";

fn val_hexbyte(input: &str) -> IResult<&str, u8> {
    let (input, byte) = count(one_of(HEX), 2)(input)?;
    let byte = byte.iter().collect::<String>();

    u8::from_str_radix(&byte, 16).map_or_else(
        |_| {
            Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Digit,
            )))
        },
        |byte| Ok((input, byte)),
    )
}

fn val_macaddr(input: &str) -> IResult<&str, String> {
    let (input, x1) = val_hexbyte(input)?;
    let (input, _) = char(':')(input)?;
    let (input, x2) = val_hexbyte(input)?;
    let (input, _) = char(':')(input)?;
    let (input, x3) = val_hexbyte(input)?;
    let (input, _) = char(':')(input)?;
    let (input, x4) = val_hexbyte(input)?;
    let (input, _) = char(':')(input)?;
    let (input, x5) = val_hexbyte(input)?;
    let (input, _) = char(':')(input)?;
    let (input, x6) = val_hexbyte(input)?;

    let mac = format!(
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        x1, x2, x3, x4, x5, x6
    );

    Ok((input, mac))
}

