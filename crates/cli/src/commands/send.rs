//! Direct sender command.

use std::io::Write as _;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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

use crate::{CliFailure, ExitCode, args::SendArgs};

use super::shared::read_sender_input;

pub(crate) async fn execute(arguments: SendArgs) -> Result<i32, CliFailure> {
    if arguments.expires.is_zero() {
        return Err(CliFailure::new(
            ExitCode::Configuration,
            "expiry must be positive",
        ));
    }
    let envelope = prepare_envelope(&arguments)?;
    let code = ShareCode::generate().map_err(|_| CoreError::Internal)?;
    let root =
        derive_root(code.secret(), &arguments.network).map_err(|_| CoreError::InvalidCode)?;
    let namespace = DiscoveryNamespace::from_room_id(*root.room_id().as_bytes());
    let code_text = Zeroizing::new(code.to_string());

    let keypair = identity::Keypair::generate_ed25519();
    let sender_peer = keypair.public().to_peer_id();
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
    let listen: Multiaddr = arguments
        .listen
        .parse()
        .map_err(|_| CoreError::Configuration)?;
    let driver_cancel = CancellationToken::new();
    let driver_task = tokio::spawn(driver.run(driver_cancel.clone()));
    client
        .listen(listen)
        .await
        .map_err(|_| CoreError::Network)?;
    let advertised = wait_for_listener(&mut events).await?;
    let mut registration =
        establish_public_reachability(&arguments, &client, &mut events, &advertised, &namespace)
            .await?;

    emit_ready(&arguments, &code_text, &sender_peer, &advertised);
    std::io::stdout().flush().map_err(|_| CoreError::Output)?;
    let actor = SenderActor::new(
        root,
        arguments.network,
        sender_peer.to_bytes(),
        &envelope,
        Instant::now()
            .checked_add(arguments.expires)
            .ok_or(CoreError::Configuration)?,
        std::time::Duration::from_secs(30),
    )?;
    let service_cancel = CancellationToken::new();
    let service = DirectSender::new(client, events, actor);
    let state = tokio::select! {
        result = service.run(service_cancel.clone()) => result?,
        interrupted = tokio::signal::ctrl_c() => {
            interrupted.map_err(|_| CoreError::Internal)?;
            service_cancel.cancel();
            stop_registration(registration.take()).await;
            driver_cancel.cancel();
            let _ = driver_task.await;
            return Err(CliFailure::new(ExitCode::Interrupted, "interrupted"));
        }
    };
    stop_registration(registration.take()).await;
    driver_cancel.cancel();
    driver_task
        .await
        .map_err(|_| CliFailure::new(ExitCode::Internal, "network task failed"))?;
    match state {
        SenderState::Consumed => {
            emit_event(arguments.json, "consumed");
            Ok(ExitCode::Success.as_i32())
        }
        SenderState::Expired => Err(CliFailure::new(
            ExitCode::ShareUnavailable,
            "share expired before it was claimed",
        )),
        SenderState::DeliveryUnknown => Err(CliFailure::new(
            ExitCode::Transfer,
            "delivery could not be confirmed; the share will not reopen",
        )),
        _ => Err(CliFailure::new(
            ExitCode::Internal,
            "sender stopped unexpectedly",
        )),
    }
}

fn prepare_envelope(arguments: &SendArgs) -> Result<SecretEnvelope, CliFailure> {
    let raw = Zeroizing::new(read_sender_input(&arguments.input)?);
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
    let expiry_ms =
        u64::try_from(arguments.expires.as_millis()).map_err(|_| CoreError::Configuration)?;
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
    advertised: &Multiaddr,
    namespace: &DiscoveryNamespace,
) -> Result<Option<RegistrationMaintenance>, CliFailure> {
    if arguments.discovery.relay_only
        && !advertised
            .iter()
            .any(|protocol| matches!(protocol, network::MultiaddrProtocol::P2pCircuit))
    {
        return Err(CliFailure::new(
            ExitCode::Configuration,
            "relay-only mode requires a relay circuit listen address",
        ));
    }
    if arguments.discovery.nodes.is_empty() {
        return Ok(None);
    }
    client
        .add_discovery_address(advertised.clone())
        .await
        .map_err(|_| CoreError::Network)?;
    let ttl_seconds = arguments.expires.as_secs().clamp(30, 300);
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

fn emit_ready(arguments: &SendArgs, code: &str, peer: &network::PeerId, address: &Multiaddr) {
    if arguments.code_only {
        println!("{code}");
    } else if arguments.json {
        println!(
            "{}",
            serde_json::json!({
                "event": "ready",
                "peer_id": peer.to_string(),
                "address": address.to_string()
            })
        );
        eprintln!("Share code: {code}");
    } else {
        println!("Share code: {code}");
        println!("Sender peer: {peer}");
        println!("Direct address: {address}");
    }
}

fn emit_event(json: bool, event: &'static str) {
    if json {
        println!("{}", serde_json::json!({ "event": event }));
    } else {
        println!("Share consumed.");
    }
}
