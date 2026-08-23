//! Direct sender command.

use std::io::Write as _;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use app_core::{CoreError, DirectSender, SenderActor, SenderState, select_dotenv};
use code::ShareCode;
use crypto::derive_root;
use network::{
    DiscoveryNamespace, DiscoveryProvider, Multiaddr, NetworkConfig, NetworkDriver, NetworkEvent,
    PrivacyMode, dispatch_registration, identity, maintain_registrations,
};
use protocol::{ContentType, SecretEnvelope};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::{CliFailure, ExitCode, args::SendArgs, presentation};

use super::shared::{RunningNetwork, read_sender_input, reserve_relays};

const CONFIRMED_TRANSFER_DRAIN: Duration = Duration::from_millis(500);

pub(crate) async fn execute(arguments: SendArgs) -> Result<i32, CliFailure> {
    let expires = arguments.expires.ok_or_else(invalid_resolved_config)?;
    let network = arguments
        .network
        .as_deref()
        .ok_or_else(invalid_resolved_config)?;
    if expires.is_zero() {
        return Err(CliFailure::new(
            ExitCode::Configuration,
            "expiry must be positive",
        ));
    }
    let input = match &arguments.input {
        Some(path) => path.clone(),
        None if arguments.json || arguments.code_only => {
            return Err(CliFailure::new(
                ExitCode::Usage,
                "input file is required for machine-readable output",
            ));
        }
        None => presentation::choose_sender_input()?,
    };
    let envelope = prepare_envelope(&arguments, &input, expires)?;
    let code = ShareCode::generate().map_err(|_| CoreError::Internal)?;
    let root = derive_root(code.secret(), network).map_err(|_| CoreError::InvalidCode)?;
    let namespace = DiscoveryNamespace::from_room_id(*root.room_id().as_bytes());
    let code_text = Zeroizing::new(code.to_string());

    let ReachableNetwork {
        client,
        mut events,
        running: running_network,
        peer: sender_peer,
        advertised,
    } = start_reachable_network(&arguments).await?;
    let mut registration =
        establish_public_reachability(&arguments, &client, &mut events, &advertised, &namespace)
            .await?;

    let send_view = emit_ready(
        &arguments,
        &code_text,
        &sender_peer,
        &advertised[0],
        envelope.content_type(),
    )?;
    std::io::stdout().flush().map_err(|_| CoreError::Output)?;
    let actor = SenderActor::new(
        root,
        network.to_owned(),
        sender_peer.to_bytes(),
        &envelope,
        Instant::now()
            .checked_add(expires)
            .ok_or(CoreError::Configuration)?,
        std::time::Duration::from_secs(30),
    )?;
    let service_cancel = CancellationToken::new();
    let service = DirectSender::new(client, events, actor);
    let outcome = tokio::select! {
        result = service.run(service_cancel.clone()) => result.map_err(Into::into),
        interrupted = tokio::signal::ctrl_c() => {
            interrupted
                .map_err(|_| CoreError::Internal)
                .map_err(CliFailure::from)
                .and_then(|()| Err(CliFailure::new(ExitCode::Interrupted, "interrupted")))
        }
    };
    service_cancel.cancel();
    if matches!(outcome, Ok(SenderState::Consumed)) {
        // ResponseSent means the frame reached the negotiated stream. Keep the
        // swarm alive briefly so a relayed stream can deliver it before teardown.
        tokio::time::sleep(CONFIRMED_TRANSFER_DRAIN).await;
    }
    stop_registration(registration.take()).await;
    running_network.stop().await?;
    let state = match outcome {
        Ok(state) => state,
        Err(failure) => {
            if let Some(view) = send_view {
                view.cancel("Share stopped");
            }
            return Err(failure);
        }
    };
    finish_sender(state, send_view, &arguments)
}

fn finish_sender(
    state: SenderState,
    send_view: Option<presentation::SendView>,
    arguments: &SendArgs,
) -> Result<i32, CliFailure> {
    match state {
        SenderState::Consumed => {
            if let Some(view) = send_view {
                view.consumed()?;
            } else {
                emit_event(arguments, "consumed");
            }
            Ok(ExitCode::Success.as_i32())
        }
        SenderState::Expired => {
            if let Some(view) = send_view {
                view.cancel("Share expired");
            }
            Err(CliFailure::new(
                ExitCode::ShareUnavailable,
                "share expired before it was claimed",
            ))
        }
        SenderState::DeliveryUnknown => {
            if let Some(view) = send_view {
                view.cancel("Delivery could not be confirmed");
            }
            Err(CliFailure::new(
                ExitCode::Transfer,
                "delivery could not be confirmed; the share will not reopen",
            ))
        }
        _ => {
            if let Some(view) = send_view {
                view.cancel("Share stopped");
            }
            Err(CliFailure::new(
                ExitCode::Internal,
                "sender stopped unexpectedly",
            ))
        }
    }
}

struct ReachableNetwork {
    client: network::NetworkClient,
    events: tokio::sync::mpsc::Receiver<NetworkEvent>,
    running: RunningNetwork,
    peer: network::PeerId,
    advertised: Vec<Multiaddr>,
}

async fn start_reachable_network(arguments: &SendArgs) -> Result<ReachableNetwork, CliFailure> {
    let keypair = identity::Keypair::generate_ed25519();
    let peer = keypair.public().to_peer_id();
    let privacy = if arguments.discovery.relay_only {
        PrivacyMode::RelayOnly
    } else {
        PrivacyMode::Standard
    };
    let config = NetworkConfig {
        enable_mdns: arguments.discovery.mdns,
        privacy_mode: privacy,
        ..NetworkConfig::default()
    };
    let (client, mut events, driver) =
        NetworkDriver::new(keypair, &config).map_err(|_| CoreError::Network)?;
    let cancellation = CancellationToken::new();
    let task = tokio::spawn(driver.run(cancellation.clone()));
    let running = RunningNetwork::new(cancellation, task);
    let mut advertised = Vec::new();
    if !arguments.discovery.relay_only {
        let listen: Multiaddr = arguments
            .listen
            .parse()
            .map_err(|_| CoreError::Configuration)?;
        client
            .listen(listen)
            .await
            .map_err(|_| CoreError::Network)?;
        advertised.push(wait_for_listener(&mut events).await?);
    }
    let reservations = reserve_relays(&client, &mut events, &arguments.discovery.relays).await;
    if arguments.discovery.require_relay && reservations.is_empty() {
        return Err(relay_failure(arguments.discovery.relays.is_empty()));
    }
    let relay_addresses = reservations
        .into_iter()
        .map(|(_, address)| address)
        .collect::<Vec<_>>();
    if !relay_addresses.is_empty() && !arguments.discovery.nodes.is_empty() {
        advertised.clear();
    }
    advertised.extend(relay_addresses);
    if advertised.is_empty() {
        return Err(CliFailure::new(
            ExitCode::Network,
            "no direct listener or relay reservation is available",
        ));
    }
    Ok(ReachableNetwork {
        client,
        events,
        running,
        peer,
        advertised,
    })
}

const fn relay_failure(missing: bool) -> CliFailure {
    if missing {
        CliFailure::new(
            ExitCode::Configuration,
            "relay mode requires at least one configured relay",
        )
    } else {
        CliFailure::new(
            ExitCode::Network,
            "no configured relay accepted a reservation",
        )
    }
}

fn prepare_envelope(
    arguments: &SendArgs,
    input: &std::path::Path,
    expires: std::time::Duration,
) -> Result<SecretEnvelope, CliFailure> {
    let raw = Zeroizing::new(read_sender_input(input)?);
    let (payload, content_type) = if arguments.keys.is_empty() {
        (raw.as_slice().to_vec(), ContentType::DotenvRaw)
    } else {
        (
            select_dotenv(&raw, &arguments.keys, arguments.allow_missing_keys)?,
            ContentType::DotenvNormalized,
        )
    };
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CoreError::Internal)?
        .as_millis();
    let now_ms = u64::try_from(now_ms).map_err(|_| CoreError::Internal)?;
    let expiry_ms = u64::try_from(expires.as_millis()).map_err(|_| CoreError::Configuration)?;
    let expires_at = now_ms
        .checked_add(expiry_ms)
        .ok_or(CoreError::Configuration)?;
    SecretEnvelope::new(content_type, None, now_ms, expires_at, payload)
        .map_err(|_| CoreError::Transfer.into())
}

struct RegistrationMaintenance {
    cancellation: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

async fn establish_public_reachability(
    arguments: &SendArgs,
    client: &network::NetworkClient,
    events: &mut tokio::sync::mpsc::Receiver<NetworkEvent>,
    advertised: &[Multiaddr],
    namespace: &DiscoveryNamespace,
) -> Result<Option<RegistrationMaintenance>, CliFailure> {
    if arguments.discovery.nodes.is_empty() {
        return Ok(None);
    }
    for address in advertised {
        client
            .add_discovery_address(address.clone())
            .await
            .map_err(|_| CoreError::Network)?;
    }
    let ttl_seconds = arguments
        .expires
        .ok_or_else(invalid_resolved_config)?
        .as_secs()
        .clamp(30, 300);
    let dispatched = dispatch_registration(
        client,
        &arguments.discovery.nodes,
        namespace,
        ttl_seconds,
        4,
    )
    .await;
    if dispatched.iter().all(|(_, result)| result.is_err())
        || !wait_for_public_registration(events, arguments.discovery.nodes.len()).await?
    {
        return Err(CliFailure::new(
            ExitCode::Network,
            "no discovery node accepted the share registration",
        ));
    }
    let cancellation = CancellationToken::new();
    let provider = client.clone();
    let nodes = arguments.discovery.nodes.clone();
    let renewal_namespace = namespace.clone();
    let renewal_cancel = cancellation.clone();
    let task = tokio::spawn(async move {
        maintain_registrations(
            &provider,
            &nodes,
            &renewal_namespace,
            ttl_seconds,
            4,
            renewal_cancel,
        )
        .await;
    });
    Ok(Some(RegistrationMaintenance { cancellation, task }))
}

async fn stop_registration(registration: Option<RegistrationMaintenance>) {
    if let Some(registration) = registration {
        registration.cancellation.cancel();
        let _ = registration.task.await;
    }
}

async fn wait_for_public_registration(
    events: &mut tokio::sync::mpsc::Receiver<NetworkEvent>,
    node_count: usize,
) -> Result<bool, CoreError> {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let mut failed = std::collections::HashSet::new();
        loop {
            match events.recv().await.ok_or(CoreError::Network)? {
                NetworkEvent::DiscoveryRegistered { .. } => return Ok(true),
                NetworkEvent::DiscoveryFailed { node } => {
                    failed.insert(node);
                    if failed.len() == node_count {
                        return Ok(false);
                    }
                }
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| CoreError::Network)?
}

async fn wait_for_listener(
    events: &mut tokio::sync::mpsc::Receiver<NetworkEvent>,
) -> Result<Multiaddr, CoreError> {
    loop {
        match events.recv().await.ok_or(CoreError::Network)? {
            NetworkEvent::Listening { address } => return Ok(address),
            NetworkEvent::InboundRequest { .. }
            | NetworkEvent::Connected { .. }
            | NetworkEvent::Disconnected { .. }
            | NetworkEvent::RelayReservation { .. }
            | NetworkEvent::OutboundRelayCircuit { .. }
            | NetworkEvent::InboundRelayCircuit { .. }
            | NetworkEvent::DiscoveryRegistered { .. }
            | NetworkEvent::DiscoveryResults { .. }
            | NetworkEvent::DiscoveryFailed { .. }
            | NetworkEvent::DiscoveryExpired { .. }
            | NetworkEvent::LanDiscovered { .. }
            | NetworkEvent::LanExpired { .. } => {}
        }
    }
}

fn emit_ready(
    arguments: &SendArgs,
    code: &str,
    peer: &network::PeerId,
    address: &Multiaddr,
    content_type: ContentType,
) -> Result<Option<presentation::SendView>, CliFailure> {
    if arguments.code_only {
        println!("{code}");
    } else if arguments.json {
        println!(
            "{}",
            serde_json::json!({
                "event": "ready",
                "peer_id": peer.to_string(),
                "address": address.to_string(),
                "payload_format": payload_format(content_type)
            })
        );
        eprintln!("Share code: {code}");
    } else if arguments.verbose {
        println!("Share code: {code}");
        println!("Sender peer: {peer}");
        if arguments.discovery.relay_only {
            println!("Relay address: {address}");
        } else {
            println!("Direct address: {address}");
        }
        if content_type == ContentType::DotenvNormalized {
            println!("Payload format: normalized selected keys");
        }
    } else {
        return presentation::show_share(code).map(Some);
    }
    Ok(None)
}

const fn payload_format(content_type: ContentType) -> &'static str {
    match content_type {
        ContentType::DotenvRaw => "dotenv_raw",
        ContentType::DotenvNormalized => "dotenv_normalized",
    }
}

fn emit_event(arguments: &SendArgs, event: &'static str) {
    if arguments.json {
        println!("{}", serde_json::json!({ "event": event }));
    } else if !arguments.code_only {
        println!("Share consumed.");
    }
}

const fn invalid_resolved_config() -> CliFailure {
    CliFailure::new(ExitCode::Internal, "resolved configuration is incomplete")
}
