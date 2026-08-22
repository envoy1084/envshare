//! Shared direct receiver and network helpers.

use std::{io::IsTerminal as _, str::FromStr};

use app_core::{CoreError, DirectReceiver, PendingDirectOffer, ReceiverSession, read_bounded};
use code::ShareCode;
use crypto::derive_root;
use network::{
    CandidatePolicy, CandidateSet, DiscoveryNamespace, NetworkConfig, NetworkDriver, NetworkEvent,
    PeerId, PrivacyMode, dispatch_discovery, identity,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::{CliFailure, ExitCode, args::ConnectionArgs};

pub(crate) struct RunningNetwork {
    pub cancellation: CancellationToken,
    pub task: JoinHandle<()>,
}

impl RunningNetwork {
    pub async fn stop(self) -> Result<(), CliFailure> {
        self.cancellation.cancel();
        self.task
            .await
            .map_err(|_| CliFailure::new(ExitCode::Internal, "network task failed"))
    }
}

pub(crate) async fn receive_direct(
    arguments: &mut ConnectionArgs,
) -> Result<(PendingDirectOffer, RunningNetwork), CliFailure> {
    let network_id = arguments
        .network
        .as_deref()
        .ok_or_else(invalid_resolved_config)?
        .to_owned();
    let code_text = read_code(arguments)?;
    let code = ShareCode::from_str(code_text.trim())
        .map_err(|_| CliFailure::new(ExitCode::InvalidCode, "invalid share code"))?;
    let keypair = identity::Keypair::generate_ed25519();
    let receiver_peer = keypair.public().to_peer_id();
    let root = derive_root(code.secret(), &network_id).map_err(|_| CoreError::InvalidCode)?;
    let namespace = DiscoveryNamespace::from_room_id(*root.room_id().as_bytes());
    let privacy = if arguments.discovery.relay_only {
        PrivacyMode::RelayOnly
    } else {
        PrivacyMode::Standard
    };
    let config = NetworkConfig {
        enable_mdns: arguments.discovery.mdns,
        privacy_mode: privacy,
        request_timeout: std::time::Duration::from_secs(5),
        ..NetworkConfig::default()
    };
    let (client, mut events, driver) =
        NetworkDriver::new(keypair, &config).map_err(|_| CoreError::Network)?;
    let cancellation = CancellationToken::new();
    let task = tokio::spawn(driver.run(cancellation.clone()));
    let network = RunningNetwork { cancellation, task };

    if let (Some(peer), Some(address)) = (&arguments.peer, &arguments.address) {
        let sender_peer = PeerId::from_str(peer)
            .map_err(|_| CliFailure::new(ExitCode::Configuration, "invalid sender Peer ID"))?;
        let sender_address = network::Multiaddr::from_str(address)
            .map_err(|_| CliFailure::new(ExitCode::Configuration, "invalid sender address"))?;
        let session = receiver_session(root, &network_id, sender_peer, receiver_peer)?;
        let pending = DirectReceiver::new(client, session, sender_peer, sender_address)
            .receive()
            .await?;
        return Ok((pending, network));
    }
    let routes = discover_routes(&client, &mut events, arguments, &namespace, privacy).await?;
    for route in routes {
        if route.peer == receiver_peer {
            continue;
        }
        let candidate_root =
            derive_root(code.secret(), &network_id).map_err(|_| CoreError::InvalidCode)?;
        let session = receiver_session(candidate_root, &network_id, route.peer, receiver_peer)?;
        if let Ok(pending) = DirectReceiver::new(client.clone(), session, route.peer, route.address)
            .receive()
            .await
        {
            return Ok((pending, network));
        }
    }
    network.stop().await?;
    Err(CliFailure::new(
        ExitCode::NotFoundOrUnauthorized,
        "share not found or capability was not authorized",
    ))
}

async fn discover_routes(
    client: &network::NetworkClient,
    events: &mut tokio::sync::mpsc::Receiver<NetworkEvent>,
    arguments: &ConnectionArgs,
    namespace: &DiscoveryNamespace,
    privacy: PrivacyMode,
) -> Result<Vec<network::RouteCandidate>, CliFailure> {
    if arguments.discovery.nodes.is_empty() && !arguments.discovery.mdns {
        return Err(CliFailure::new(
            ExitCode::Configuration,
            "provide a direct peer/address, discovery node, or --mdns",
        ));
    }
    let _ = dispatch_discovery(client, &arguments.discovery.nodes, namespace, 4).await;
    let mut candidates = CandidateSet::new(CandidatePolicy {
        allow_lan: arguments.discovery.lan,
        privacy,
        ..CandidatePolicy::default()
    })
    .map_err(|_| CoreError::Configuration)?;
    let mut pending_nodes = arguments
        .discovery
        .nodes
        .iter()
        .map(|node| node.peer)
        .collect::<std::collections::HashSet<_>>();
    let discovery_window = if arguments.discovery.mdns {
        std::time::Duration::from_secs(3)
    } else {
        std::time::Duration::from_secs(5)
    };
    let _ = tokio::time::timeout(discovery_window, async {
        loop {
            match events.recv().await {
                Some(NetworkEvent::DiscoveryResults { node, peers }) => {
                    pending_nodes.remove(&node);
                    for peer in peers {
                        candidates.insert(peer);
                    }
                }
                Some(NetworkEvent::DiscoveryFailed { node }) => {
                    pending_nodes.remove(&node);
                }
                Some(NetworkEvent::LanDiscovered { peers }) => {
                    for peer in peers {
                        candidates.insert(peer);
                    }
                }
                Some(_) => {}
                None => break,
            }
            if pending_nodes.is_empty() && !arguments.discovery.mdns {
                break;
            }
        }
    })
    .await;
    Ok(candidates.into_ranked())
}

fn receiver_session(
    root: crypto::DerivedRoot,
    network_id: &str,
    sender_peer: PeerId,
    receiver_peer: PeerId,
) -> Result<ReceiverSession, CliFailure> {
    ReceiverSession::new(
        root,
        network_id.to_owned(),
        sender_peer.to_bytes(),
        receiver_peer.to_bytes(),
        random_receiver_nonce()?,
    )
    .map_err(Into::into)
}

fn read_code(arguments: &mut ConnectionArgs) -> Result<Zeroizing<String>, CliFailure> {
    if let Some(code) = arguments.code.take() {
        return Ok(Zeroizing::new(code));
    }
    if arguments.code_stdin {
        let bytes = read_bounded(std::io::stdin().lock(), 128)?;
        return String::from_utf8(bytes)
            .map(Zeroizing::new)
            .map_err(|_| CliFailure::new(ExitCode::InvalidCode, "invalid share code"));
    }
    if !std::io::stdin().is_terminal() {
        return Err(CliFailure::new(
            ExitCode::Usage,
            "interactive code prompt requires a terminal; use --code-stdin",
        ));
    }
    rpassword::prompt_password("Share code: ")
        .map(Zeroizing::new)
        .map_err(|_| CliFailure::new(ExitCode::InvalidCode, "could not read share code"))
}

fn random_receiver_nonce() -> Result<[u8; 32], CliFailure> {
    let mut nonce = [0_u8; 32];
    getrandom::fill(&mut nonce)
        .map_err(|_| CliFailure::new(ExitCode::Internal, "secure randomness unavailable"))?;
    Ok(nonce)
}

const fn invalid_resolved_config() -> CliFailure {
    CliFailure::new(ExitCode::Internal, "resolved configuration is incomplete")
}

pub(crate) fn read_sender_input(path: &std::path::Path) -> Result<Vec<u8>, CliFailure> {
    if path == std::path::Path::new("-") {
        return read_bounded(std::io::stdin().lock(), protocol::MAX_PAYLOAD_BYTES)
            .map_err(Into::into);
    }
    let file = std::fs::File::open(path)
        .map_err(|_| CliFailure::new(ExitCode::Output, "could not open input"))?;
    let metadata = file
        .metadata()
        .map_err(|_| CliFailure::new(ExitCode::Output, "could not inspect input"))?;
    if !metadata.is_file() {
        return Err(CliFailure::new(
            ExitCode::Output,
            "input must be a regular file",
        ));
    }
    read_bounded(file, protocol::MAX_PAYLOAD_BYTES).map_err(Into::into)
}
