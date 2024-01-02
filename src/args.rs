use std::{net::SocketAddr, path::PathBuf};

use clap::Parser;

#[derive(Debug, Parser)]
#[command(author, version, about)]
pub struct Args {
    #[arg(short, long, default_value = "/var/log/messages")]
    pub file: PathBuf,

    #[arg(short, long, default_value = "0.0.0.0:5580")]
    pub listen: SocketAddr,

    #[arg(long, default_value = "false")]
    pub json_logs: bool,
}

impl Args {
    pub fn new() -> Self {
        Self::parse()
    }
}
