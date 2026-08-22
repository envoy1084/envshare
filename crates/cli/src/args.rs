//! Typed initial command-line model.

use std::{ffi::OsString, path::PathBuf, time::Duration};

use clap::{Args, Parser, Subcommand, ValueEnum};
use network::DiscoveryNode;

/// Envshare command-line arguments.
#[derive(Debug, Parser)]
#[command(name = "envshare", version, about)]
pub struct Cli {
    /// Override the platform client configuration path.
    #[arg(long, global = true)]
    pub(crate) config: Option<PathBuf>,
    /// Disable terminal styling (also implied by `NO_COLOR`).
    #[arg(long, global = true)]
    pub(crate) no_color: bool,
    /// Set a secret-safe tracing filter.
    #[arg(long, global = true)]
    pub(crate) log: Option<String>,
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
    /// Test local safety and disposable public connectivity.
    Doctor(DoctorArgs),
    /// Manage named discovery-network profiles.
    Network(NetworkArgs),
    /// Generate shell completions or a manual page.
    Completions(CompletionArgs),
}

#[derive(Debug, Args)]
pub(crate) struct SendArgs {
    /// Input file, or `-` for stdin.
    pub input: PathBuf,
    /// Sender-owned share lifetime.
    #[arg(long, value_parser = humantime::parse_duration)]
    pub expires: Option<Duration>,
    /// Normalize and include only these dotenv keys.
    #[arg(long, value_delimiter = ',')]
    pub keys: Vec<String>,
    /// Permit requested keys that are absent.
    #[arg(long, requires = "keys")]
    pub allow_missing_keys: bool,
    /// Public network derivation scope.
    #[arg(long)]
    pub network: Option<String>,
    /// Explicit local multiaddress to advertise for direct transfer.
    #[arg(long, default_value = "/ip4/127.0.0.1/tcp/0")]
    pub listen: String,
    #[command(flatten)]
    pub discovery: SenderDiscoveryArgs,
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
    #[command(flatten)]
    pub environment: RunEnvironmentArgs,
    /// Emit non-secret machine-readable lifecycle events.
    #[arg(long)]
    pub json: bool,
    /// Program and arguments; no shell is invoked.
    #[arg(required = true, trailing_var_arg = true)]
    pub command: Vec<OsString>,
}

#[derive(Debug, Args)]
pub(crate) struct RunEnvironmentArgs {
    /// Received values replace matching inherited variables.
    #[arg(long, conflicts_with = "clean_env")]
    pub r#override: bool,
    /// Clear inherited variables before adding received values.
    #[arg(long)]
    pub clean_env: bool,
    /// Fail if a received key already exists in the inherited environment.
    #[arg(long, conflicts_with_all = ["override", "clean_env"])]
    pub strict: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ConnectionArgs {
    /// Capability code. Prefer the hidden prompt or `--code-stdin` interactively.
    #[arg(long, conflicts_with = "code_stdin")]
    pub code: Option<String>,
    /// Read the capability code from stdin.
    #[arg(long)]
    pub code_stdin: bool,
    /// Sender Peer ID for explicit direct mode.
    #[arg(long, requires = "address")]
    pub peer: Option<String>,
    /// Sender multiaddress for explicit direct mode.
    #[arg(long, requires = "peer")]
    pub address: Option<String>,
    #[command(flatten)]
    pub discovery: ReceiverDiscoveryArgs,
    /// Public network derivation scope.
    #[arg(long)]
    pub network: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct SenderDiscoveryArgs {
    /// Federated Rendezvous endpoint including its trailing `/p2p/<peer-id>`.
    #[arg(long = "discovery-node", value_name = "MULTIADDR")]
    pub nodes: Vec<DiscoveryNode>,
    /// Enable multicast DNS discovery on the local network.
    #[arg(long)]
    pub mdns: bool,
    /// Advertise and accept only Circuit Relay routes.
    #[arg(long)]
    pub relay_only: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ReceiverDiscoveryArgs {
    /// Federated Rendezvous endpoint including its trailing `/p2p/<peer-id>`.
    #[arg(long = "discovery-node", value_name = "MULTIADDR")]
    pub nodes: Vec<DiscoveryNode>,
    /// Enable multicast DNS candidate discovery on the local network.
    #[arg(long)]
    pub mdns: bool,
    /// Admit private and link-local candidate routes.
    #[arg(long)]
    pub lan: bool,
    /// Dial only Circuit Relay routes and disable mDNS.
    #[arg(long)]
    pub relay_only: bool,
}

#[derive(Debug, Args)]
pub(crate) struct DoctorArgs {
    /// Network profile to test.
    #[arg(long)]
    pub network: Option<String>,
    /// Emit newline-delimited machine-readable results.
    #[arg(long)]
    pub json: bool,
    /// Include per-node public identity results.
    #[arg(long)]
    pub verbose: bool,
}

#[derive(Debug, Args)]
pub(crate) struct NetworkArgs {
    #[command(subcommand)]
    pub command: NetworkCommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum NetworkCommand {
    /// List configured profile names.
    List,
    /// Show one non-secret profile.
    Show { name: String },
    /// Add or replace a profile from a TOML file.
    Add {
        name: String,
        #[arg(long)]
        file: PathBuf,
    },
    /// Remove a profile that is not the active default.
    Remove { name: String },
    /// Select the default profile.
    Use { name: String },
}

#[derive(Clone, Copy, Debug, Args)]
pub(crate) struct CompletionArgs {
    /// Output format written to stdout.
    #[arg(value_enum)]
    pub target: CompletionTarget,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum CompletionTarget {
    Bash,
    Zsh,
    Fish,
    PowerShell,
    Elvish,
    Man,
}
