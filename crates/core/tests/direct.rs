//! End-to-end encrypted direct transfer over a real localhost swarm.

use std::{error::Error, time::Duration};

use app_core::{DirectReceiver, DirectSender, ReceiverSession, SenderActor, SenderState};
use code::ShareCodeSecret;
use crypto::derive_root;
use network::{NetworkConfig, NetworkDriver, NetworkEvent, identity};
use protocol::{ContentType, SecretEnvelope};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread")]
async fn direct_transfer_authenticates_decrypts_and_consumes() -> Result<(), Box<dyn Error>> {
    let sender_key = identity::Keypair::generate_ed25519();
    let (sender_client, mut sender_events, sender_driver) =
        NetworkDriver::new(sender_key, &NetworkConfig::default())?;
    let sender_driver_cancel = CancellationToken::new();
    let sender_driver_task = tokio::spawn(sender_driver.run(sender_driver_cancel.clone()));
    sender_client
        .listen("/ip4/127.0.0.1/tcp/0".parse()?)
        .await?;
    let sender_address = loop {
        let event = timeout(Duration::from_secs(5), sender_events.recv())
            .await?
            .ok_or("sender event stream closed")?;
        if let NetworkEvent::Listening { address } = event {
            break address;
        }
    };

    let receiver_key = identity::Keypair::generate_ed25519();
    let (receiver_client, _receiver_events, receiver_driver) =
        NetworkDriver::new(receiver_key, &NetworkConfig::default())?;
    let receiver_driver_cancel = CancellationToken::new();
    let receiver_driver_task = tokio::spawn(receiver_driver.run(receiver_driver_cancel.clone()));

    let sender_peer = sender_client.local_peer_id();
    let receiver_peer = receiver_client.local_peer_id();
    let envelope = SecretEnvelope::new(
        ContentType::DotenvRaw,
        None,
        1_000,
        2_000,
        b"DATABASE_URL=sentinel\n".to_vec(),
    )?;
    let now = std::time::Instant::now();
    let sender_actor = SenderActor::new(
        derive_root(&ShareCodeSecret::new([8; 20]), "public-v1")?,
        "public-v1".to_owned(),
        sender_peer.to_bytes(),
        &envelope,
        now + Duration::from_mins(1),
        Duration::from_secs(10),
    )?;
    let sender_service_cancel = CancellationToken::new();
    let sender_service = DirectSender::new(sender_client, sender_events, sender_actor);
    let sender_service_task = tokio::spawn(sender_service.run(sender_service_cancel.clone()));

    let receiver_session = ReceiverSession::new(
        derive_root(&ShareCodeSecret::new([8; 20]), "public-v1")?,
        "public-v1".to_owned(),
        sender_peer.to_bytes(),
        receiver_peer.to_bytes(),
        [9; 32],
    )?;
    let pending = DirectReceiver::new(
        receiver_client,
        receiver_session,
        sender_peer,
        sender_address,
    )
    .receive()
    .await?;
    assert_eq!(pending.envelope().payload(), b"DATABASE_URL=sentinel\n");
    let received = pending.acknowledge().await?;
    assert_eq!(received.payload(), b"DATABASE_URL=sentinel\n");
    assert_eq!(
        timeout(Duration::from_secs(5), sender_service_task).await???,
        SenderState::Consumed
    );

    sender_service_cancel.cancel();
    sender_driver_cancel.cancel();
    receiver_driver_cancel.cancel();
    sender_driver_task.await?;
    receiver_driver_task.await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn acknowledgement_loss_closes_real_swarm_share_to_another_receiver()
-> Result<(), Box<dyn Error>> {
    let sender_key = identity::Keypair::generate_ed25519();
    let (sender_client, mut sender_events, sender_driver) =
        NetworkDriver::new(sender_key, &NetworkConfig::default())?;
    let sender_driver_cancel = CancellationToken::new();
    let sender_driver_task = tokio::spawn(sender_driver.run(sender_driver_cancel.clone()));
    sender_client
        .listen("/ip4/127.0.0.1/tcp/0".parse()?)
        .await?;
    let sender_address = loop {
        let event = timeout(Duration::from_secs(5), sender_events.recv())
            .await?
            .ok_or("sender event stream closed")?;
        if let NetworkEvent::Listening { address } = event {
            break address;
        }
    };

    let first_key = identity::Keypair::generate_ed25519();
    let (first_client, _first_events, first_driver) =
        NetworkDriver::new(first_key, &NetworkConfig::default())?;
    let first_driver_cancel = CancellationToken::new();
    let first_driver_task = tokio::spawn(first_driver.run(first_driver_cancel.clone()));
    let second_key = identity::Keypair::generate_ed25519();
    let (second_client, _second_events, second_driver) =
        NetworkDriver::new(second_key, &NetworkConfig::default())?;
    let second_driver_cancel = CancellationToken::new();
    let second_driver_task = tokio::spawn(second_driver.run(second_driver_cancel.clone()));

    let sender_peer = sender_client.local_peer_id();
    let envelope = SecretEnvelope::new(
        ContentType::DotenvRaw,
        None,
        1_000,
        2_000,
        b"TOKEN=ack-loss-sentinel\n".to_vec(),
    )?;
    let sender_actor = SenderActor::new(
        derive_root(&ShareCodeSecret::new([10; 20]), "public-v1")?,
        "public-v1".to_owned(),
        sender_peer.to_bytes(),
        &envelope,
        std::time::Instant::now() + Duration::from_mins(1),
        Duration::from_millis(100),
    )?;
    let sender_service_cancel = CancellationToken::new();
    let sender_service = DirectSender::new(sender_client, sender_events, sender_actor);
    let sender_service_task = tokio::spawn(sender_service.run(sender_service_cancel.clone()));

    let first_session = ReceiverSession::new(
        derive_root(&ShareCodeSecret::new([10; 20]), "public-v1")?,
        "public-v1".to_owned(),
        sender_peer.to_bytes(),
        first_client.local_peer_id().to_bytes(),
        [11; 32],
    )?;
    let pending = DirectReceiver::new(
        first_client,
        first_session,
        sender_peer,
        sender_address.clone(),
    )
    .receive()
    .await?;
    assert_eq!(pending.envelope().payload(), b"TOKEN=ack-loss-sentinel\n");
    drop(pending);

    tokio::time::sleep(Duration::from_millis(150)).await;
    let second_session = ReceiverSession::new(
        derive_root(&ShareCodeSecret::new([10; 20]), "public-v1")?,
        "public-v1".to_owned(),
        sender_peer.to_bytes(),
        second_client.local_peer_id().to_bytes(),
        [12; 32],
    )?;
    let second_result =
        DirectReceiver::new(second_client, second_session, sender_peer, sender_address)
            .receive()
            .await;
    assert!(matches!(
        second_result,
        Err(app_core::CoreError::ShareUnavailable)
    ));
    assert_eq!(
        timeout(Duration::from_secs(5), sender_service_task).await???,
        SenderState::DeliveryUnknown
    );

    sender_service_cancel.cancel();
    sender_driver_cancel.cancel();
    first_driver_cancel.cancel();
    second_driver_cancel.cancel();
    sender_driver_task.await?;
    first_driver_task.await?;
    second_driver_task.await?;
    Ok(())
}
