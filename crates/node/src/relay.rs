//! Bounded Circuit Relay v2 server.

use std::collections::HashSet;

use futures::StreamExt;
use libp2p::core::transport::ListenerId;
use libp2p::{
    Multiaddr, PeerId, Swarm, SwarmBuilder, identify, memory_connection_limits, ping, relay,
    request_response,
    swarm::{NetworkBehaviour, SwarmEvent},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{NodeConfig, NodeError, NodeStatus, admission, rendezvous};

#[derive(NetworkBehaviour)]
struct Behaviour {
    admission: admission::Behaviour,
    relay: relay::Behaviour,
    rendezvous: rendezvous::Behaviour,
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
    /// A relayed circuit closed.
    CircuitClosed {
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
    /// One signed discovery registration was accepted.
    DiscoveryRegistered {
        /// Authenticated registering peer.
        peer: PeerId,
    },
    /// One registration was explicitly removed or expired.
    DiscoveryUnregistered {
        /// Authenticated registered peer.
        peer: PeerId,
    },
    /// A bounded discovery response was served.
    DiscoveryServed {
        /// Authenticated querying peer.
        peer: PeerId,
        /// Number of signed registrations returned.
        result_count: usize,
    },
    /// A discovery operation was rejected safely.
    DiscoveryRejected {
        /// Authenticated requesting peer.
        peer: PeerId,
    },
}

/// Single-owner relay server swarm.
pub struct NodeServer {
    swarm: Swarm<Behaviour>,
    listen_addresses: Vec<Multiaddr>,
    events: mpsc::Sender<NodeEvent>,
    discovery: rendezvous::Store,
    status: NodeStatus,
    ready_listeners: HashSet<ListenerId>,
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
        let status = NodeStatus::default();
        status.expect_listeners(config.listen_addresses.len());
        status.configure_capacities(
            config.max_connections,
            config.max_reservations,
            config.max_circuits,
            config.discovery_registrations_total,
        );
        let behaviour = build_behaviour(&keypair, peer_id, config, status.clone());
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
                discovery: rendezvous::Store::new(config),
                status,
                ready_listeners: HashSet::new(),
            },
        ))
    }

    /// Returns shared non-secret health and metric state.
    #[must_use]
    pub fn status(&self) -> NodeStatus {
        self.status.clone()
    }

    /// Binds configured listeners and runs until cancellation.
    ///
    /// # Errors
    ///
    /// Returns if any configured listener cannot be started.
    pub async fn run(mut self, cancellation: CancellationToken) -> Result<(), NodeError> {
        self.run_loop(cancellation, None).await
    }

    /// Runs until cancellation, then drains established peers up to a deadline.
    ///
    /// # Errors
    ///
    /// Returns if any configured listener cannot be started.
    pub async fn run_graceful(
        mut self,
        cancellation: CancellationToken,
        grace_period: std::time::Duration,
    ) -> Result<(), NodeError> {
        self.run_loop(cancellation, Some(grace_period)).await
    }

    async fn run_loop(
        &mut self,
        cancellation: CancellationToken,
        grace_period: Option<std::time::Duration>,
    ) -> Result<(), NodeError> {
        self.status.start();
        let result = self.serve(cancellation, grace_period).await;
        self.status.stop();
        result
    }

    async fn serve(
        &mut self,
        cancellation: CancellationToken,
        grace_period: Option<std::time::Duration>,
    ) -> Result<(), NodeError> {
        let mut listeners = Vec::with_capacity(self.listen_addresses.len());
        for address in self.listen_addresses.drain(..) {
            listeners.push(
                self.swarm
                    .listen_on(address)
                    .map_err(|_| NodeError::Listen)?,
            );
        }
        let mut expiry = tokio::time::interval(std::time::Duration::from_secs(1));
        expiry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                () = cancellation.cancelled() => {
                    let Some(grace_period) = grace_period else { return Ok(()) };
                    return self.drain(listeners, grace_period, &mut expiry).await;
                },
                event = self.swarm.select_next_some() => self.handle_event(event),
                _ = expiry.tick() => self.expire_discovery(),
            }
        }
    }

    async fn drain(
        &mut self,
        listeners: Vec<ListenerId>,
        grace_period: std::time::Duration,
        expiry: &mut tokio::time::Interval,
    ) -> Result<(), NodeError> {
        self.status.begin_drain();
        for listener in listeners {
            self.swarm.remove_listener(listener);
        }
        let deadline = tokio::time::sleep(grace_period);
        tokio::pin!(deadline);
        loop {
            if self.swarm.connected_peers().next().is_none() {
                return Ok(());
            }
            tokio::select! {
                () = &mut deadline => return Ok(()),
                event = self.swarm.select_next_some() => self.handle_event(event),
                _ = expiry.tick() => self.expire_discovery(),
            }
        }
    }

    fn expire_discovery(&mut self) {
        self.status.touch();
        for peer in self.discovery.expire(std::time::Instant::now()) {
            self.emit(NodeEvent::DiscoveryUnregistered { peer });
        }
        self.status
            .set_discovery_registrations(self.discovery.len());
    }

    fn handle_event(&mut self, event: SwarmEvent<BehaviourEvent>) {
        self.status.touch();
        let event = match event {
            SwarmEvent::NewListenAddr {
                listener_id,
                address,
            } => {
                self.swarm.add_external_address(address.clone());
                self.ready_listeners.insert(listener_id);
                self.status.listeners_ready(self.ready_listeners.len());
                Some(NodeEvent::Listening { address })
            }
            SwarmEvent::ConnectionEstablished { .. } => {
                self.status.connection_opened();
                None
            }
            SwarmEvent::ConnectionClosed { .. } => {
                self.status.connection_closed();
                None
            }
            SwarmEvent::ListenerClosed { listener_id, .. } => {
                self.ready_listeners.remove(&listener_id);
                self.status.listeners_ready(self.ready_listeners.len());
                None
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
            SwarmEvent::Behaviour(BehaviourEvent::Relay(relay::Event::CircuitClosed {
                src_peer_id,
                dst_peer_id,
                ..
            })) => Some(NodeEvent::CircuitClosed {
                source: src_peer_id,
                destination: dst_peer_id,
            }),
            SwarmEvent::Behaviour(BehaviourEvent::Relay(
                relay::Event::ReservationClosed { src_peer_id }
                | relay::Event::ReservationTimedOut { src_peer_id },
            )) => Some(NodeEvent::ReservationClosed { peer: src_peer_id }),
            SwarmEvent::Behaviour(BehaviourEvent::Rendezvous(event)) => {
                self.handle_discovery_event(event)
            }
            _ => None,
        };
        if let Some(event) = event {
            self.emit(event);
        }
    }

    fn emit(&self, event: NodeEvent) {
        match &event {
            NodeEvent::ReservationAccepted { renewed, .. } => {
                self.status.reservation_accepted(*renewed);
            }
            NodeEvent::ReservationClosed { .. } => self.status.reservation_closed(),
            NodeEvent::ReservationDenied { .. } => self.status.reservation_denied(),
            NodeEvent::CircuitAccepted { .. } => self.status.circuit_accepted(),
            NodeEvent::CircuitClosed { .. } => self.status.circuit_closed(),
            NodeEvent::CircuitDenied { .. } => self.status.circuit_denied(),
            NodeEvent::DiscoveryRejected { .. } => self.status.discovery_rejected(),
            NodeEvent::Listening { .. }
            | NodeEvent::DiscoveryRegistered { .. }
            | NodeEvent::DiscoveryUnregistered { .. }
            | NodeEvent::DiscoveryServed { .. } => {}
        }
        if self.events.try_send(event).is_err() {
            self.status.event_dropped();
        }
    }

    fn handle_discovery_event(&mut self, event: rendezvous::ProtocolEvent) -> Option<NodeEvent> {
        let request_response::Event::Message {
            peer,
            message:
                request_response::Message::Request {
                    request, channel, ..
                },
            ..
        } = event
        else {
            return None;
        };
        self.status.discovery_request();
        for expired_peer in self.discovery.expire(std::time::Instant::now()) {
            self.emit(NodeEvent::DiscoveryUnregistered { peer: expired_peer });
        }
        let rendezvous::Handled { response, event } =
            self.discovery
                .handle(peer, request, std::time::Instant::now());
        if let Some(response) = response
            && self
                .swarm
                .behaviour_mut()
                .rendezvous
                .send_response(channel, response)
                .is_err()
        {
            return Some(NodeEvent::DiscoveryRejected { peer });
        }
        let event = match event {
            rendezvous::Event::Registered { peer } => NodeEvent::DiscoveryRegistered { peer },
            rendezvous::Event::Unregistered { peer } => NodeEvent::DiscoveryUnregistered { peer },
            rendezvous::Event::Served { peer, count } => NodeEvent::DiscoveryServed {
                peer,
                result_count: count,
            },
            rendezvous::Event::Rejected { peer } => NodeEvent::DiscoveryRejected { peer },
        };
        self.status
            .set_discovery_registrations(self.discovery.len());
        Some(event)
    }
}

fn build_behaviour(
    keypair: &libp2p::identity::Keypair,
    peer_id: PeerId,
    config: &NodeConfig,
    status: NodeStatus,
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
        admission: admission::Behaviour::new(config, status),
        relay: relay::Behaviour::new(peer_id, relay_config),
        rendezvous: rendezvous::behaviour(),
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
