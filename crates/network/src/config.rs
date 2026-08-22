//! Bounded client network configuration.

use std::time::Duration;

use crate::PrivacyMode;

/// Hard operational bounds for one client swarm.
#[derive(Clone, Debug)]
pub struct NetworkConfig {
    /// Capacity of the command queue into the swarm owner.
    pub command_capacity: usize,
    /// Capacity of the application event queue.
    pub event_capacity: usize,
    /// Timeout for a complete transfer request-response exchange.
    pub request_timeout: Duration,
    /// Maximum concurrent request-response streams.
    pub max_concurrent_streams: usize,
    /// Maximum total established connections.
    pub max_established_connections: u32,
    /// Maximum established connections to one peer.
    pub max_connections_per_peer: u32,
    /// Memory threshold after which new connections are denied.
    pub max_process_memory_bytes: usize,
    /// Maximum registrations accepted from one discovery response.
    pub max_discovery_results: usize,
    /// Enables local-network multicast discovery in standard privacy mode.
    pub enable_mdns: bool,
    /// Controls whether direct addresses may be exposed or dialed.
    pub privacy_mode: PrivacyMode,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            command_capacity: 32,
            event_capacity: 32,
            request_timeout: Duration::from_secs(15),
            max_concurrent_streams: 16,
            max_established_connections: 64,
            max_connections_per_peer: 4,
            max_process_memory_bytes: 512 * 1024 * 1024,
            max_discovery_results: 32,
            enable_mdns: false,
            privacy_mode: PrivacyMode::Standard,
        }
    }
}

impl NetworkConfig {
    pub(crate) fn validate(&self) -> bool {
        self.command_capacity > 0
            && self.event_capacity > 0
            && !self.request_timeout.is_zero()
            && self.max_concurrent_streams > 0
            && self.max_established_connections > 0
            && self.max_connections_per_peer > 0
            && self.max_connections_per_peer <= self.max_established_connections
            && self.max_process_memory_bytes >= 16 * 1024 * 1024
            && self.max_discovery_results > 0
            && self.max_discovery_results <= 256
    }
}
