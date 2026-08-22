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
    /// Minimum accepted discovery registration lifetime.
    pub discovery_min_ttl_seconds: u64,
    /// Maximum accepted discovery registration lifetime.
    pub discovery_max_ttl_seconds: u64,
    /// Maximum discovery registrations owned by one peer.
    pub discovery_registrations_per_peer: usize,
    /// Absolute registration and maximum response-result bound.
    pub discovery_registrations_total: usize,
    /// Maximum simultaneous registrations in one opaque namespace.
    pub discovery_registrations_per_namespace: usize,
    /// Maximum incremental-discovery cookies retained in memory.
    pub discovery_cookies: usize,
    /// Maximum addresses accepted in one signed peer record.
    pub discovery_addresses_per_registration: usize,
    /// Maximum encoded signed peer-record size.
    pub discovery_record_bytes: usize,
    /// Maximum registrations returned by one discovery request.
    pub discovery_results: usize,
    /// Maximum register/unregister requests accepted per peer each minute.
    pub discovery_register_requests_per_minute: u32,
    /// Maximum discover requests accepted per peer each minute.
    pub discovery_discover_requests_per_minute: u32,
    /// Maximum peer rate buckets retained in memory.
    pub discovery_rate_limit_peers: usize,
    /// Admit private, loopback, and link-local registrations for private nodes.
    pub discovery_allow_private_addresses: bool,
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
            discovery_min_ttl_seconds: 30,
            discovery_max_ttl_seconds: 300,
            discovery_registrations_per_peer: 8,
            discovery_registrations_total: 256,
            discovery_registrations_per_namespace: 32,
            discovery_cookies: 512,
            discovery_addresses_per_registration: 8,
            discovery_record_bytes: 16 * 1024,
            discovery_results: 32,
            discovery_register_requests_per_minute: 12,
            discovery_discover_requests_per_minute: 30,
            discovery_rate_limit_peers: 1_024,
            discovery_allow_private_addresses: false,
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
            && self.discovery_min_ttl_seconds > 0
            && self.discovery_min_ttl_seconds <= self.discovery_max_ttl_seconds
            && self.discovery_max_ttl_seconds <= 86_400
            && self.discovery_registrations_per_peer > 0
            && self.discovery_registrations_per_peer <= self.discovery_registrations_total
            && self.discovery_registrations_total <= 4_096
            && self.discovery_registrations_per_namespace > 0
            && self.discovery_registrations_per_namespace <= self.discovery_registrations_total
            && self.discovery_cookies > 0
            && self.discovery_cookies <= 8_192
            && self.discovery_addresses_per_registration > 0
            && self.discovery_addresses_per_registration <= 16
            && (512..=16 * 1024).contains(&self.discovery_record_bytes)
            && self.discovery_results > 0
            && self.discovery_results <= 64
            && self.discovery_results <= self.discovery_registrations_total
            && self
                .discovery_results
                .checked_mul(self.discovery_record_bytes)
                .is_some_and(|bytes| bytes <= 900 * 1024)
            && self.discovery_register_requests_per_minute > 0
            && self.discovery_register_requests_per_minute <= 120
            && self.discovery_discover_requests_per_minute > 0
            && self.discovery_discover_requests_per_minute <= 240
            && self.discovery_rate_limit_peers > 0
            && self.discovery_rate_limit_peers <= 4_096
    }
}
