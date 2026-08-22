//! Composite production client behavior.

use std::time::Duration;

use libp2p::swarm::NetworkBehaviour;
use libp2p::{
    dcutr, identify, mdns, memory_connection_limits, ping, relay, rendezvous, request_response,
    swarm::behaviour::toggle::Toggle,
};

use crate::{NetworkConfig, PrivacyMode, codec::TransferCodec, codec::transfer_protocol};

#[derive(NetworkBehaviour)]
pub(crate) struct Behaviour {
    pub(crate) transfer: request_response::Behaviour<TransferCodec>,
    relay: relay::client::Behaviour,
    dcutr: dcutr::Behaviour,
    pub(crate) rendezvous: rendezvous::client::Behaviour,
    mdns: Toggle<mdns::tokio::Behaviour>,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    connection_limits: libp2p::connection_limits::Behaviour,
    memory_limits: memory_connection_limits::Behaviour,
}

impl Behaviour {
    pub(crate) fn new(
        keypair: &libp2p::identity::Keypair,
        config: &NetworkConfig,
        relay: relay::client::Behaviour,
    ) -> Self {
        let transfer_config = request_response::Config::default()
            .with_request_timeout(config.request_timeout)
            .with_max_concurrent_streams(config.max_concurrent_streams);
        let transfer = request_response::Behaviour::new(
            [(transfer_protocol(), request_response::ProtocolSupport::Full)],
            transfer_config,
        );
        let identify_config =
            identify::Config::new("/envshare/id/1.0.0".to_owned(), keypair.public())
                .with_agent_version(format!("envshare/{}", env!("CARGO_PKG_VERSION")))
                .with_interval(Duration::from_mins(5));
        let limits = libp2p::connection_limits::ConnectionLimits::default()
            .with_max_pending_incoming(Some(config.max_established_connections))
            .with_max_pending_outgoing(Some(config.max_established_connections))
            .with_max_established(Some(config.max_established_connections))
            .with_max_established_per_peer(Some(config.max_connections_per_peer));
        let mdns = if config.enable_mdns && config.privacy_mode != PrivacyMode::RelayOnly {
            mdns::tokio::Behaviour::new(mdns::Config::default(), keypair.public().to_peer_id()).ok()
        } else {
            None
        };
        Self {
            transfer,
            relay,
            dcutr: dcutr::Behaviour::new(keypair.public().to_peer_id()),
            rendezvous: rendezvous::client::Behaviour::new(keypair.clone()),
            mdns: Toggle::from(mdns),
            identify: identify::Behaviour::new(identify_config),
            ping: ping::Behaviour::default(),
            connection_limits: libp2p::connection_limits::Behaviour::new(limits),
            memory_limits: memory_connection_limits::Behaviour::with_max_bytes(
                config.max_process_memory_bytes,
            ),
        }
    }
}
