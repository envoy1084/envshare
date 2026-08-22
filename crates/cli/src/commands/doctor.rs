//! Disposable, non-secret connectivity and local-safety diagnostics.

use std::{collections::HashSet, time::Duration};

use app_core::{PrivateOutputOptions, write_private_atomic};
use network::{
    DiscoveryNamespace, DiscoveryProvider, NetworkConfig, NetworkDriver, NetworkEvent,
    dispatch_discovery, dispatch_registration, dispatch_unregistration, identity,
};
use tokio_util::sync::CancellationToken;

use crate::{CliFailure, ExitCode, args::DoctorArgs, config::ClientConfig};

pub(crate) async fn execute(
    arguments: DoctorArgs,
    config: &ClientConfig,
) -> Result<i32, CliFailure> {
    check_clock()?;
    check_private_output()?;
    let (_network_id, nodes, require_relay) =
        config.diagnostic_nodes(arguments.network.as_deref())?;
    if nodes.is_empty() {
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
    client
        .listen(
            "/ip4/127.0.0.1/tcp/0"
                .parse()
                .map_err(|_| network_failure())?,
        )
        .await
        .map_err(|_| network_failure())?;
    let address = wait_for_listener(&mut events).await?;
    client
        .add_discovery_address(address)
        .await
        .map_err(|_| network_failure())?;
    let namespace = disposable_namespace()?;
    if require_relay {
        for node in &nodes {
            let relay_address = format!("{}/p2p/{}/p2p-circuit", node.address, node.peer)
                .parse()
                .map_err(|_| network_failure())?;
            client
                .listen(relay_address)
                .await
                .map_err(|_| network_failure())?;
        }
    }
    let _ = dispatch_registration(&client, &nodes, &namespace, 30, 4).await;
    let registration = collect_registration_results(&mut events, nodes.len(), require_relay).await;
    let _ = dispatch_discovery(&client, &nodes, &namespace, 4).await;
    let discovered = collect_discovery_results(&mut events, nodes.len()).await;
    dispatch_unregistration(&client, &nodes, &namespace, 4).await;
    cancellation.cancel();
    task.await.map_err(|_| network_failure())?;

    emit_result(&arguments, nodes.len(), &registration, discovered);
    let relay_ok = !require_relay || registration.relay_nodes.len() == nodes.len();
    if registration.registered_nodes.len() == nodes.len() && discovered == nodes.len() && relay_ok {
        Ok(ExitCode::Success.as_i32())
    } else {
        Err(network_failure())
    }
}

struct RegistrationResults {
    registered_nodes: HashSet<network::PeerId>,
    relay_nodes: HashSet<network::PeerId>,
}

async fn collect_registration_results(
    events: &mut tokio::sync::mpsc::Receiver<NetworkEvent>,
    expected: usize,
    require_relay: bool,
) -> RegistrationResults {
    let mut results = RegistrationResults {
        registered_nodes: HashSet::new(),
        relay_nodes: HashSet::new(),
    };
    let _ = tokio::time::timeout(Duration::from_secs(10), async {
        while results.registered_nodes.len() < expected
            || (require_relay && results.relay_nodes.len() < expected)
        {
            match events.recv().await {
                Some(NetworkEvent::DiscoveryRegistered { node, .. }) => {
                    results.registered_nodes.insert(node);
                }
                Some(NetworkEvent::RelayReservation { relay_peer, .. }) => {
                    results.relay_nodes.insert(relay_peer);
                }
                Some(_) => {}
                None => break,
            }
        }
    })
    .await;
    results
}

async fn collect_discovery_results(
    events: &mut tokio::sync::mpsc::Receiver<NetworkEvent>,
    expected: usize,
) -> usize {
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
    nodes.len()
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
    expected: usize,
    registration: &RegistrationResults,
    discovered: usize,
) {
    if arguments.json {
        println!(
            "{}",
            serde_json::json!({
                "event": "doctor_complete",
                "nodes_expected": expected,
                "registrations": registration.registered_nodes.len(),
                "relay_reservations": registration.relay_nodes.len(),
                "discoveries": discovered,
            })
        );
    } else {
        println!("Local clock and private output: ok");
        println!(
            "Discovery registrations: {}/{expected}",
            registration.registered_nodes.len()
        );
        println!("Discovery queries: {discovered}/{expected}");
        if arguments.verbose {
            println!(
                "Relay reservations: {}/{expected}",
                registration.relay_nodes.len()
            );
        }
    }
}

const fn network_failure() -> CliFailure {
    CliFailure::new(ExitCode::Network, "network diagnostics failed")
}
