//! Typed initial command-line model.

use std::{ffi::OsString, path::PathBuf, time::Duration};

use clap::{Args, Parser, Subcommand};

/// Envshare command-line arguments.
#[derive(Debug, Parser)]
#[command(name = "envshare", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Share a dotenv payload with one direct receiver.
    Send(SendArgs),
    /// Receive a dotenv payload into a private file.
    Receive(ReceiveArgs),
    /// Receive variables and execute a child without a shell.
    Run(RunArgs),
}

#[derive(Debug, Args)]
pub(crate) struct SendArgs {
    /// Input file, or `-` for stdin.
    pub input: PathBuf,
    /// Sender-owned share lifetime.
    #[arg(long, default_value = "10m", value_parser = humantime::parse_duration)]
    pub expires: Duration,
    /// Normalize and include only these dotenv keys.
    #[arg(long, value_delimiter = ',')]
    pub keys: Vec<String>,
    /// Permit requested keys that are absent.
    #[arg(long, requires = "keys")]
    pub allow_missing_keys: bool,
    /// Public network derivation scope.
    #[arg(long, default_value = "public-v1")]
    pub network: String,
    /// Explicit local multiaddress to advertise for direct transfer.
    #[arg(long, default_value = "/ip4/127.0.0.1/tcp/0")]
    pub listen: String,
    /// Print only the secret capability code on stdout.
    #[arg(long, conflicts_with = "json")]
    pub code_only: bool,
    /// Emit non-secret machine-readable lifecycle events.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ReceiveArgs {
    #[command(flatten)]
    pub connection: ConnectionArgs,
    /// Destination chosen by the receiver.
    #[arg(short, long, default_value = ".env.shared")]
    pub output: PathBuf,
    /// Atomically replace an existing regular destination.
    #[arg(long)]
    pub force: bool,
    /// Flush content and parent directory metadata before acknowledgement.
    #[arg(long)]
    pub durable: bool,
    /// Emit non-secret machine-readable events.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct RunArgs {
    #[command(flatten)]
    pub connection: ConnectionArgs,
    /// Received values replace matching inherited variables.
    #[arg(long, conflicts_with = "clean_env")]
    pub r#override: bool,
    /// Clear inherited variables before adding received values.
    #[arg(long)]
    pub clean_env: bool,
    /// Program and arguments; no shell is invoked.
    #[arg(required = true, trailing_var_arg = true)]
    pub command: Vec<OsString>,
}

#[derive(Debug, Args)]
pub(crate) struct ConnectionArgs {
    /// Capability code. Prefer the hidden prompt or `--code-stdin` interactively.
    #[arg(long, conflicts_with = "code_stdin")]
    pub code: Option<String>,
    /// Read the capability code from stdin.
    #[arg(long)]
    pub code_stdin: bool,
    /// Sender Peer ID printed by the direct sender.
    #[arg(long)]
    pub peer: String,
    /// Sender multiaddress printed by the direct sender.
    #[arg(long)]
    pub address: String,
    /// Public network derivation scope.
    #[arg(long, default_value = "public-v1")]
    pub network: String,
}
