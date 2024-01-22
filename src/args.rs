use clap::Parser;
use std::{net::SocketAddr, path::PathBuf};

#[derive(Debug, Parser)]
#[command(author, version, about)]
pub struct Args {
    /// The file to read from. It is assumed this log will be in json format,
    /// with the following keys: host, program, timestamp, and message.
    #[arg(short, long, default_value = "/var/log/messages")]
    pub file: PathBuf,

    /// The address to listen on for HTTP requests
    #[arg(short, long, default_value = "0.0.0.0:5580")]
    pub listen: SocketAddr,

    /// Enable JSON logging (off by default)
    #[arg(long, default_value = "false")]
    pub json_logs: bool,

    /// This is a URL that is presumably a Pushcut URL, accepting a POST request with a JSON body
    /// containing a `text` field.
    #[arg(env = "WATCHDOG_URL")]
    pub watchdog_url: Option<String>,
}

impl Args {
    pub fn new() -> Self {
        Self::parse()
    }
}
