//! Relay-only transfer integration through a bounded node server.

use std::{error::Error, time::Duration};

use network::{NetworkConfig, NetworkDriver, NetworkEvent, identity};
use node::{NodeConfig, NodeEvent, NodeServer};
use protocol::{CompletedResponse, OpenRequest, TransferRequest, TransferResponse};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

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
            .await?
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
