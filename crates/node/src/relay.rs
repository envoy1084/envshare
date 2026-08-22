//! Bounded Circuit Relay v2 server.

use futures::StreamExt;
use libp2p::{
    Multiaddr, PeerId, Swarm, SwarmBuilder, identify, memory_connection_limits, ping, relay,
    swarm::{NetworkBehaviour, SwarmEvent},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{NodeConfig, NodeError};

#[derive(NetworkBehaviour)]
struct Behaviour {
    relay: relay::Behaviour,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    connection_limits: libp2p::connection_limits::Behaviour,
    memory_limits: memory_connection_limits::Behaviour,
}

/// Safe operational events emitted by a relay node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NodeEvent {
    /// A concrete transport listener is ready.
    Listening {
        /// Bound address without secret data.
        address: Multiaddr,
    },
    /// A reservation was accepted or renewed.
    ReservationAccepted {
        /// Authenticated reserving peer.
        peer: PeerId,
        /// True for renewal.
        renewed: bool,
    },
    /// A relay circuit was accepted.
    CircuitAccepted {
        /// Authenticated source peer.
        source: PeerId,
        /// Authenticated destination peer.
        destination: PeerId,
    },
    /// A reservation ended normally or timed out.
    ReservationClosed {
        /// Authenticated reserving peer.
        peer: PeerId,
    },
    /// A reservation request was denied by configured bounds or rate limits.
    ReservationDenied {
        /// Authenticated requesting peer.
        peer: PeerId,
    },
    /// A circuit request was denied by configured bounds or rate limits.
    CircuitDenied {
        /// Authenticated source peer.
        source: PeerId,
        /// Authenticated destination peer.
        destination: PeerId,
    },
}

/// Single-owner relay server swarm.
pub struct NodeServer {
    swarm: Swarm<Behaviour>,
    listen_addresses: Vec<Multiaddr>,
    events: mpsc::Sender<NodeEvent>,
}

impl NodeServer {
    /// Builds a bounded TCP/QUIC Circuit Relay v2 server.
    ///
    /// # Errors
    ///
    /// Returns a configuration failure before binding any sockets.
    pub fn new(
        keypair: libp2p::identity::Keypair,
        config: &NodeConfig,
    ) -> Result<(PeerId, mpsc::Receiver<NodeEvent>, Self), NodeError> {
        if !config.validate() {
            return Err(NodeError::Configuration);
        }
        let peer_id = keypair.public().to_peer_id();
        let behaviour = build_behaviour(&keypair, peer_id, config);
        let swarm = SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                libp2p::tcp::Config::default().nodelay(true),
                libp2p::noise::Config::new,
                libp2p::yamux::Config::default,
            )
            .map_err(|_| NodeError::Configuration)?
            .with_quic()
            .with_behaviour(|_| behaviour)
            .map_err(|_| NodeError::Configuration)?
            .build();
        let (events, event_receiver) = mpsc::channel(config.event_capacity);
        Ok((
            peer_id,
            event_receiver,
            Self {
                swarm,
                listen_addresses: config.listen_addresses.clone(),
                events,
            },
        ))
    }

    /// Binds configured listeners and runs until cancellation.
    ///
    /// # Errors
    ///
    /// Returns if any configured listener cannot be started.
    pub async fn run(mut self, cancellation: CancellationToken) -> Result<(), NodeError> {
        for address in self.listen_addresses.drain(..) {
            self.swarm
                .listen_on(address)
                .map_err(|_| NodeError::Listen)?;
        }
        loop {
            tokio::select! {
                () = cancellation.cancelled() => return Ok(()),
                event = self.swarm.select_next_some() => self.handle_event(event),
            }
        }
    }

    fn handle_event(&mut self, event: SwarmEvent<BehaviourEvent>) {
        let event = match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                self.swarm.add_external_address(address.clone());
                Some(NodeEvent::Listening { address })
            }
            SwarmEvent::Behaviour(BehaviourEvent::Relay(
                relay::Event::ReservationReqAccepted {
                    src_peer_id,
                    renewed,
                },
            )) => Some(NodeEvent::ReservationAccepted {
                peer: src_peer_id,
                renewed,
            }),
            SwarmEvent::Behaviour(BehaviourEvent::Relay(relay::Event::ReservationReqDenied {
                src_peer_id,
                ..
            })) => Some(NodeEvent::ReservationDenied { peer: src_peer_id }),
            SwarmEvent::Behaviour(BehaviourEvent::Relay(relay::Event::CircuitReqAccepted {
                src_peer_id,
                dst_peer_id,
            })) => Some(NodeEvent::CircuitAccepted {
                source: src_peer_id,
                destination: dst_peer_id,
            }),
            SwarmEvent::Behaviour(BehaviourEvent::Relay(relay::Event::CircuitReqDenied {
                src_peer_id,
                dst_peer_id,
                ..
            })) => Some(NodeEvent::CircuitDenied {
                source: src_peer_id,
                destination: dst_peer_id,
            }),
            SwarmEvent::Behaviour(BehaviourEvent::Relay(
                relay::Event::ReservationClosed { src_peer_id }
                | relay::Event::ReservationTimedOut { src_peer_id },
            )) => Some(NodeEvent::ReservationClosed { peer: src_peer_id }),
            _ => None,
        };
        if let Some(event) = event {
            let _ = self.events.try_send(event);
        }
    }
}

fn build_behaviour(
    keypair: &libp2p::identity::Keypair,
    peer_id: PeerId,
    config: &NodeConfig,
) -> Behaviour {
    let relay_config = relay::Config {
        max_reservations: config.max_reservations,
        max_reservations_per_peer: config.max_reservations_per_peer,
        reservation_duration: config.reservation_duration,
        max_circuits: config.max_circuits,
        max_circuits_per_peer: config.max_circuits_per_peer,
        max_circuit_duration: config.max_circuit_duration,
        max_circuit_bytes: config.max_circuit_bytes,
        ..relay::Config::default()
    };
    let limits = libp2p::connection_limits::ConnectionLimits::default()
        .with_max_pending_incoming(Some(config.max_connections))
        .with_max_pending_outgoing(Some(config.max_connections))
        .with_max_established(Some(config.max_connections))
        .with_max_established_per_peer(Some(config.max_connections_per_peer));
    Behaviour {
        relay: relay::Behaviour::new(peer_id, relay_config),
        identify: identify::Behaviour::new(
            identify::Config::new("/envshare/node/1.0.0".to_owned(), keypair.public())
                .with_agent_version(format!("envshare-node/{}", env!("CARGO_PKG_VERSION"))),
        ),
        ping: ping::Behaviour::default(),
        connection_limits: libp2p::connection_limits::Behaviour::new(limits),
        memory_limits: memory_connection_limits::Behaviour::with_max_bytes(
            config.max_process_memory_bytes,
        ),
    }
}

impl std::fmt::Debug for NodeServer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NodeServer")
            .field("peer_id", self.swarm.local_peer_id())
            .finish_non_exhaustive()
    }
}
