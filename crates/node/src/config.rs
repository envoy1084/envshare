//! Validated relay service bounds.

use std::time::Duration;

use libp2p::Multiaddr;

/// Absolute safety ceilings for one relay node.
#[derive(Clone, Debug)]
pub struct NodeConfig {
    /// Transport addresses to bind.
    pub listen_addresses: Vec<Multiaddr>,
    /// Maximum simultaneous reservations.
    pub max_reservations: usize,
    /// Maximum reservations owned by one peer.
    pub max_reservations_per_peer: usize,
    /// Reservation lifetime advertised to clients.
    pub reservation_duration: Duration,
    /// Maximum simultaneous relay circuits.
    pub max_circuits: usize,
    /// Maximum circuits associated with one peer.
    pub max_circuits_per_peer: usize,
    /// Hard duration for one circuit.
    pub max_circuit_duration: Duration,
    /// Hard byte count for one relayed circuit.
    pub max_circuit_bytes: u64,
    /// Maximum established transport connections.
    pub max_connections: u32,
    /// Maximum transport connections for one peer.
    pub max_connections_per_peer: u32,
    /// Process memory threshold for rejecting new connections.
    pub max_process_memory_bytes: usize,
    /// Capacity of the safe operational event stream.
    pub event_capacity: usize,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            listen_addresses: vec![
                "/ip4/0.0.0.0/tcp/4001"
                    .parse()
                    .unwrap_or_else(|_| Multiaddr::empty()),
                "/ip4/0.0.0.0/udp/4001/quic-v1"
                    .parse()
                    .unwrap_or_else(|_| Multiaddr::empty()),
            ],
            max_reservations: 128,
            max_reservations_per_peer: 2,
            reservation_duration: Duration::from_hours(1),
            max_circuits: 64,
            max_circuits_per_peer: 4,
            max_circuit_duration: Duration::from_mins(2),
            max_circuit_bytes: 2 * 1024 * 1024,
            max_connections: 512,
            max_connections_per_peer: 8,
            max_process_memory_bytes: 1024 * 1024 * 1024,
            event_capacity: 256,
        }
    }
}

impl NodeConfig {
    pub(crate) fn validate(&self) -> bool {
        !self.listen_addresses.is_empty()
            && self
                .listen_addresses
                .iter()
                .all(|address| !address.is_empty())
            && self.max_reservations > 0
            && self.max_reservations_per_peer > 0
            && self.max_reservations_per_peer <= self.max_reservations
            && !self.reservation_duration.is_zero()
            && self.max_circuits > 0
            && self.max_circuits_per_peer > 0
            && self.max_circuits_per_peer <= self.max_circuits
            && !self.max_circuit_duration.is_zero()
            && self.max_circuit_bytes >= 1024
            && self.max_connections > 0
            && self.max_connections_per_peer > 0
            && self.max_connections_per_peer <= self.max_connections
            && self.max_process_memory_bytes >= 64 * 1024 * 1024
            && self.event_capacity > 0
    }
}
