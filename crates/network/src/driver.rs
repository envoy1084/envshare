//! Bounded command API and single-owner swarm loop.

use std::{collections::HashMap, time::Duration};

use futures::StreamExt;
use libp2p::{
    Multiaddr, PeerId, Swarm, SwarmBuilder,
    request_response::{self, OutboundRequestId, ResponseChannel},
    swarm::{SwarmEvent, dial_opts::DialOpts},
};
use protocol::{
    PROTOCOL_VERSION, ProtocolErrorCode, ProtocolErrorResponse, TransferRequest, TransferResponse,
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::{
    DiscoveredPeer, DiscoveryNamespace, DiscoveryProvider, NetworkConfig, NetworkError,
    behaviour::{Behaviour, BehaviourEvent},
};

/// Opaque token for replying to one inbound transfer request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct InboundRequestId(u64);

/// Application-visible events emitted by the network owner.
#[derive(Debug)]
pub enum NetworkEvent {
    /// A transport began listening on a concrete address.
    Listening {
        /// Bound transport address.
        address: Multiaddr,
    },
    /// A remote peer opened the Envshare transfer protocol.
    InboundRequest {
        /// Authenticated remote libp2p identity.
        peer: PeerId,
        /// Token used to send exactly one response.
        request_id: InboundRequestId,
        /// Strictly decoded transfer request.
        request: TransferRequest,
    },
    /// A peer connection became established.
    Connected {
        /// Authenticated remote libp2p identity.
        peer: PeerId,
    },
    /// A peer connection closed.
    Disconnected {
        /// Authenticated remote libp2p identity.
        peer: PeerId,
    },
    /// A relay server accepted or renewed this client's reservation.
    RelayReservation {
        /// Relay server identity.
        relay_peer: PeerId,
        /// True when an existing reservation was renewed.
        renewal: bool,
    },
    /// A circuit to a remote destination was established through a relay.
    OutboundRelayCircuit {
        /// Relay server identity.
        relay_peer: PeerId,
    },
    /// A remote source reached this client through a relay reservation.
    InboundRelayCircuit {
        /// Authenticated remote source identity.
        source_peer: PeerId,
    },
    /// Local registration was accepted by a Rendezvous node.
    DiscoveryRegistered {
        /// Rendezvous node identity.
        node: PeerId,
        /// Effective registration lifetime.
        ttl_seconds: u64,
    },
    /// A bounded candidate page was returned by a Rendezvous node.
    DiscoveryResults {
        /// Rendezvous node identity.
        node: PeerId,
        /// Signed candidate records truncated to the configured bound.
        peers: Vec<DiscoveredPeer>,
    },
    /// A registration or discovery request failed safely.
    DiscoveryFailed {
        /// Rendezvous node identity.
        node: PeerId,
    },
    /// A previously discovered peer record reached its advertised expiry.
    DiscoveryExpired {
        /// Authenticated peer whose cached record expired.
        peer: PeerId,
    },
}

enum Command {
    Listen {
        address: Multiaddr,
        result: oneshot::Sender<Result<(), NetworkError>>,
    },
    Dial {
        peer: PeerId,
        address: Multiaddr,
        result: oneshot::Sender<Result<(), NetworkError>>,
    },
    Request {
        peer: PeerId,
        address: Multiaddr,
        request: TransferRequest,
        result: oneshot::Sender<Result<TransferResponse, NetworkError>>,
    },
    Respond {
        request_id: InboundRequestId,
        response: TransferResponse,
        result: oneshot::Sender<Result<(), NetworkError>>,
    },
    Discovery(DiscoveryCommand),
}

enum DiscoveryCommand {
    AddAddress {
        address: Multiaddr,
        result: oneshot::Sender<Result<(), NetworkError>>,
    },
    Register {
        node: PeerId,
        node_address: Multiaddr,
        namespace: DiscoveryNamespace,
        ttl_seconds: u64,
        result: oneshot::Sender<Result<(), NetworkError>>,
    },
    Discover {
        node: PeerId,
        node_address: Multiaddr,
        namespace: DiscoveryNamespace,
        result: oneshot::Sender<Result<(), NetworkError>>,
    },
    Unregister {
        node: PeerId,
        namespace: DiscoveryNamespace,
        result: oneshot::Sender<Result<(), NetworkError>>,
    },
}

/// Cloneable bounded handle into the swarm owner task.
#[derive(Clone, Debug)]
pub struct NetworkClient {
    local_peer_id: PeerId,
    commands: mpsc::Sender<Command>,
}

impl NetworkClient {
    /// Returns the local authenticated libp2p identity.
    #[must_use]
    pub const fn local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }

    /// Starts listening on one explicit transport address.
    ///
    /// # Errors
    ///
    /// Returns a bounded API or listen error.
    pub async fn listen(&self, address: Multiaddr) -> Result<(), NetworkError> {
        let (result, receiver) = oneshot::channel();
        self.send(Command::Listen { address, result }).await?;
        receiver.await.map_err(|_| NetworkError::TaskStopped)?
    }

    /// Starts dialing an authenticated peer at an explicit address.
    ///
    /// # Errors
    ///
    /// Returns if the command cannot be queued or the dial cannot start.
    pub async fn dial(&self, peer: PeerId, address: Multiaddr) -> Result<(), NetworkError> {
        let (result, receiver) = oneshot::channel();
        self.send(Command::Dial {
            peer,
            address,
            result,
        })
        .await?;
        receiver.await.map_err(|_| NetworkError::TaskStopped)?
    }

    /// Sends one transfer request to an explicit peer and address.
    ///
    /// # Errors
    ///
    /// Returns a secret-safe queue, dial, timeout, codec, or transport failure.
    pub async fn request(
        &self,
        peer: PeerId,
        address: Multiaddr,
        request: TransferRequest,
    ) -> Result<TransferResponse, NetworkError> {
        let (result, receiver) = oneshot::channel();
        self.send(Command::Request {
            peer,
            address,
            request,
            result,
        })
        .await?;
        receiver.await.map_err(|_| NetworkError::TaskStopped)?
    }

    /// Delivers exactly one response to an inbound request token.
    ///
    /// # Errors
    ///
    /// Returns if the token is stale or response delivery fails.
    pub async fn respond(
        &self,
        request_id: InboundRequestId,
        response: TransferResponse,
    ) -> Result<(), NetworkError> {
        let (result, receiver) = oneshot::channel();
        self.send(Command::Respond {
            request_id,
            response,
            result,
        })
        .await?;
        receiver.await.map_err(|_| NetworkError::TaskStopped)?
    }

    async fn send(&self, command: Command) -> Result<(), NetworkError> {
        self.commands
            .send(command)
            .await
            .map_err(|_| NetworkError::TaskStopped)
    }
}

#[async_trait::async_trait]
impl DiscoveryProvider for NetworkClient {
    async fn add_discovery_address(&self, address: Multiaddr) -> Result<(), NetworkError> {
        let (result, receiver) = oneshot::channel();
        self.send(Command::Discovery(DiscoveryCommand::AddAddress {
            address,
            result,
        }))
        .await?;
        receiver.await.map_err(|_| NetworkError::TaskStopped)?
    }

    async fn register(
        &self,
        node: PeerId,
        node_address: Multiaddr,
        namespace: DiscoveryNamespace,
        ttl_seconds: u64,
    ) -> Result<(), NetworkError> {
        let (result, receiver) = oneshot::channel();
        self.send(Command::Discovery(DiscoveryCommand::Register {
            node,
            node_address,
            namespace,
            ttl_seconds,
            result,
        }))
        .await?;
        receiver.await.map_err(|_| NetworkError::TaskStopped)?
    }

    async fn discover(
        &self,
        node: PeerId,
        node_address: Multiaddr,
        namespace: DiscoveryNamespace,
    ) -> Result<(), NetworkError> {
        let (result, receiver) = oneshot::channel();
        self.send(Command::Discovery(DiscoveryCommand::Discover {
            node,
            node_address,
            namespace,
            result,
        }))
        .await?;
        receiver.await.map_err(|_| NetworkError::TaskStopped)?
    }

    async fn unregister(
        &self,
        node: PeerId,
        namespace: DiscoveryNamespace,
    ) -> Result<(), NetworkError> {
        let (result, receiver) = oneshot::channel();
        self.send(Command::Discovery(DiscoveryCommand::Unregister {
            node,
            namespace,
            result,
        }))
        .await?;
        receiver.await.map_err(|_| NetworkError::TaskStopped)?
    }
}

/// Tokio-owned swarm driver. Exactly one task must call [`Self::run`].
pub struct NetworkDriver {
    swarm: Swarm<Behaviour>,
    commands: mpsc::Receiver<Command>,
    events: mpsc::Sender<NetworkEvent>,
    outbound: HashMap<OutboundRequestId, oneshot::Sender<Result<TransferResponse, NetworkError>>>,
    inbound: HashMap<InboundRequestId, ResponseChannel<TransferResponse>>,
    inbound_protocol_ids: HashMap<request_response::InboundRequestId, InboundRequestId>,
    next_inbound_id: u64,
    max_discovery_results: usize,
}

impl NetworkDriver {
    /// Builds TCP+Noise+Yamux and QUIC transports with bounded behaviors.
    ///
    /// # Errors
    ///
    /// Returns a configuration error if bounds or transport construction fail.
    pub fn new(
        keypair: libp2p::identity::Keypair,
        config: &NetworkConfig,
    ) -> Result<(NetworkClient, mpsc::Receiver<NetworkEvent>, Self), NetworkError> {
        if !config.validate() {
            return Err(NetworkError::Configuration);
        }
        let command_capacity = config.command_capacity;
        let event_capacity = config.event_capacity;
        let max_streams = config.max_concurrent_streams;
        let swarm = SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                libp2p::tcp::Config::default().nodelay(true),
                libp2p::noise::Config::new,
                libp2p::yamux::Config::default,
            )
            .map_err(|_| NetworkError::Configuration)?
            .with_quic()
            .with_relay_client(libp2p::noise::Config::new, libp2p::yamux::Config::default)
            .map_err(|_| NetworkError::Configuration)?
            .with_behaviour(|keypair, relay| Behaviour::new(keypair, config, relay))
            .map_err(|_| NetworkError::Configuration)?
            .with_swarm_config(|swarm_config| {
                swarm_config
                    .with_idle_connection_timeout(Duration::from_secs(30))
                    .with_max_negotiating_inbound_streams(max_streams)
            })
            .build();
        let local_peer_id = *swarm.local_peer_id();
        let (command_sender, commands) = mpsc::channel(command_capacity);
        let (events, event_receiver) = mpsc::channel(event_capacity);
        let client = NetworkClient {
            local_peer_id,
            commands: command_sender,
        };
        Ok((
            client,
            event_receiver,
            Self {
                swarm,
                commands,
                events,
                outbound: HashMap::new(),
                inbound: HashMap::new(),
                inbound_protocol_ids: HashMap::new(),
                next_inbound_id: 1,
                max_discovery_results: config.max_discovery_results,
            },
        ))
    }

    /// Drives commands, transports, and protocols until cancellation or all
    /// client handles are dropped.
    pub async fn run(mut self, cancellation: CancellationToken) {
        loop {
            tokio::select! {
                () = cancellation.cancelled() => break,
                command = self.commands.recv() => {
                    let Some(command) = command else { break };
                    self.handle_command(command);
                }
                event = self.swarm.select_next_some() => self.handle_swarm_event(event),
            }
        }
        self.outbound.clear();
        self.inbound.clear();
        self.inbound_protocol_ids.clear();
    }

    fn handle_command(&mut self, command: Command) {
        match command {
            Command::Listen { address, result } => {
                let outcome = self
                    .swarm
                    .listen_on(address)
                    .map(|_| ())
                    .map_err(|_| NetworkError::Listen);
                let _ = result.send(outcome);
            }
            Command::Dial {
                peer,
                address,
                result,
            } => {
                let options = DialOpts::peer_id(peer).addresses(vec![address]).build();
                let outcome = self.swarm.dial(options).map_err(|_| NetworkError::Dial);
                let _ = result.send(outcome);
            }
            Command::Request {
                peer,
                address,
                request,
                result,
            } => {
                self.swarm.add_peer_address(peer, address);
                let request_id = self
                    .swarm
                    .behaviour_mut()
                    .transfer
                    .send_request(&peer, request);
                self.outbound.insert(request_id, result);
            }
            Command::Respond {
                request_id,
                response,
                result,
            } => {
                let outcome = self
                    .inbound
                    .remove(&request_id)
                    .ok_or(NetworkError::Response)
                    .and_then(|channel| {
                        self.swarm
                            .behaviour_mut()
                            .transfer
                            .send_response(channel, response)
                            .map_err(|_| NetworkError::Response)
                    });
                self.inbound_protocol_ids
                    .retain(|_, application_id| *application_id != request_id);
                let _ = result.send(outcome);
            }
            Command::Discovery(command) => self.handle_discovery_command(command),
        }
    }

    fn handle_discovery_command(&mut self, command: DiscoveryCommand) {
        match command {
            DiscoveryCommand::AddAddress { address, result } => {
                self.swarm.add_external_address(address);
                let _ = result.send(Ok(()));
            }
            DiscoveryCommand::Register {
                node,
                node_address,
                namespace,
                ttl_seconds,
                result,
            } => {
                self.swarm.add_peer_address(node, node_address);
                let outcome = if ttl_seconds == 0 || ttl_seconds > 86_400 {
                    Err(NetworkError::Configuration)
                } else {
                    namespace.to_rendezvous().and_then(|namespace| {
                        self.swarm
                            .behaviour_mut()
                            .rendezvous
                            .register(namespace, node, Some(ttl_seconds))
                            .map_err(|_| NetworkError::Configuration)
                    })
                };
                let _ = result.send(outcome);
            }
            DiscoveryCommand::Discover {
                node,
                node_address,
                namespace,
                result,
            } => {
                self.swarm.add_peer_address(node, node_address);
                let outcome = namespace.to_rendezvous().map(|namespace| {
                    self.swarm.behaviour_mut().rendezvous.discover(
                        Some(namespace),
                        None,
                        u64::try_from(self.max_discovery_results).ok(),
                        node,
                    );
                });
                let _ = result.send(outcome);
            }
            DiscoveryCommand::Unregister {
                node,
                namespace,
                result,
            } => {
                let outcome = namespace.to_rendezvous().map(|namespace| {
                    self.swarm
                        .behaviour_mut()
                        .rendezvous
                        .unregister(namespace, node);
                });
                let _ = result.send(outcome);
            }
        }
    }

    fn handle_swarm_event(&mut self, event: SwarmEvent<BehaviourEvent>) {
        match event {
            SwarmEvent::Behaviour(BehaviourEvent::Transfer(event)) => {
                self.handle_transfer_event(event);
            }
            SwarmEvent::Behaviour(BehaviourEvent::Relay(event)) => {
                self.handle_relay_event(&event);
            }
            SwarmEvent::Behaviour(BehaviourEvent::Rendezvous(event)) => {
                self.handle_rendezvous_event(event);
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                let _ = self.events.try_send(NetworkEvent::Listening { address });
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                let _ = self
                    .events
                    .try_send(NetworkEvent::Connected { peer: peer_id });
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                let _ = self
                    .events
                    .try_send(NetworkEvent::Disconnected { peer: peer_id });
            }
            _ => {}
        }
    }

    fn handle_transfer_event(
        &mut self,
        event: request_response::Event<TransferRequest, TransferResponse>,
    ) {
        match event {
            request_response::Event::Message { peer, message, .. } => match message {
                request_response::Message::Request {
                    request_id,
                    request,
                    channel,
                } => self.handle_inbound_request(peer, request_id, request, channel),
                request_response::Message::Response {
                    request_id,
                    response,
                } => {
                    if let Some(result) = self.outbound.remove(&request_id) {
                        let _ = result.send(Ok(response));
                    }
                }
            },
            request_response::Event::OutboundFailure { request_id, .. } => {
                if let Some(result) = self.outbound.remove(&request_id) {
                    let _ = result.send(Err(NetworkError::Request));
                }
            }
            request_response::Event::InboundFailure { request_id, .. }
            | request_response::Event::ResponseSent { request_id, .. } => {
                if let Some(application_id) = self.inbound_protocol_ids.remove(&request_id) {
                    self.inbound.remove(&application_id);
                }
            }
        }
    }

    fn handle_relay_event(&mut self, event: &libp2p::relay::client::Event) {
        let event = match event {
            libp2p::relay::client::Event::ReservationReqAccepted {
                relay_peer_id,
                renewal,
                ..
            } => NetworkEvent::RelayReservation {
                relay_peer: *relay_peer_id,
                renewal: *renewal,
            },
            libp2p::relay::client::Event::OutboundCircuitEstablished { relay_peer_id, .. } => {
                NetworkEvent::OutboundRelayCircuit {
                    relay_peer: *relay_peer_id,
                }
            }
            libp2p::relay::client::Event::InboundCircuitEstablished { src_peer_id, .. } => {
                NetworkEvent::InboundRelayCircuit {
                    source_peer: *src_peer_id,
                }
            }
        };
        let _ = self.events.try_send(event);
    }

    fn handle_rendezvous_event(&mut self, event: libp2p::rendezvous::client::Event) {
        let event = match event {
            libp2p::rendezvous::client::Event::Registered {
                rendezvous_node,
                ttl,
                ..
            } => NetworkEvent::DiscoveryRegistered {
                node: rendezvous_node,
                ttl_seconds: ttl,
            },
            libp2p::rendezvous::client::Event::Discovered {
                rendezvous_node,
                registrations,
                ..
            } => {
                let peers = registrations
                    .into_iter()
                    .take(self.max_discovery_results)
                    .map(|registration| DiscoveredPeer {
                        peer: registration.record.peer_id(),
                        addresses: registration.record.addresses().to_vec(),
                    })
                    .collect();
                NetworkEvent::DiscoveryResults {
                    node: rendezvous_node,
                    peers,
                }
            }
            libp2p::rendezvous::client::Event::DiscoverFailed {
                rendezvous_node, ..
            }
            | libp2p::rendezvous::client::Event::RegisterFailed {
                rendezvous_node, ..
            } => NetworkEvent::DiscoveryFailed {
                node: rendezvous_node,
            },
            libp2p::rendezvous::client::Event::Expired { peer } => {
                NetworkEvent::DiscoveryExpired { peer }
            }
        };
        let _ = self.events.try_send(event);
    }

    fn handle_inbound_request(
        &mut self,
        peer: PeerId,
        protocol_id: request_response::InboundRequestId,
        request: TransferRequest,
        channel: ResponseChannel<TransferResponse>,
    ) {
        let request_id = InboundRequestId(self.next_inbound_id);
        self.next_inbound_id = self.next_inbound_id.wrapping_add(1).max(1);
        self.inbound.insert(request_id, channel);
        self.inbound_protocol_ids.insert(protocol_id, request_id);
        let event = NetworkEvent::InboundRequest {
            peer,
            request_id,
            request,
        };
        if self.events.try_send(event).is_err()
            && let Some(channel) = self.inbound.remove(&request_id)
        {
            self.inbound_protocol_ids.remove(&protocol_id);
            let response = TransferResponse::Error(ProtocolErrorResponse {
                protocol_version: PROTOCOL_VERSION,
                code: ProtocolErrorCode::TemporarilyUnavailable,
            });
            let _ = self
                .swarm
                .behaviour_mut()
                .transfer
                .send_response(channel, response);
        }
    }
}

impl std::fmt::Debug for NetworkDriver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NetworkDriver")
            .field("local_peer_id", self.swarm.local_peer_id())
            .field("pending_outbound", &self.outbound.len())
            .field("pending_inbound", &self.inbound.len())
            .finish_non_exhaustive()
    }
}
