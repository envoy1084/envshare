//! Localhost transport integration tests for the bounded swarm API.

use std::{error::Error, time::Duration};

use network::{Multiaddr, NetworkClient, NetworkConfig, NetworkDriver, NetworkEvent};
use protocol::{CompletedResponse, OpenRequest, TransferRequest, TransferResponse};
use tokio::{sync::mpsc, task::JoinHandle, time::timeout};
use tokio_util::sync::CancellationToken;

async fn next_listen_address(
    events: &mut mpsc::Receiver<NetworkEvent>,
) -> Result<Multiaddr, Box<dyn Error>> {
    loop {
        let event = timeout(Duration::from_secs(5), events.recv())
            .await?
            .ok_or("network event stream closed")?;
        if let NetworkEvent::Listening { address } = event {
            return Ok(address);
        }
    }
}

async fn next_request(
    events: &mut mpsc::Receiver<NetworkEvent>,
) -> Result<(network::PeerId, network::InboundRequestId), Box<dyn Error>> {
    loop {
        let event = timeout(Duration::from_secs(5), events.recv())
            .await?
            .ok_or("network event stream closed")?;
        if let NetworkEvent::InboundRequest {
            peer,
            request_id,
            request: TransferRequest::Open(_),
        } = event
        {
            return Ok((peer, request_id));
        }
    }
}

fn start_driver() -> Result<
    (
        NetworkClient,
        mpsc::Receiver<NetworkEvent>,
        CancellationToken,
        JoinHandle<()>,
    ),
    network::NetworkError,
> {
    let (client, events, driver) = NetworkDriver::new(
        libp2p::identity::Keypair::generate_ed25519(),
        &NetworkConfig::default(),
    )?;
    let cancellation = CancellationToken::new();
    let task = tokio::spawn(driver.run(cancellation.clone()));
    Ok((client, events, cancellation, task))
}

async fn transfer_over(listen_address: &str) -> Result<(), Box<dyn Error>> {
    let (sender, mut sender_events, sender_cancel, sender_task) = start_driver()?;
    let (receiver, _receiver_events, receiver_cancel, receiver_task) = start_driver()?;
    sender.listen(listen_address.parse()?).await?;
    let address = next_listen_address(&mut sender_events).await?;
    let sender_peer = sender.local_peer_id();
    let receiver_peer = receiver.local_peer_id();
    let request_task = tokio::spawn(async move {
        receiver
            .request(
                sender_peer,
                address,
                TransferRequest::Open(OpenRequest {
                    protocol_version: 1,
                    room_id: [1; 16],
                    receiver_nonce: [2; 32],
                    receiver_proof: [3; 32],
                }),
            )
            .await
    });
    let (authenticated_peer, request_id) = next_request(&mut sender_events).await?;
    assert_eq!(authenticated_peer, receiver_peer);
    sender
        .respond(
            request_id,
            TransferResponse::Completed(CompletedResponse {
                protocol_version: 1,
                claim_id: [4; 16],
            }),
        )
        .await?;
    let response = timeout(Duration::from_secs(5), request_task).await???;
    assert!(matches!(response, TransferResponse::Completed(_)));

    sender_cancel.cancel();
    receiver_cancel.cancel();
    sender_task.await?;
    receiver_task.await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn tcp_noise_yamux_transfers_a_bounded_message() -> Result<(), Box<dyn Error>> {
    transfer_over("/ip4/127.0.0.1/tcp/0").await
}

#[tokio::test(flavor = "multi_thread")]
async fn quic_transfers_a_bounded_message() -> Result<(), Box<dyn Error>> {
    transfer_over("/ip4/127.0.0.1/udp/0/quic-v1").await
}
