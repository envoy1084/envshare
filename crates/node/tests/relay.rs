//! Relay-only transfer integration through a bounded node server.

use std::{error::Error, time::Duration};

use network::{NetworkConfig, NetworkDriver, NetworkEvent, identity};
use node::{NodeConfig, NodeEvent, NodeServer};
use protocol::{CompletedResponse, OpenRequest, TransferRequest, TransferResponse};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

type NodeTask = tokio::task::JoinHandle<Result<(), node::NodeError>>;
type ClientTask = tokio::task::JoinHandle<()>;

async fn start_node(
    config: NodeConfig,
) -> Result<
    (
        network::PeerId,
        network::Multiaddr,
        tokio::sync::mpsc::Receiver<NodeEvent>,
        CancellationToken,
        NodeTask,
    ),
    Box<dyn Error>,
> {
    let (peer, mut events, node) = NodeServer::new(identity::Keypair::generate_ed25519(), &config)?;
    let cancellation = CancellationToken::new();
    let task = tokio::spawn(node.run(cancellation.clone()));
    let address = loop {
        let event = timeout(Duration::from_secs(5), events.recv())
            .await?
            .ok_or("node event stream closed")?;
        if let NodeEvent::Listening { address } = event {
            break address;
        }
    };
    Ok((peer, address, events, cancellation, task))
}

fn start_client() -> Result<
    (
        network::NetworkClient,
        tokio::sync::mpsc::Receiver<NetworkEvent>,
        CancellationToken,
        ClientTask,
    ),
    network::NetworkError,
> {
    let (client, events, driver) = NetworkDriver::new(
        identity::Keypair::generate_ed25519(),
        &NetworkConfig::default(),
    )?;
    let cancellation = CancellationToken::new();
    let task = tokio::spawn(driver.run(cancellation.clone()));
    Ok((client, events, cancellation, task))
}

async fn reserve(
    client: &network::NetworkClient,
    events: &mut tokio::sync::mpsc::Receiver<NetworkEvent>,
    node_peer: network::PeerId,
    node_address: &network::Multiaddr,
) -> Result<(), Box<dyn Error>> {
    client
        .listen(format!("{node_address}/p2p/{node_peer}/p2p-circuit").parse()?)
        .await?;
    loop {
        let event = timeout(Duration::from_secs(5), events.recv())
            .await?
            .ok_or("client event stream closed")?;
        if matches!(event, NetworkEvent::RelayReservation { renewal: false, .. }) {
            return Ok(());
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn relay_reservation_carries_a_transfer_request() -> Result<(), Box<dyn Error>> {
    let node_config = NodeConfig {
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse()?],
        ..NodeConfig::default()
    };
    let (node_peer, mut node_events, node) =
        NodeServer::new(identity::Keypair::generate_ed25519(), &node_config)?;
    let node_cancel = CancellationToken::new();
    let node_task = tokio::spawn(node.run(node_cancel.clone()));
    let node_address = loop {
        let event = timeout(Duration::from_secs(5), node_events.recv())
            .await?
            .ok_or("node event stream closed")?;
        if let NodeEvent::Listening { address } = event {
            break address;
        }
    };

    let (sender, mut sender_events, sender_driver) = NetworkDriver::new(
        identity::Keypair::generate_ed25519(),
        &NetworkConfig::default(),
    )?;
    let sender_peer = sender.local_peer_id();
    let sender_cancel = CancellationToken::new();
    let sender_task = tokio::spawn(sender_driver.run(sender_cancel.clone()));
    let reservation_address = format!("{node_address}/p2p/{node_peer}/p2p-circuit").parse()?;
    sender.listen(reservation_address).await?;
    loop {
        let event = timeout(Duration::from_secs(5), sender_events.recv())
            .await?
            .ok_or("sender event stream closed")?;
        if matches!(event, NetworkEvent::RelayReservation { renewal: false, .. }) {
            break;
        }
    }

    let (receiver, _receiver_events, receiver_driver) = NetworkDriver::new(
        identity::Keypair::generate_ed25519(),
        &NetworkConfig::default(),
    )?;
    let receiver_peer = receiver.local_peer_id();
    let receiver_cancel = CancellationToken::new();
    let receiver_task = tokio::spawn(receiver_driver.run(receiver_cancel.clone()));
    let sender_route =
        format!("{node_address}/p2p/{node_peer}/p2p-circuit/p2p/{sender_peer}").parse()?;
    let request_task = tokio::spawn(async move {
        receiver
            .request(
                sender_peer,
                sender_route,
                TransferRequest::Open(OpenRequest {
                    protocol_version: 1,
                    room_id: [1; 16],
                    receiver_nonce: [2; 32],
                    receiver_proof: [3; 32],
                }),
            )
            .await
    });
    let request_id = loop {
        let event = timeout(Duration::from_secs(5), sender_events.recv())
            .await
            .map_err(|_| "relay request did not reach sender")?
            .ok_or("sender event stream closed")?;
        if let NetworkEvent::InboundRequest {
            peer,
            request_id,
            request: TransferRequest::Open(_),
        } = event
        {
            assert_eq!(peer, receiver_peer);
            break request_id;
        }
    };
    sender
        .respond(
            request_id,
            TransferResponse::Completed(CompletedResponse {
                protocol_version: 1,
                claim_id: [4; 16],
            }),
        )
        .await?;
    assert!(matches!(
        timeout(Duration::from_secs(5), request_task).await???,
        TransferResponse::Completed(_)
    ));

    node_cancel.cancel();
    sender_cancel.cancel();
    receiver_cancel.cancel();
    node_task.await??;
    sender_task.await?;
    receiver_task.await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn client_renews_short_relay_reservation() -> Result<(), Box<dyn Error>> {
    let config = NodeConfig {
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse()?],
        reservation_duration: Duration::from_secs(2),
        ..NodeConfig::default()
    };
    let (node_peer, node_address, _node_events, node_cancel, node_task) =
        start_node(config).await?;
    let (client, mut events, client_cancel, client_task) = start_client()?;
    reserve(&client, &mut events, node_peer, &node_address).await?;
    loop {
        let event = timeout(Duration::from_secs(5), events.recv())
            .await?
            .ok_or("client event stream closed")?;
        if matches!(event, NetworkEvent::RelayReservation { renewal: true, .. }) {
            break;
        }
    }
    client_cancel.cancel();
    node_cancel.cancel();
    client_task.await?;
    node_task.await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn reservation_saturation_denies_second_peer() -> Result<(), Box<dyn Error>> {
    let config = NodeConfig {
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse()?],
        max_reservations: 1,
        max_reservations_per_peer: 1,
        ..NodeConfig::default()
    };
    let (node_peer, node_address, mut node_events, node_cancel, node_task) =
        start_node(config).await?;
    let (first, mut first_events, first_cancel, first_task) = start_client()?;
    reserve(&first, &mut first_events, node_peer, &node_address).await?;
    let (second, _second_events, second_cancel, second_task) = start_client()?;
    second
        .listen(format!("{node_address}/p2p/{node_peer}/p2p-circuit").parse()?)
        .await?;

    let mut accepted = 0;
    let mut denied = 0;
    while accepted + denied < 2 {
        match timeout(Duration::from_secs(5), node_events.recv())
            .await?
            .ok_or("node event stream closed")?
        {
            NodeEvent::ReservationAccepted { .. } => accepted += 1,
            NodeEvent::ReservationDenied { .. } => denied += 1,
            NodeEvent::Listening { .. }
            | NodeEvent::CircuitAccepted { .. }
            | NodeEvent::CircuitClosed { .. }
            | NodeEvent::ReservationClosed { .. }
            | NodeEvent::CircuitDenied { .. }
            | NodeEvent::DiscoveryRegistered { .. }
            | NodeEvent::DiscoveryUnregistered { .. }
            | NodeEvent::DiscoveryServed { .. }
            | NodeEvent::DiscoveryRejected { .. } => {}
        }
    }
    assert_eq!(accepted, 1);
    assert_eq!(denied, 1);

    first_cancel.cancel();
    second_cancel.cancel();
    node_cancel.cancel();
    first_task.await?;
    second_task.await?;
    node_task.await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn circuit_duration_limit_closes_unanswered_request() -> Result<(), Box<dyn Error>> {
    let config = NodeConfig {
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse()?],
        max_circuit_duration: Duration::from_millis(200),
        ..NodeConfig::default()
    };
    let (node_peer, node_address, _node_events, node_cancel, node_task) =
        start_node(config).await?;
    let (sender, mut sender_events, sender_cancel, sender_task) = start_client()?;
    let sender_peer = sender.local_peer_id();
    reserve(&sender, &mut sender_events, node_peer, &node_address).await?;
    let receiver_config = NetworkConfig {
        request_timeout: Duration::from_secs(2),
        ..NetworkConfig::default()
    };
    let (receiver, _receiver_events, receiver_driver) =
        NetworkDriver::new(identity::Keypair::generate_ed25519(), &receiver_config)?;
    let receiver_cancel = CancellationToken::new();
    let receiver_task = tokio::spawn(receiver_driver.run(receiver_cancel.clone()));
    let route = relay_route(&node_address, node_peer, sender_peer)?;
    let request_task =
        tokio::spawn(async move { receiver.request(sender_peer, route, open_request()).await });
    loop {
        let event = timeout(Duration::from_secs(5), sender_events.recv())
            .await?
            .ok_or("sender event stream closed")?;
        if matches!(event, NetworkEvent::InboundRequest { .. }) {
            break;
        }
    }
    assert!(
        timeout(Duration::from_secs(5), request_task)
            .await
            .map_err(|_| "duration-limited receiver request did not finish")??
            .is_err()
    );

    sender_cancel.cancel();
    receiver_cancel.cancel();
    node_cancel.cancel();
    sender_task.await?;
    receiver_task.await?;
    node_task.await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn circuit_byte_limit_interrupts_oversized_response() -> Result<(), Box<dyn Error>> {
    let config = NodeConfig {
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse()?],
        max_circuit_bytes: 1_024,
        ..NodeConfig::default()
    };
    let (node_peer, node_address, _node_events, node_cancel, node_task) =
        start_node(config).await?;
    let (sender, mut sender_events, sender_cancel, sender_task) = start_client()?;
    let sender_peer = sender.local_peer_id();
    reserve(&sender, &mut sender_events, node_peer, &node_address).await?;
    let receiver_config = NetworkConfig {
        request_timeout: Duration::from_secs(2),
        ..NetworkConfig::default()
    };
    let (receiver, _receiver_events, receiver_driver) =
        NetworkDriver::new(identity::Keypair::generate_ed25519(), &receiver_config)?;
    let receiver_cancel = CancellationToken::new();
    let receiver_task = tokio::spawn(receiver_driver.run(receiver_cancel.clone()));
    let route = relay_route(&node_address, node_peer, sender_peer)?;
    let request_task =
        tokio::spawn(async move { receiver.request(sender_peer, route, open_request()).await });
    assert!(
        timeout(Duration::from_secs(5), request_task)
            .await
            .map_err(|_| "byte-limit receiver request did not finish")??
            .is_err()
    );

    sender_cancel.cancel();
    receiver_cancel.cancel();
    node_cancel.cancel();
    sender_task.await?;
    receiver_task.await?;
    node_task.await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn stopped_relay_produces_bounded_outbound_failure() -> Result<(), Box<dyn Error>> {
    let config = NodeConfig {
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse()?],
        ..NodeConfig::default()
    };
    let (node_peer, node_address, _node_events, node_cancel, node_task) =
        start_node(config).await?;
    let (sender, mut sender_events, sender_cancel, sender_task) = start_client()?;
    let sender_peer = sender.local_peer_id();
    reserve(&sender, &mut sender_events, node_peer, &node_address).await?;
    node_cancel.cancel();
    node_task.await??;

    let (receiver, _receiver_events, receiver_cancel, receiver_task) = start_client()?;
    let route = relay_route(&node_address, node_peer, sender_peer)?;
    assert!(
        timeout(
            Duration::from_secs(5),
            receiver.request(sender_peer, route, open_request())
        )
        .await?
        .is_err()
    );
    sender_cancel.cancel();
    receiver_cancel.cancel();
    sender_task.await?;
    receiver_task.await?;
    Ok(())
}

fn relay_route(
    node_address: &network::Multiaddr,
    node_peer: network::PeerId,
    destination: network::PeerId,
) -> Result<network::Multiaddr, Box<dyn Error>> {
    Ok(format!("{node_address}/p2p/{node_peer}/p2p-circuit/p2p/{destination}").parse()?)
}

fn open_request() -> TransferRequest {
    TransferRequest::Open(OpenRequest {
        protocol_version: 1,
        room_id: [1; 16],
        receiver_nonce: [2; 32],
        receiver_proof: [3; 32],
    })
}
