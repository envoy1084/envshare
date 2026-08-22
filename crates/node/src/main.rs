//! Envshare discovery and relay node entry point.

#![forbid(unsafe_code)]

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use node::{
    NodeConfig, NodeError, NodeEvent, NodeServer, OperationsServer, generate_identity,
    load_identity, save_identity,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Parser)]
#[command(name = "envshare-node", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Manage the stable Ed25519 node identity.
    Key(KeyArgs),
    /// Run the bounded relay service.
    Serve(ServeArgs),
}

#[derive(Debug, Args)]
struct KeyArgs {
    #[command(subcommand)]
    command: KeyCommand,
}

#[derive(Debug, Subcommand)]
enum KeyCommand {
    /// Generate and privately persist a new identity.
    Generate {
        /// Identity output file.
        #[arg(long)]
        output: PathBuf,
        /// Replace an existing regular identity file.
        #[arg(long)]
        force: bool,
    },
    /// Print the public Peer ID of an identity file.
    Inspect {
        /// Existing identity file.
        #[arg(long)]
        identity: PathBuf,
    },
}

#[derive(Debug, Args)]
struct ServeArgs {
    /// Stable Ed25519 identity file.
    #[arg(long)]
    identity: PathBuf,
    /// Strict bounded node TOML configuration.
    #[arg(long)]
    config: Option<PathBuf>,
    /// TCP or QUIC multiaddress to bind; repeat for multiple listeners.
    #[arg(long = "listen")]
    listen_addresses: Vec<String>,
    /// Emit newline-delimited JSON operational events.
    #[arg(long)]
    json: bool,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run(Cli::parse()).await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), NodeError> {
    match cli.command {
        Command::Key(arguments) => run_key(arguments),
        Command::Serve(arguments) => run_server(arguments).await,
    }
}

fn run_key(arguments: KeyArgs) -> Result<(), NodeError> {
    match arguments.command {
        KeyCommand::Generate { output, force } => {
            let keypair = generate_identity();
            save_identity(&output, &keypair, force)?;
            println!("{}", keypair.public().to_peer_id());
        }
        KeyCommand::Inspect { identity } => {
            println!("{}", load_identity(&identity)?.public().to_peer_id());
        }
    }
    Ok(())
}

async fn run_server(arguments: ServeArgs) -> Result<(), NodeError> {
    let mut config = arguments
        .config
        .as_deref()
        .map(NodeConfig::load)
        .transpose()?
        .unwrap_or_default();
    if !arguments.listen_addresses.is_empty() {
        config.listen_addresses = arguments
            .listen_addresses
            .iter()
            .map(|address| address.parse().map_err(|_| NodeError::Configuration))
            .collect::<Result<Vec<_>, _>>()?;
    }
    let (peer_id, mut events, server) =
        NodeServer::new(load_identity(&arguments.identity)?, &config)?;
    let status = server.status();
    let operations = match config.operations_address {
        Some(address) => Some(OperationsServer::bind(address, status).await?),
        None => None,
    };
    let operations_cancel = CancellationToken::new();
    let operations_enabled = operations.is_some();
    let operations_wait = operations_cancel.clone();
    let mut operations_task = tokio::spawn(async move {
        if let Some(server) = operations {
            server.run(operations_wait).await
        } else {
            operations_wait.cancelled().await;
            Ok(())
        }
    });
    if arguments.json {
        println!(
            "{}",
            serde_json::json!({ "event": "starting", "peer_id": peer_id.to_string() })
        );
    } else {
        println!("Node peer: {peer_id}");
    }
    let cancellation = CancellationToken::new();
    let mut server_task =
        tokio::spawn(server.run_graceful(cancellation.clone(), config.shutdown_grace_period));
    let mut operations_finished = false;
    let result = loop {
        tokio::select! {
            interrupted = tokio::signal::ctrl_c() => {
                cancellation.cancel();
                let node_result = flatten_node_task(server_task.await);
                break if interrupted.is_err() {
                    Err(NodeError::Configuration)
                } else {
                    node_result
                };
            }
            result = &mut server_task => {
                break flatten_node_task(result);
            }
            result = &mut operations_task, if operations_enabled && !operations_finished => {
                operations_finished = true;
                cancellation.cancel();
                let operations_result = flatten_operations_task(result);
                let node_result = flatten_node_task(server_task.await);
                break operations_result.and(node_result);
            }
            event = events.recv() => {
                let Some(event) = event else { continue };
                print_event(arguments.json, event);
            }
        }
    };
    operations_cancel.cancel();
    if !operations_finished {
        flatten_operations_task(operations_task.await)?;
    }
    result
}

fn flatten_node_task(
    result: Result<Result<(), NodeError>, tokio::task::JoinError>,
) -> Result<(), NodeError> {
    result.map_err(|_| NodeError::Configuration)?
}

fn flatten_operations_task(
    result: Result<Result<(), NodeError>, tokio::task::JoinError>,
) -> Result<(), NodeError> {
    result.map_err(|_| NodeError::Operations)?
}

fn print_event(json: bool, event: NodeEvent) {
    if json {
        let event_name = match event {
            NodeEvent::Listening { .. } => "listening",
            NodeEvent::ReservationAccepted { renewed: false, .. } => "reservation_accepted",
            NodeEvent::ReservationAccepted { renewed: true, .. } => "reservation_renewed",
            NodeEvent::CircuitAccepted { .. } => "circuit_accepted",
            NodeEvent::ReservationClosed { .. } => "reservation_closed",
            NodeEvent::ReservationDenied { .. } => "reservation_denied",
            NodeEvent::CircuitDenied { .. } => "circuit_denied",
            NodeEvent::DiscoveryRegistered { .. } => "discovery_registered",
            NodeEvent::DiscoveryUnregistered { .. } => "discovery_unregistered",
            NodeEvent::DiscoveryServed { .. } => "discovery_served",
            NodeEvent::DiscoveryRejected { .. } => "discovery_rejected",
        };
        println!("{}", serde_json::json!({ "event": event_name }));
    } else if let NodeEvent::Listening { address } = event {
        println!("Listening: {address}");
    }
}
