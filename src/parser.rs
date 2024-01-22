use crate::Error;
use chrono::{DateTime, Utc};
use nom::character::complete::{char, one_of, space1};
use nom::multi::count;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until},
    combinator::map,
    sequence::terminated,
};
use nom::{Finish, IResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub timestamp: DateTime<Utc>,
    pub hostname: String,
    pub interface: String,
    pub mac: String,
    #[serde(flatten)]
    pub action: Action,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "action")]
pub enum Action {
    #[serde(rename = "associated")]
    Associated,

    #[serde(rename = "disassociated")]
    Disassociated,

    #[serde(rename = "observed")]
    Observed,
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

pub fn parse(input: &str) -> Result<Option<Event>, Error> {
    let log: Log = serde_json::from_str(input)?;

    // for now, only parse hostapd logs
    // I hope we don't need to parse other program's logs
    if log.program != "hostapd" {
        return Ok(None);
    }

    match parse_message(&log.message).finish() {
        Ok((_, (interface, mac, Some(action)))) => Ok(Some(Event {
            timestamp: log.timestamp,
            hostname: log.host,
            interface,
            mac,
            action,
        })),
        Ok((_, (_, _, None))) => Ok(None),
        Err(e) => Err(Error::Parse(e.to_string())),
    }
}

// wl1.1: STA 32:42:fd:88:86:0c IEEE 802.11: associated
// wl1.1: STA 32:42:fd:88:86:0c IEEE 802.11: disassociated
// wl1.1: STA 32:42:fd:88:86:0c WPA: pairwise key handshake completed (RSN)
// eth10: STA 04:17:b6:37:96:dc WPA: group key handshake completed (RSN)
// eth10: STA 04:17:b6:37:96:dc RADIUS: starting accounting session 5F3F4F6F-00000000

fn parse_message(input: &str) -> IResult<&str, (String, String, Option<Action>)> {
    let (input, interface) = terminated(take_until(": "), tag(": "))(input)?;
    let (input, _) = tag("STA ")(input)?;
    let (input, mac) = terminated(val_macaddr, space1)(input)?;
    let (input, action) = alt((
        map(tag("IEEE 802.11: associated"), |_| Some(Action::Associated)),
        map(tag("IEEE 802.11: disassociated"), |_| {
            Some(Action::Disassociated)
        }),
        map(tag("WPA: pairwise key handshake completed (RSN)"), |_| {
            Some(Action::Observed)
        }),
        map(tag("WPA: group key handshake completed (RSN)"), |_| {
            Some(Action::Observed)
        }),
        map(tag("RADIUS: starting accounting session"), |_| None),
    ))(input)?;

    Ok((input, (interface.to_string(), mac, action)))
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

    let mac = format!("{x1:02x}:{x2:02x}:{x3:02x}:{x4:02x}:{x5:02x}:{x6:02x}");

    Ok((input, mac))
}
