//! Envshare command-line entry point.

#![forbid(unsafe_code)]

use clap::Parser;
use cli::Cli;

#[tokio::main]
async fn main() {
    let status = match cli::run(Cli::parse()).await {
        Ok(status) => status,
        Err(error) => {
            eprintln!("{error}");
            error.exit_code().as_i32()
        }
    };
    std::process::exit(status);
}
