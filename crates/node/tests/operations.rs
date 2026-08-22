//! Node health and graceful-drain integration coverage.

use std::{error::Error, time::Duration};

use network::{NetworkConfig, NetworkDriver, NetworkEvent, identity};
use node::{NodeConfig, NodeEvent, NodeServer};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn readiness_drops_during_drain_before_liveness() -> Result<(), Box<dyn Error>> {
    let config = NodeConfig {
        listen_addresses: vec!["/ip4/127.0.0.1/tcp/0".parse()?],
        ..NodeConfig::default()
    };
    let (node_peer, mut node_events, server) =
        NodeServer::new(identity::Keypair::generate_ed25519(), &config)?;
    let status = server.status();
    let node_cancel = CancellationToken::new();
    let node_task = tokio::spawn(server.run_graceful(node_cancel.clone(), Duration::from_secs(2)));
    let address = timeout(Duration::from_secs(5), async {
        loop {
            if let Some(NodeEvent::Listening { address }) = node_events.recv().await {
                return address;
            }
        }
    })
    .await?;
    assert!(status.is_live());
    assert!(status.is_ready());

    let (client, mut client_events, driver) = NetworkDriver::new(
        identity::Keypair::generate_ed25519(),
        &NetworkConfig::default(),
    )?;
    let client_cancel = CancellationToken::new();
    let client_task = tokio::spawn(driver.run(client_cancel.clone()));
    client.dial(node_peer, address).await?;
    timeout(Duration::from_secs(5), async {
        loop {
            if matches!(
                client_events.recv().await,
                Some(NetworkEvent::Connected { peer }) if peer == node_peer
            ) {
                return;
            }
        }
    })
    .await?;

    node_cancel.cancel();
    timeout(Duration::from_secs(1), async {
        while status.is_ready() {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(status.is_live());
    assert!(!node_task.is_finished());

    client_cancel.cancel();
    client_task.await?;
    timeout(Duration::from_secs(2), node_task).await???;
    assert!(!status.is_live());
    assert!(!status.is_ready());
    Ok(())
}
