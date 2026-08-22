//! Envshare discovery and relay node entry point.

#![forbid(unsafe_code)]

use std::{net::SocketAddr, path::PathBuf, time::Duration};

use clap::{Args, Parser, Subcommand};
use logging::TelemetryGuard;
use node::{
    LogFormat, NodeConfig, NodeError, NodeEvent, NodeServer, OperationsServer, generate_identity,
    load_identity, save_identity,
};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;

mod logging;

#[derive(Debug, Parser)]
#[command(name = "envshare-node", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate node configuration.
    Config(ConfigArgs),
    /// Manage the stable Ed25519 node identity.
    Key(KeyArgs),
    /// Run the bounded relay service.
    Serve(ServeArgs),
    /// Check a loopback node health endpoint.
    Healthcheck(HealthcheckArgs),
}

#[derive(Debug, Args)]
struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Validate a strict bounded configuration file without binding sockets.
    Check {
        /// Node configuration file.
        #[arg(long)]
        config: PathBuf,
    },
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

#[derive(Debug, Args)]
struct HealthcheckArgs {
    /// Loopback HTTP liveness or readiness URL.
    #[arg(long, default_value = "http://127.0.0.1:9100/health/ready")]
    url: String,
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
        Command::Config(arguments) => run_config(arguments),
        Command::Key(arguments) => run_key(arguments),
        Command::Serve(arguments) => run_server(arguments).await,
        Command::Healthcheck(arguments) => run_healthcheck(&arguments.url).await,
    }
}

fn run_config(arguments: ConfigArgs) -> Result<(), NodeError> {
    match arguments.command {
        ConfigCommand::Check { config } => {
            let _ = NodeConfig::load(&config)?;
            println!("configuration valid");
        }
    }
    Ok(())
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
    let json = arguments.json || config.telemetry.log_format == LogFormat::Json;
    let _telemetry = TelemetryGuard::initialize(&config.telemetry, arguments.json)?;
    run_server_config(arguments, config, json)
        .instrument(tracing::info_span!("node_service"))
        .await
}

async fn run_server_config(
    arguments: ServeArgs,
    config: NodeConfig,
    json: bool,
) -> Result<(), NodeError> {
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
    tracing::info!(event = "node_starting");
    if json {
        println!("{}", event_json("starting"));
    } else {
        println!("Node peer: {peer_id}");
    }
    let cancellation = CancellationToken::new();
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);
    let mut server_task =
        tokio::spawn(server.run_graceful(cancellation.clone(), config.shutdown_grace_period));
    let mut operations_finished = false;
    let result = loop {
        tokio::select! {
            interrupted = &mut shutdown => {
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
                print_event(json, event);
            }
        }
    };
    operations_cancel.cancel();
    if !operations_finished {
        flatten_operations_task(operations_task.await)?;
    }
    result
}

async fn shutdown_signal() -> Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result,
            _ = terminate.recv() => Ok(()),
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await
    }
}

async fn run_healthcheck(url: &str) -> Result<(), NodeError> {
    let (address, path) = parse_healthcheck_url(url)?;
    let check = async {
        let mut stream = tokio::net::TcpStream::connect(address)
            .await
            .map_err(|_| NodeError::Operations)?;
        stream
            .write_all(
                format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .map_err(|_| NodeError::Operations)?;
        let mut response = Vec::with_capacity(1_024);
        stream
            .take(4_096)
            .read_to_end(&mut response)
            .await
            .map_err(|_| NodeError::Operations)?;
        response
            .starts_with(b"HTTP/1.1 200 ")
            .then_some(())
            .ok_or(NodeError::Operations)
    };
    tokio::time::timeout(Duration::from_secs(3), check)
        .await
        .map_err(|_| NodeError::Operations)?
}

fn parse_healthcheck_url(url: &str) -> Result<(SocketAddr, &'static str), NodeError> {
    let value = url
        .strip_prefix("http://")
        .ok_or(NodeError::Configuration)?;
    let (authority, path) = value.split_once('/').ok_or(NodeError::Configuration)?;
    let address: SocketAddr = authority.parse().map_err(|_| NodeError::Configuration)?;
    if !address.ip().is_loopback() {
        return Err(NodeError::Configuration);
    }
    let path = match path {
        "health/live" => "/health/live",
        "health/ready" => "/health/ready",
        _ => return Err(NodeError::Configuration),
    };
    Ok((address, path))
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
    let event_name = event_name(&event);
    tracing::info!(event = event_name);
    if json {
        println!("{}", event_json(event_name));
    } else if let NodeEvent::Listening { address } = event {
        println!("Listening: {address}");
    }
}

fn event_name(event: &NodeEvent) -> &'static str {
    match event {
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
    }
}

fn event_json(event: &'static str) -> serde_json::Value {
    serde_json::json!({ "event": event })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_events_do_not_serialize_identifiers_or_addresses()
    -> Result<(), Box<dyn std::error::Error>> {
        let peer = libp2p::identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id();
        let address: libp2p::Multiaddr = "/dns4/private.example/tcp/4001".parse()?;
        let events = [
            NodeEvent::Listening { address },
            NodeEvent::ReservationAccepted {
                peer,
                renewed: false,
            },
            NodeEvent::CircuitAccepted {
                source: peer,
                destination: peer,
            },
            NodeEvent::DiscoveryServed {
                peer,
                result_count: 42,
            },
        ];

        for event in events {
            let encoded = event_json(event_name(&event)).to_string();
            assert_eq!(encoded.matches(':').count(), 1);
            assert!(!encoded.contains(&peer.to_string()));
            assert!(!encoded.contains("private.example"));
            assert!(!encoded.contains("42"));
        }
        assert_eq!(
            event_json("starting"),
            serde_json::json!({ "event": "starting" })
        );
        Ok(())
    }

    #[test]
    fn healthcheck_urls_are_loopback_and_path_bounded() {
        assert!(parse_healthcheck_url("http://127.0.0.1:9100/health/ready").is_ok());
        assert!(parse_healthcheck_url("http://[::1]:9100/health/live").is_ok());
        assert!(parse_healthcheck_url("https://127.0.0.1:9100/health/ready").is_err());
        assert!(parse_healthcheck_url("http://192.0.2.1:9100/health/ready").is_err());
        assert!(parse_healthcheck_url("http://127.0.0.1:9100/metrics").is_err());
    }

    #[tokio::test]
    async fn healthcheck_requires_a_success_response() -> Result<(), Box<dyn std::error::Error>> {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let address = listener.local_addr()?;
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await?;
            let mut request = [0_u8; 256];
            let _ = stream.read(&mut request).await?;
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nok\n")
                .await
        });

        run_healthcheck(&format!("http://{address}/health/ready")).await?;
        server.await??;
        Ok(())
    }
}
