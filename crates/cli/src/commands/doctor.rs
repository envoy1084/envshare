//! Disposable, non-secret connectivity and local-safety diagnostics.

use std::{collections::HashSet, time::Duration};

use app_core::{PrivateOutputOptions, write_private_atomic};
use network::{
    DiscoveryNamespace, DiscoveryProvider, NetworkConfig, NetworkDriver, NetworkEvent,
    dispatch_discovery, dispatch_registration, dispatch_unregistration, identity,
};
use tokio_util::sync::CancellationToken;

use crate::{CliFailure, ExitCode, args::DoctorArgs, config::ClientConfig};

use super::shared::{RunningNetwork, reserve_relays};

pub(crate) async fn execute(
    arguments: DoctorArgs,
    config: &ClientConfig,
) -> Result<i32, CliFailure> {
    check_clock()?;
    check_private_output()?;
    let profile = config.diagnostic_profile(arguments.network.as_deref())?;
    if profile.rendezvous.is_empty() {
        return Err(CliFailure::new(
            ExitCode::Configuration,
            "selected network has no discovery nodes",
        ));
    }
    let (client, mut events, driver) = NetworkDriver::new(
        identity::Keypair::generate_ed25519(),
        &NetworkConfig::default(),
    )
    .map_err(|_| network_failure())?;
    let cancellation = CancellationToken::new();
    let task = tokio::spawn(driver.run(cancellation.clone()));
    let running_network = RunningNetwork::new(cancellation, task);
    client
        .listen(
            "/ip4/127.0.0.1/tcp/0"
                .parse()
                .map_err(|_| network_failure())?,
        )
        .await
        .map_err(|_| network_failure())?;
    let address = wait_for_listener(&mut events).await?;
    let direct = check_route_connectivity(&client, &mut events, &address, None).await;
    client
        .add_discovery_address(address.clone())
        .await
        .map_err(|_| network_failure())?;
    let namespace = disposable_namespace()?;
    let relay_reservations = reserve_relays(&client, &mut events, &profile.relays).await;
    let mut relay_circuits = HashSet::new();
    for (relay_peer, relay_address) in &relay_reservations {
        if check_route_connectivity(&client, &mut events, relay_address, Some(*relay_peer)).await {
            relay_circuits.insert(*relay_peer);
        }
        client
            .add_discovery_address(relay_address.clone())
            .await
            .map_err(|_| network_failure())?;
    }
    let _ = dispatch_registration(&client, &profile.rendezvous, &namespace, 30, 4).await;
    let registration = collect_registration_results(&mut events, profile.rendezvous.len()).await;
    let _ = dispatch_discovery(&client, &profile.rendezvous, &namespace, 4).await;
    let discovered = collect_discovery_results(&mut events, profile.rendezvous.len()).await;
    dispatch_unregistration(&client, &profile.rendezvous, &namespace, 4).await;
    running_network.stop().await?;

    emit_result(
        &arguments,
        &profile,
        direct,
        &registration,
        &discovered,
        &relay_reservations.iter().map(|(peer, _)| *peer).collect(),
        &relay_circuits,
    );
    let relay_ok = !profile.require_relay
        || (relay_reservations.len() == profile.relays.len()
            && relay_circuits.len() == profile.relays.len());
    if direct
        && registration.len() == profile.rendezvous.len()
        && discovered.len() == profile.rendezvous.len()
        && relay_ok
    {
        Ok(ExitCode::Success.as_i32())
    } else {
        Err(network_failure())
    }
}

async fn collect_registration_results(
    events: &mut tokio::sync::mpsc::Receiver<NetworkEvent>,
    expected: usize,
) -> HashSet<network::PeerId> {
    let mut results = HashSet::new();
    let _ = tokio::time::timeout(Duration::from_secs(10), async {
        while results.len() < expected {
            match events.recv().await {
                Some(NetworkEvent::DiscoveryRegistered { node, .. }) => {
                    results.insert(node);
                }
                Some(_) => {}
                None => break,
            }
        }
    })
    .await;
    results
}

async fn check_route_connectivity(
    client: &network::NetworkClient,
    events: &mut tokio::sync::mpsc::Receiver<NetworkEvent>,
    address: &network::Multiaddr,
    expected_relay: Option<network::PeerId>,
) -> bool {
    let Ok((probe, mut probe_events, driver)) = NetworkDriver::new(
        identity::Keypair::generate_ed25519(),
        &NetworkConfig::default(),
    ) else {
        return false;
    };
    let probe_peer = probe.local_peer_id();
    let cancellation = CancellationToken::new();
    let task = tokio::spawn(driver.run(cancellation.clone()));
    let started = probe
        .dial(client.local_peer_id(), address.clone())
        .await
        .is_ok();
    let connected = started
        && tokio::time::timeout(Duration::from_secs(5), async {
            let mut connected = false;
            let mut circuit = expected_relay.is_none();
            loop {
                tokio::select! {
                    event = events.recv() => {
                        match event {
                            Some(NetworkEvent::Connected { peer }) if peer == probe_peer => connected = true,
                            Some(NetworkEvent::InboundRelayCircuit { source_peer })
                                if source_peer == probe_peer => circuit = true,
                            None => return false,
                            _ => {}
                        }
                    }
                    event = probe_events.recv() => {
                        match event {
                            Some(NetworkEvent::Connected { peer }) if peer == client.local_peer_id() => connected = true,
                            Some(NetworkEvent::OutboundRelayCircuit { relay_peer })
                                if Some(relay_peer) == expected_relay => circuit = true,
                            None => return false,
                            _ => {}
                        }
                    }
                }
                if connected && circuit {
                    return true;
                }
            }
        })
        .await
        .unwrap_or(false);
    cancellation.cancel();
    let _ = task.await;
    connected
}

async fn collect_discovery_results(
    events: &mut tokio::sync::mpsc::Receiver<NetworkEvent>,
    expected: usize,
) -> HashSet<network::PeerId> {
    let mut nodes = HashSet::new();
    let _ = tokio::time::timeout(Duration::from_secs(5), async {
        while nodes.len() < expected {
            match events.recv().await {
                Some(NetworkEvent::DiscoveryResults { node, .. }) => {
                    nodes.insert(node);
                }
                Some(_) => {}
                None => break,
            }
        }
    })
    .await;
    nodes
}

async fn wait_for_listener(
    events: &mut tokio::sync::mpsc::Receiver<NetworkEvent>,
) -> Result<network::Multiaddr, CliFailure> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(NetworkEvent::Listening { address }) = events.recv().await {
                return Ok(address);
            }
        }
    })
    .await
    .map_err(|_| network_failure())?
}

fn disposable_namespace() -> Result<DiscoveryNamespace, CliFailure> {
    let mut room = [0_u8; 16];
    getrandom::fill(&mut room)
        .map_err(|_| CliFailure::new(ExitCode::Internal, "secure randomness unavailable"))?;
    Ok(DiscoveryNamespace::from_room_id(room))
}

fn check_clock() -> Result<(), CliFailure> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|_| ())
        .map_err(|_| CliFailure::new(ExitCode::Configuration, "local clock is invalid"))
}

fn check_private_output() -> Result<(), CliFailure> {
    let mut random = [0_u8; 8];
    getrandom::fill(&mut random)
        .map_err(|_| CliFailure::new(ExitCode::Internal, "secure randomness unavailable"))?;
    let name = random.iter().fold(String::new(), |mut output, byte| {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
        output
    });
    let path = std::env::temp_dir().join(format!("envshare-doctor-{name}"));
    write_private_atomic(&path, b"doctor", PrivateOutputOptions::default())?;
    std::fs::remove_file(path)
        .map_err(|_| CliFailure::new(ExitCode::Output, "private output cleanup failed"))
}

fn emit_result(
    arguments: &DoctorArgs,
    profile: &crate::config::DiagnosticProfile,
    direct: bool,
    registrations: &HashSet<network::PeerId>,
    discoveries: &HashSet<network::PeerId>,
    relay_reservations: &HashSet<network::PeerId>,
    relay_circuits: &HashSet<network::PeerId>,
) {
    let expected = profile.rendezvous.len();
    let relays_expected = profile.relays.len();
    let dns_endpoints = profile
        .rendezvous
        .iter()
        .chain(&profile.relays)
        .filter(|node| {
            node.address.iter().any(|protocol| {
                matches!(
                    protocol,
                    network::MultiaddrProtocol::Dns(_)
                        | network::MultiaddrProtocol::Dns4(_)
                        | network::MultiaddrProtocol::Dns6(_)
                )
            })
        })
        .count();
    if arguments.json {
        let mut result = serde_json::json!({
            "event": "doctor_complete",
            "local_safety": true,
            "direct_connectivity": direct,
            "dns_endpoints": dns_endpoints,
            "nodes_expected": expected,
            "registrations": registrations.len(),
            "relays_expected": relays_expected,
            "relay_reservations": relay_reservations.len(),
            "relay_circuits": relay_circuits.len(),
            "discoveries": discoveries.len(),
        });
        if arguments.verbose {
            result["rendezvous"] = serde_json::Value::Array(
                profile
                    .rendezvous
                    .iter()
                    .map(|node| {
                        serde_json::json!({
                            "peer_id": node.peer.to_string(),
                            "registered": registrations.contains(&node.peer),
                            "discovered": discoveries.contains(&node.peer),
                        })
                    })
                    .collect(),
            );
            result["relays"] = serde_json::Value::Array(
                profile
                    .relays
                    .iter()
                    .map(|node| {
                        serde_json::json!({
                            "peer_id": node.peer.to_string(),
                            "reserved": relay_reservations.contains(&node.peer),
                            "circuit": relay_circuits.contains(&node.peer),
                        })
                    })
                    .collect(),
            );
        }
        println!("{result}");
    } else {
        println!("Local clock and private output: ok");
        println!(
            "Direct TCP, Noise, and Yamux connectivity: {}",
            if direct { "ok" } else { "failed" }
        );
        println!(
            "Discovery registrations: {}/{expected}",
            registrations.len()
        );
        println!("Discovery queries: {}/{expected}", discoveries.len());
        println!("Configured DNS endpoints: {dns_endpoints}");
        println!(
            "Relay reservations/circuits: {}/{}, {}/{}",
            relay_reservations.len(),
            relays_expected,
            relay_circuits.len(),
            relays_expected
        );
        if arguments.verbose {
            for node in &profile.rendezvous {
                println!(
                    "Rendezvous {}: registration={}, discovery={}",
                    node.peer,
                    status(registrations.contains(&node.peer)),
                    status(discoveries.contains(&node.peer))
                );
            }
            for node in &profile.relays {
                println!(
                    "Relay {}: reservation={}, circuit={}",
                    node.peer,
                    status(relay_reservations.contains(&node.peer)),
                    status(relay_circuits.contains(&node.peer))
                );
            }
        }
    }
}

const fn status(success: bool) -> &'static str {
    if success { "ok" } else { "failed" }
}

const fn network_failure() -> CliFailure {
    CliFailure::new(ExitCode::Network, "network diagnostics failed")
}
