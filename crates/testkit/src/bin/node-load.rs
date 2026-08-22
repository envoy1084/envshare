//! Bounded local load and soak harness for the production node.

#![forbid(unsafe_code)]

use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail};
use clap::{Args, Parser, Subcommand};
use network::{
    DiscoveryNamespace, DiscoveryProvider as _, Multiaddr, NetworkConfig, NetworkDriver,
    NetworkEvent, PeerId, identity,
};
use node::{NodeConfig, NodeEvent, NodeServer};
use serde::Serialize;
use tokio::{sync::mpsc, task::JoinSet, time::timeout};
use tokio_util::sync::CancellationToken;

const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_CLIENTS: usize = 256;
const MAX_SOAK_DURATION: Duration = Duration::from_hours(168);

#[derive(Debug, Parser)]
#[command(name = "node-load", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Saturate concurrent Circuit Relay v2 reservations.
    Relay(LoadArgs),
    /// Saturate signed discovery registrations.
    Discovery(LoadArgs),
    /// Repeat relay and discovery churn for a bounded duration.
    Soak(SoakArgs),
}

#[derive(Clone, Debug, Args)]
struct LoadArgs {
    /// Number of concurrent client swarms.
    #[arg(long, default_value_t = 64)]
    clients: usize,
    /// Node reservation and registration capacity under test.
    #[arg(long, default_value_t = 32)]
    capacity: usize,
    /// How long accepted work remains active.
    #[arg(long, default_value = "5s", value_parser = parse_duration)]
    hold: Duration,
    /// Emit one aggregate JSON object.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Debug, Args)]
struct SoakArgs {
    /// Concurrent client swarms in each workload round.
    #[arg(long, default_value_t = 32)]
    clients: usize,
    /// Node reservation and registration capacity under test.
    #[arg(long, default_value_t = 32)]
    capacity: usize,
    /// Total soak duration, capped at seven days.
    #[arg(long, default_value = "24h", value_parser = parse_duration)]
    duration: Duration,
    /// How long accepted work remains active in each round.
    #[arg(long, default_value = "5s", value_parser = parse_duration)]
    hold: Duration,
    /// Emit one aggregate JSON object.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
struct Counts {
    attempted: u64,
    accepted: u64,
    rejected: u64,
    timed_out: u64,
    failed: u64,
}

impl Counts {
    fn record(&mut self, outcome: Outcome) {
        self.attempted = self.attempted.saturating_add(1);
        match outcome {
            Outcome::Accepted => self.accepted = self.accepted.saturating_add(1),
            Outcome::Rejected => self.rejected = self.rejected.saturating_add(1),
            Outcome::TimedOut => self.timed_out = self.timed_out.saturating_add(1),
            Outcome::Failed => self.failed = self.failed.saturating_add(1),
        }
    }

    fn merge(&mut self, other: Self) {
        self.attempted = self.attempted.saturating_add(other.attempted);
        self.accepted = self.accepted.saturating_add(other.accepted);
        self.rejected = self.rejected.saturating_add(other.rejected);
        self.timed_out = self.timed_out.saturating_add(other.timed_out);
        self.failed = self.failed.saturating_add(other.failed);
    }

    fn validate_round(self, clients: usize, capacity: usize) -> Result<Self> {
        if self.attempted != u64::try_from(clients).unwrap_or(u64::MAX)
            || self.accepted > u64::try_from(capacity).unwrap_or(u64::MAX)
            || self.accepted == 0
        {
            bail!("load invariants failed");
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug)]
enum Outcome {
    Accepted,
    Rejected,
    TimedOut,
    Failed,
}

#[derive(Debug, Serialize)]
struct Report {
    mode: &'static str,
    rounds: u64,
    elapsed_ms: u64,
    relay: Counts,
    discovery: Counts,
    start_rss_bytes: Option<u64>,
    peak_rss_bytes: Option<u64>,
    end_rss_bytes: Option<u64>,
    start_open_fds: Option<u64>,
    peak_open_fds: Option<u64>,
    end_open_fds: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default)]
struct Resources {
    rss_bytes: Option<u64>,
    open_fds: Option<u64>,
}

impl Resources {
    fn peaks(self, next: Self) -> Self {
        Self {
            rss_bytes: option_max(self.rss_bytes, next.rss_bytes),
            open_fds: option_max(self.open_fds, next.open_fds),
        }
    }
}

struct LocalNode {
    peer: PeerId,
    address: Multiaddr,
    cancellation: CancellationToken,
    task: tokio::task::JoinHandle<Result<(), node::NodeError>>,
    event_task: tokio::task::JoinHandle<()>,
}

impl LocalNode {
    async fn start(capacity: usize) -> Result<Self> {
        let config = NodeConfig {
            listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse()?],
            max_reservations: capacity,
            max_reservations_per_peer: 1,
            max_connections_per_ip: MAX_CLIENTS,
            connection_attempts_per_ip_per_minute: 1_200,
            max_connections: 1_024,
            event_capacity: 8_192,
            operations_address: None,
            shutdown_grace_period: Duration::ZERO,
            discovery_registrations_per_peer: 1,
            discovery_registrations_total: capacity,
            discovery_registrations_per_namespace: 1,
            discovery_results: capacity.min(64),
            discovery_allow_private_addresses: true,
            ..NodeConfig::default()
        };
        let (peer, mut events, server) =
            NodeServer::new(identity::Keypair::generate_ed25519(), &config)?;
        let cancellation = CancellationToken::new();
        let task = tokio::spawn(server.run(cancellation.clone()));
        let address = timeout(ATTEMPT_TIMEOUT, async {
            loop {
                match events.recv().await {
                    Some(NodeEvent::Listening { address }) => break Some(address),
                    Some(_) => {}
                    None => break None,
                }
            }
        })
        .await
        .context("node listener timed out")?
        .context("node event stream closed")?;
        let event_task = tokio::spawn(async move { while events.recv().await.is_some() {} });
        Ok(Self {
            peer,
            address,
            cancellation,
            task,
            event_task,
        })
    }

    async fn shutdown(self) -> Result<()> {
        self.cancellation.cancel();
        self.task.await.context("node task panicked")??;
        self.event_task.await.context("node event task panicked")?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Workload {
    Relay,
    Discovery,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let (report, json) = match cli.command {
        Command::Relay(args) => {
            let json = args.json;
            (run_once("relay", args, Workload::Relay).await?, json)
        }
        Command::Discovery(args) => {
            let json = args.json;
            (
                run_once("discovery", args, Workload::Discovery).await?,
                json,
            )
        }
        Command::Soak(args) => {
            let json = args.json;
            (run_soak(args).await?, json)
        }
    };
    print_report(&report, json)
}

async fn run_once(mode: &'static str, args: LoadArgs, workload: Workload) -> Result<Report> {
    validate_args(args.clients, args.capacity, args.hold)?;
    let started = Instant::now();
    let start = resources();
    let node = LocalNode::start(args.capacity).await?;
    let counts = run_round(
        workload,
        node.peer,
        &node.address,
        args.clients,
        args.hold,
        0,
    )
    .await
    .validate_round(args.clients, args.capacity)?;
    if node.task.is_finished() {
        bail!("node stopped during load");
    }
    let peak = start.peaks(resources());
    node.shutdown().await?;
    let end = resources();
    Ok(Report {
        mode,
        rounds: 1,
        elapsed_ms: millis(started.elapsed()),
        relay: if matches!(workload, Workload::Relay) {
            counts
        } else {
            Counts::default()
        },
        discovery: if matches!(workload, Workload::Discovery) {
            counts
        } else {
            Counts::default()
        },
        start_rss_bytes: start.rss_bytes,
        peak_rss_bytes: peak.rss_bytes,
        end_rss_bytes: end.rss_bytes,
        start_open_fds: start.open_fds,
        peak_open_fds: peak.open_fds,
        end_open_fds: end.open_fds,
    })
}

async fn run_soak(args: SoakArgs) -> Result<Report> {
    validate_args(args.clients, args.capacity, args.hold)?;
    if args.duration.is_zero() || args.duration > MAX_SOAK_DURATION {
        bail!("soak duration is outside bounds");
    }
    let started = Instant::now();
    let deadline = started + args.duration;
    let start = resources();
    let mut peak = start;
    let mut relay = Counts::default();
    let mut discovery = Counts::default();
    let mut rounds = 0_u64;
    let node = LocalNode::start(args.capacity).await?;
    while Instant::now() < deadline {
        let relay_round = run_round(
            Workload::Relay,
            node.peer,
            &node.address,
            args.clients,
            args.hold,
            rounds,
        )
        .await
        .validate_round(args.clients, args.capacity)?;
        relay.merge(relay_round);
        if Instant::now() < deadline {
            let discovery_round = run_round(
                Workload::Discovery,
                node.peer,
                &node.address,
                args.clients,
                args.hold,
                rounds,
            )
            .await
            .validate_round(args.clients, args.capacity)?;
            discovery.merge(discovery_round);
        }
        rounds = rounds.saturating_add(1);
        peak = peak.peaks(resources());
        if node.task.is_finished() {
            bail!("node stopped during soak");
        }
    }
    node.shutdown().await?;
    let end = resources();
    peak = peak.peaks(end);
    Ok(Report {
        mode: "soak",
        rounds,
        elapsed_ms: millis(started.elapsed()),
        relay,
        discovery,
        start_rss_bytes: start.rss_bytes,
        peak_rss_bytes: peak.rss_bytes,
        end_rss_bytes: end.rss_bytes,
        start_open_fds: start.open_fds,
        peak_open_fds: peak.open_fds,
        end_open_fds: end.open_fds,
    })
}

async fn run_round(
    workload: Workload,
    node_peer: PeerId,
    node_address: &Multiaddr,
    clients: usize,
    hold: Duration,
    seed: u64,
) -> Counts {
    let mut tasks = JoinSet::new();
    for index in 0..clients {
        let address = node_address.clone();
        tasks.spawn(async move {
            match workload {
                Workload::Relay => relay_attempt(node_peer, address, hold).await,
                Workload::Discovery => {
                    discovery_attempt(node_peer, address, hold, seed, index).await
                }
            }
        });
    }
    let mut counts = Counts::default();
    while let Some(result) = tasks.join_next().await {
        counts.record(result.unwrap_or(Outcome::Failed));
    }
    counts
}

async fn relay_attempt(node_peer: PeerId, node_address: Multiaddr, hold: Duration) -> Outcome {
    let Ok((client, mut events, cancellation, task)) = start_client() else {
        return Outcome::Failed;
    };
    let outcome = async {
        let Ok(address) = format!("{node_address}/p2p/{node_peer}/p2p-circuit").parse() else {
            return Outcome::Failed;
        };
        if client.listen(address).await.is_err() {
            return Outcome::Rejected;
        }
        match timeout(ATTEMPT_TIMEOUT, async {
            loop {
                match events.recv().await {
                    Some(NetworkEvent::RelayReservation { renewal: false, .. }) => break true,
                    Some(NetworkEvent::Disconnected { .. }) | None => break false,
                    Some(_) => {}
                }
            }
        })
        .await
        {
            Ok(true) => {
                tokio::time::sleep(hold).await;
                Outcome::Accepted
            }
            Ok(false) => Outcome::Rejected,
            Err(_) => Outcome::TimedOut,
        }
    }
    .await;
    cancellation.cancel();
    let _ = task.await;
    outcome
}

async fn discovery_attempt(
    node_peer: PeerId,
    node_address: Multiaddr,
    hold: Duration,
    seed: u64,
    index: usize,
) -> Outcome {
    let Ok((client, mut events, cancellation, task)) = start_client() else {
        return Outcome::Failed;
    };
    let outcome = async {
        let Ok(listen_address) = "/ip4/127.0.0.1/tcp/0".parse() else {
            return Outcome::Failed;
        };
        if client.listen(listen_address).await.is_err() {
            return Outcome::Failed;
        }
        let Some(address) = wait_for_listener(&mut events).await else {
            return Outcome::TimedOut;
        };
        if client.add_discovery_address(address).await.is_err() {
            return Outcome::Failed;
        }
        let namespace = namespace(seed, index);
        if client
            .register(node_peer, node_address, namespace.clone(), 30)
            .await
            .is_err()
        {
            return Outcome::Rejected;
        }
        let registered = timeout(ATTEMPT_TIMEOUT, async {
            loop {
                match events.recv().await {
                    Some(NetworkEvent::DiscoveryRegistered { .. }) => break Some(true),
                    Some(NetworkEvent::DiscoveryFailed { .. }) => break Some(false),
                    Some(_) => {}
                    None => break None,
                }
            }
        })
        .await;
        match registered {
            Ok(Some(true)) => {
                tokio::time::sleep(hold).await;
                let _ = client.unregister(node_peer, namespace).await;
                Outcome::Accepted
            }
            Ok(Some(false)) => Outcome::Rejected,
            Ok(None) => Outcome::Failed,
            Err(_) => Outcome::TimedOut,
        }
    }
    .await;
    cancellation.cancel();
    let _ = task.await;
    outcome
}

fn start_client() -> Result<
    (
        network::NetworkClient,
        mpsc::Receiver<NetworkEvent>,
        CancellationToken,
        tokio::task::JoinHandle<()>,
    ),
    network::NetworkError,
> {
    let config = NetworkConfig {
        command_capacity: 8,
        event_capacity: 16,
        request_timeout: ATTEMPT_TIMEOUT,
        max_concurrent_streams: 4,
        max_established_connections: 4,
        max_connections_per_peer: 2,
        ..NetworkConfig::default()
    };
    let (client, events, driver) =
        NetworkDriver::new(identity::Keypair::generate_ed25519(), &config)?;
    let cancellation = CancellationToken::new();
    let task = tokio::spawn(driver.run(cancellation.clone()));
    Ok((client, events, cancellation, task))
}

async fn wait_for_listener(events: &mut mpsc::Receiver<NetworkEvent>) -> Option<Multiaddr> {
    timeout(ATTEMPT_TIMEOUT, async {
        loop {
            match events.recv().await {
                Some(NetworkEvent::Listening { address }) => break Some(address),
                Some(_) => {}
                None => break None,
            }
        }
    })
    .await
    .ok()
    .flatten()
}

fn namespace(seed: u64, index: usize) -> DiscoveryNamespace {
    let mut room = [0_u8; 16];
    room[..8].copy_from_slice(&seed.to_le_bytes());
    room[8..].copy_from_slice(&u64::try_from(index).unwrap_or(u64::MAX).to_le_bytes());
    DiscoveryNamespace::from_room_id(room)
}

fn validate_args(clients: usize, capacity: usize, hold: Duration) -> Result<()> {
    if clients == 0
        || clients > MAX_CLIENTS
        || capacity == 0
        || capacity > clients
        || hold < ATTEMPT_TIMEOUT
        || hold > Duration::from_hours(1)
    {
        bail!("load arguments are outside bounds");
    }
    Ok(())
}

fn print_report(report: &Report, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string(report)?);
    } else {
        println!(
            "mode={} rounds={} elapsed_ms={} relay={:?} discovery={:?}",
            report.mode, report.rounds, report.elapsed_ms, report.relay, report.discovery
        );
        println!(
            "rss_bytes start={:?} peak={:?} end={:?}; open_fds start={:?} peak={:?} end={:?}",
            report.start_rss_bytes,
            report.peak_rss_bytes,
            report.end_rss_bytes,
            report.start_open_fds,
            report.peak_open_fds,
            report.end_open_fds
        );
    }
    Ok(())
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

const fn option_max(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(if left > right { left } else { right }),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

#[cfg(target_os = "linux")]
fn resources() -> Resources {
    let rss_bytes = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                let kib = line.strip_prefix("VmRSS:")?.split_whitespace().next()?;
                kib.parse::<u64>().ok()?.checked_mul(1_024)
            })
        });
    let open_fds = std::fs::read_dir("/proc/self/fd")
        .ok()
        .map(|entries| u64::try_from(entries.count()).unwrap_or(u64::MAX));
    Resources {
        rss_bytes,
        open_fds,
    }
}

#[cfg(not(target_os = "linux"))]
const fn resources() -> Resources {
    Resources {
        rss_bytes: None,
        open_fds: None,
    }
}

fn parse_duration(value: &str) -> Result<Duration, String> {
    humantime::parse_duration(value).map_err(|_| "invalid duration".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_arguments_and_namespaces_are_bounded() {
        assert!(validate_args(32, 16, ATTEMPT_TIMEOUT).is_ok());
        assert!(validate_args(0, 0, ATTEMPT_TIMEOUT).is_err());
        assert!(validate_args(MAX_CLIENTS + 1, 1, ATTEMPT_TIMEOUT).is_err());
        assert!(validate_args(4, 5, ATTEMPT_TIMEOUT).is_err());
        assert!(namespace(1, 1) != namespace(1, 2));
    }
}
