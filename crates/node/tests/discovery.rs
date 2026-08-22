//! Rendezvous registration and lookup against the bounded node service.

use std::{error::Error, time::Duration};

use network::{
    DiscoveryNamespace, DiscoveryProvider, NetworkConfig, NetworkDriver, NetworkEvent, identity,
};
use node::{NodeConfig, NodeServer};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread")]
async fn opaque_registration_is_discovered_and_unregistered() -> Result<(), Box<dyn Error>> {
    let config = NodeConfig {
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse()?],
        ..NodeConfig::default()
    };
    let (node_peer, mut node_events, node) =
        NodeServer::new(identity::Keypair::generate_ed25519(), &config)?;
    let node_cancel = CancellationToken::new();
    let node_task = tokio::spawn(node.run(node_cancel.clone()));
    let node_address = loop {
        let event = timeout(Duration::from_secs(5), node_events.recv())
            .await?
            .ok_or("node event stream closed")?;
        if let node::NodeEvent::Listening { address } = event {
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
    sender.listen("/ip4/127.0.0.1/tcp/0".parse()?).await?;
    let sender_address = wait_for_listener(&mut sender_events).await?;
    sender.add_discovery_address(sender_address.clone()).await?;

    let namespace = DiscoveryNamespace::from_room_id([7; 16]);
    sender
        .register(node_peer, node_address.clone(), namespace.clone(), 30)
        .await?;
    wait_for_registration(&mut sender_events, node_peer).await?;

    let (receiver, mut receiver_events, receiver_driver) = NetworkDriver::new(
        identity::Keypair::generate_ed25519(),
        &NetworkConfig::default(),
    )?;
    let receiver_cancel = CancellationToken::new();
    let receiver_task = tokio::spawn(receiver_driver.run(receiver_cancel.clone()));
    receiver
        .discover(node_peer, node_address.clone(), namespace.clone())
        .await?;
    let peers = wait_for_results(&mut receiver_events, node_peer).await?;
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].peer, sender_peer);
    assert!(peers[0].addresses.contains(&sender_address));

    sender.unregister(node_peer, namespace.clone()).await?;
    timeout(Duration::from_secs(5), async {
        loop {
            if matches!(
                node_events.recv().await,
                Some(node::NodeEvent::DiscoveryUnregistered { peer }) if peer == sender_peer
            ) {
                break;
            }
        }
    })
    .await?;
    receiver
        .discover(node_peer, node_address, namespace)
        .await?;
    assert!(
        wait_for_results(&mut receiver_events, node_peer)
            .await?
            .is_empty()
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
async fn expired_registration_is_not_returned() -> Result<(), Box<dyn Error>> {
    let config = NodeConfig {
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse()?],
        discovery_min_ttl_seconds: 1,
        discovery_max_ttl_seconds: 1,
        ..NodeConfig::default()
    };
    let (node_peer, mut node_events, node) =
        NodeServer::new(identity::Keypair::generate_ed25519(), &config)?;
    let node_cancel = CancellationToken::new();
    let node_task = tokio::spawn(node.run(node_cancel.clone()));
    let node_address = wait_for_node_listener(&mut node_events).await?;
    let (client, mut events, client_cancel, client_task) = start_client()?;
    client.listen("/ip4/127.0.0.1/tcp/0".parse()?).await?;
    let address = wait_for_listener(&mut events).await?;
    client.add_discovery_address(address).await?;
    let namespace = DiscoveryNamespace::from_room_id([8; 16]);
    client
        .register(node_peer, node_address.clone(), namespace.clone(), 1)
        .await?;
    wait_for_registration(&mut events, node_peer).await?;
    timeout(Duration::from_secs(3), async {
        loop {
            if matches!(
                node_events.recv().await,
                Some(node::NodeEvent::DiscoveryUnregistered { .. })
            ) {
                break;
            }
        }
    })
    .await?;
    client.discover(node_peer, node_address, namespace).await?;
    assert!(wait_for_results(&mut events, node_peer).await?.is_empty());

    client_cancel.cancel();
    node_cancel.cancel();
    client_task.await?;
    node_task.await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn global_registration_limit_rejects_overload() -> Result<(), Box<dyn Error>> {
    let config = NodeConfig {
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse()?],
        discovery_registrations_per_peer: 1,
        discovery_registrations_total: 1,
        ..NodeConfig::default()
    };
    let (node_peer, mut node_events, node) =
        NodeServer::new(identity::Keypair::generate_ed25519(), &config)?;
    let node_cancel = CancellationToken::new();
    let node_task = tokio::spawn(node.run(node_cancel.clone()));
    let node_address = wait_for_node_listener(&mut node_events).await?;
    let (first, mut first_events, first_cancel, first_task) = start_client()?;
    let (second, mut second_events, second_cancel, second_task) = start_client()?;
    for (client, events) in [(&first, &mut first_events), (&second, &mut second_events)] {
        client.listen("/ip4/127.0.0.1/tcp/0".parse()?).await?;
        let address = wait_for_listener(events).await?;
        client.add_discovery_address(address).await?;
    }
    first
        .register(
            node_peer,
            node_address.clone(),
            DiscoveryNamespace::from_room_id([1; 16]),
            30,
        )
        .await?;
    wait_for_registration(&mut first_events, node_peer).await?;
    second
        .register(
            node_peer,
            node_address,
            DiscoveryNamespace::from_room_id([2; 16]),
            30,
        )
        .await?;
    wait_for_failure(&mut second_events, node_peer).await?;

    first_cancel.cancel();
    second_cancel.cancel();
    node_cancel.cancel();
    first_task.await?;
    second_task.await?;
    node_task.await??;
    Ok(())
}

fn start_client() -> Result<
    (
        network::NetworkClient,
        tokio::sync::mpsc::Receiver<NetworkEvent>,
        CancellationToken,
        tokio::task::JoinHandle<()>,
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

async fn wait_for_node_listener(
    events: &mut tokio::sync::mpsc::Receiver<node::NodeEvent>,
) -> Result<network::Multiaddr, Box<dyn Error>> {
    timeout(Duration::from_secs(5), async {
        loop {
            if let Some(node::NodeEvent::Listening { address }) = events.recv().await {
                break Ok(address);
            }
        }
    })
    .await?
}

async fn wait_for_listener(
    events: &mut tokio::sync::mpsc::Receiver<NetworkEvent>,
) -> Result<network::Multiaddr, Box<dyn Error>> {
    timeout(Duration::from_secs(5), async {
        loop {
            if let Some(NetworkEvent::Listening { address }) = events.recv().await {
                break Ok(address);
            }
        }
    })
    .await?
}

async fn wait_for_registration(
    events: &mut tokio::sync::mpsc::Receiver<NetworkEvent>,
    expected_node: network::PeerId,
) -> Result<(), Box<dyn Error>> {
    timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await {
                Some(NetworkEvent::DiscoveryRegistered { node, .. }) if node == expected_node => {
                    break Ok(());
                }
                Some(NetworkEvent::DiscoveryFailed { .. }) => {
                    break Err("registration failed".into());
                }
                Some(_) => {}
                None => break Err("client event stream closed".into()),
            }
        }
    })
    .await?
}

async fn wait_for_results(
    events: &mut tokio::sync::mpsc::Receiver<NetworkEvent>,
    expected_node: network::PeerId,
) -> Result<Vec<network::DiscoveredPeer>, Box<dyn Error>> {
    timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await {
                Some(NetworkEvent::DiscoveryResults { node, peers }) if node == expected_node => {
                    break Ok(peers);
                }
                Some(NetworkEvent::DiscoveryFailed { .. }) => break Err("discovery failed".into()),
                Some(_) => {}
                None => break Err("client event stream closed".into()),
            }
        }
    })
    .await?
}

async fn wait_for_failure(
    events: &mut tokio::sync::mpsc::Receiver<NetworkEvent>,
    expected_node: network::PeerId,
) -> Result<(), Box<dyn Error>> {
    timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await {
                Some(NetworkEvent::DiscoveryFailed { node }) if node == expected_node => {
                    break Ok(());
                }
                Some(_) => {}
                None => break Err("client event stream closed".into()),
            }
        }
    })
    .await?
}
