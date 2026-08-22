//! Optional local-network discovery without the vulnerable libp2p DNS parser path.

use std::{error::Error, time::Duration};

use network::{NetworkConfig, NetworkDriver, NetworkEvent, identity};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread")]
async fn dns_sd_discovers_an_ephemeral_local_peer() -> Result<(), Box<dyn Error>> {
    let config = NetworkConfig {
        enable_mdns: true,
        ..NetworkConfig::default()
    };
    let (first, mut first_events, first_driver) =
        NetworkDriver::new(identity::Keypair::generate_ed25519(), &config)?;
    let (second, mut second_events, second_driver) =
        NetworkDriver::new(identity::Keypair::generate_ed25519(), &config)?;
    let first_peer = first.local_peer_id();
    let second_peer = second.local_peer_id();
    let first_cancel = CancellationToken::new();
    let second_cancel = CancellationToken::new();
    let first_task = tokio::spawn(first_driver.run(first_cancel.clone()));
    let second_task = tokio::spawn(second_driver.run(second_cancel.clone()));
    first.listen("/ip4/0.0.0.0/tcp/0".parse()?).await?;
    second.listen("/ip4/0.0.0.0/tcp/0".parse()?).await?;

    wait_for_peer(&mut first_events, second_peer).await?;
    wait_for_peer(&mut second_events, first_peer).await?;
    first_cancel.cancel();
    second_cancel.cancel();
    first_task.await?;
    second_task.await?;
    Ok(())
}

async fn wait_for_peer(
    events: &mut tokio::sync::mpsc::Receiver<NetworkEvent>,
    expected: network::PeerId,
) -> Result<(), Box<dyn Error>> {
    timeout(Duration::from_secs(10), async {
        loop {
            if let Some(NetworkEvent::LanDiscovered { peers }) = events.recv().await
                && peers.iter().any(|peer| peer.peer == expected)
            {
                break;
            }
        }
    })
    .await?;
    Ok(())
}
