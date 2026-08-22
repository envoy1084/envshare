//! Opaque bounded discovery interface.

use async_trait::async_trait;

use crate::{Multiaddr, NetworkError, PeerId};

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Validated opaque Rendezvous namespace derived from a capability room ID.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct DiscoveryNamespace(String);

impl DiscoveryNamespace {
    /// Constructs the fixed v1 namespace for a capability-derived room.
    #[must_use]
    pub fn from_room_id(room_id: [u8; 16]) -> Self {
        let mut namespace = String::with_capacity(44);
        namespace.push_str("envshare-v1-");
        for byte in room_id {
            namespace.push(char::from(HEX[usize::from(byte >> 4)]));
            namespace.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        Self(namespace)
    }

    pub(crate) fn to_rendezvous(&self) -> Result<libp2p::rendezvous::Namespace, NetworkError> {
        libp2p::rendezvous::Namespace::new(self.0.clone()).map_err(|_| NetworkError::Configuration)
    }
}

impl std::fmt::Debug for DiscoveryNamespace {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("DiscoveryNamespace([REDACTED])")
    }
}

/// One bounded discovery candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredPeer {
    /// Authenticated peer identity from its signed peer record.
    pub peer: PeerId,
    /// Signed candidate addresses, truncated to the configured result bound.
    pub addresses: Vec<Multiaddr>,
}

/// Discovery operations independent of the command-line or sender lifecycle.
#[async_trait]
pub trait DiscoveryProvider {
    /// Adds one reachable local address used in signed registrations.
    async fn add_discovery_address(&self, address: Multiaddr) -> Result<(), NetworkError>;

    /// Registers the local peer under one opaque room namespace.
    async fn register(
        &self,
        node: PeerId,
        node_address: Multiaddr,
        namespace: DiscoveryNamespace,
        ttl_seconds: u64,
    ) -> Result<(), NetworkError>;

    /// Requests a bounded candidate page for one opaque room namespace.
    async fn discover(
        &self,
        node: PeerId,
        node_address: Multiaddr,
        namespace: DiscoveryNamespace,
    ) -> Result<(), NetworkError>;

    /// Removes a local registration best-effort.
    async fn unregister(
        &self,
        node: PeerId,
        namespace: DiscoveryNamespace,
    ) -> Result<(), NetworkError>;
}
