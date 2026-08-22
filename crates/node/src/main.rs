//! Envshare discovery and relay node entry point.

#![forbid(unsafe_code)]

use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "envshare-node", version, about)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
}
