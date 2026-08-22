//! Opaque bounded discovery interface.

use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
    time::Duration,
};

use async_trait::async_trait;
use futures::{StreamExt, stream};
use libp2p::multiaddr::Protocol;
use tokio_util::sync::CancellationToken;

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

/// One configured member of the federated discovery set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryNode {
    /// Authenticated node identity expected after the Noise handshake.
    pub peer: PeerId,
    /// Explicit bounded route to the node.
    pub address: Multiaddr,
}

impl FromStr for DiscoveryNode {
    type Err = NetworkError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut address = input
            .parse::<Multiaddr>()
            .map_err(|_| NetworkError::Configuration)?;
        let Some(Protocol::P2p(peer)) = address.pop() else {
            return Err(NetworkError::Configuration);
        };
        if address.is_empty() {
            return Err(NetworkError::Configuration);
        }
        Ok(Self { peer, address })
    }
}

/// Network exposure policy applied before candidate dialing.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PrivacyMode {
    /// Permit direct and relay routes, including explicitly enabled LAN routes.
    #[default]
    Standard,
    /// Never advertise, discover, or dial a direct address.
    RelayOnly,
}

/// Classification used to rank validated candidate routes.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RouteKind {
    /// Public QUIC route, preferred for low handshake latency.
    PublicQuic,
    /// Public TCP route.
    PublicTcp,
    /// Private or link-local route admitted for LAN discovery.
    Lan,
    /// Circuit Relay v2 route.
    Relay,
}

/// One validated, deduplicated route to an authenticated candidate peer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteCandidate {
    /// Expected remote Noise identity.
    pub peer: PeerId,
    /// Validated transport route.
    pub address: Multiaddr,
    /// Route preference class.
    pub kind: RouteKind,
}

/// Hard bounds and privacy rules for untrusted discovery results.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CandidatePolicy {
    /// Maximum distinct candidate identities retained.
    pub max_peers: usize,
    /// Maximum routes retained for one identity.
    pub max_addresses_per_peer: usize,
    /// Whether private/link-local addresses may be dialed.
    pub allow_lan: bool,
    /// Direct versus relay-only exposure policy.
    pub privacy: PrivacyMode,
}

impl Default for CandidatePolicy {
    fn default() -> Self {
        Self {
            max_peers: 32,
            max_addresses_per_peer: 8,
            allow_lan: false,
            privacy: PrivacyMode::Standard,
        }
    }
}

/// Bounded accumulator for results returned by mutually untrusted nodes.
pub struct CandidateSet {
    policy: CandidatePolicy,
    routes: HashMap<PeerId, Vec<RouteCandidate>>,
    seen: HashSet<(PeerId, Multiaddr)>,
}

impl CandidateSet {
    /// Creates an empty candidate set if every requested bound is safe.
    ///
    /// # Errors
    ///
    /// Returns a configuration error for zero or excessive bounds.
    pub fn new(policy: CandidatePolicy) -> Result<Self, NetworkError> {
        if policy.max_peers == 0
            || policy.max_peers > 256
            || policy.max_addresses_per_peer == 0
            || policy.max_addresses_per_peer > 16
        {
            return Err(NetworkError::Configuration);
        }
        Ok(Self {
            policy,
            routes: HashMap::new(),
            seen: HashSet::new(),
        })
    }

    /// Validates and merges one signed peer record without exceeding bounds.
    pub fn insert(&mut self, discovered: DiscoveredPeer) {
        if !self.routes.contains_key(&discovered.peer) && self.routes.len() == self.policy.max_peers
        {
            return;
        }
        for address in discovered
            .addresses
            .into_iter()
            .take(self.policy.max_addresses_per_peer)
        {
            if self
                .routes
                .get(&discovered.peer)
                .is_some_and(|routes| routes.len() == self.policy.max_addresses_per_peer)
            {
                break;
            }
            let Some(kind) = classify_address(&address, discovered.peer, self.policy) else {
                continue;
            };
            if !self.seen.insert((discovered.peer, address.clone())) {
                continue;
            }
            self.routes
                .entry(discovered.peer)
                .or_default()
                .push(RouteCandidate {
                    peer: discovered.peer,
                    address,
                    kind,
                });
        }
    }

    /// Consumes the set into deterministic best-first dialing order.
    #[must_use]
    pub fn into_ranked(mut self) -> Vec<RouteCandidate> {
        let mut routes = self
            .routes
            .drain()
            .flat_map(|(_, routes)| routes)
            .collect::<Vec<_>>();
        routes.sort_by(|left, right| {
            left.kind
                .cmp(&right.kind)
                .then_with(|| left.peer.to_bytes().cmp(&right.peer.to_bytes()))
                .then_with(|| left.address.to_string().cmp(&right.address.to_string()))
        });
        routes
    }
}

impl std::fmt::Debug for CandidateSet {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CandidateSet")
            .field("peer_count", &self.routes.len())
            .field("route_count", &self.seen.len())
            .finish_non_exhaustive()
    }
}

/// Dispatches registration requests to a bounded federated node set in parallel.
pub async fn dispatch_registration<P: DiscoveryProvider + Sync>(
    provider: &P,
    nodes: &[DiscoveryNode],
    namespace: &DiscoveryNamespace,
    ttl_seconds: u64,
    max_parallel: usize,
) -> Vec<(PeerId, Result<(), NetworkError>)> {
    if max_parallel == 0 || max_parallel > 8 {
        return Vec::new();
    }
    stream::iter(nodes.iter().take(8).cloned())
        .map(|node| async move {
            (
                node.peer,
                provider
                    .register(
                        node.peer,
                        node.address.clone(),
                        namespace.clone(),
                        ttl_seconds,
                    )
                    .await,
            )
        })
        .buffer_unordered(max_parallel)
        .collect()
        .await
}

/// Dispatches bounded lookups to a federated node set in parallel.
pub async fn dispatch_discovery<P: DiscoveryProvider + Sync>(
    provider: &P,
    nodes: &[DiscoveryNode],
    namespace: &DiscoveryNamespace,
    max_parallel: usize,
) -> Vec<(PeerId, Result<(), NetworkError>)> {
    if max_parallel == 0 || max_parallel > 8 {
        return Vec::new();
    }
    stream::iter(nodes.iter().take(8).cloned())
        .map(|node| async move {
            (
                node.peer,
                provider
                    .discover(node.peer, node.address.clone(), namespace.clone())
                    .await,
            )
        })
        .buffer_unordered(max_parallel)
        .collect()
        .await
}

/// Dispatches best-effort unregister requests to every configured node.
pub async fn dispatch_unregistration<P: DiscoveryProvider + Sync>(
    provider: &P,
    nodes: &[DiscoveryNode],
    namespace: &DiscoveryNamespace,
    max_parallel: usize,
) {
    if max_parallel == 0 || max_parallel > 8 {
        return;
    }
    stream::iter(nodes.iter().take(8).cloned())
        .for_each_concurrent(max_parallel, |node| async move {
            let _ = provider.unregister(node.peer, namespace.clone()).await;
        })
        .await;
}

/// Maintains bounded registrations until cancellation, then unregisters.
pub async fn maintain_registrations<P: DiscoveryProvider + Sync>(
    provider: &P,
    nodes: &[DiscoveryNode],
    namespace: &DiscoveryNamespace,
    ttl_seconds: u64,
    max_parallel: usize,
    cancellation: CancellationToken,
) {
    loop {
        let _ = dispatch_registration(provider, nodes, namespace, ttl_seconds, max_parallel).await;
        tokio::select! {
            () = cancellation.cancelled() => break,
            () = tokio::time::sleep(renewal_delay(ttl_seconds)) => {}
        }
    }
    dispatch_unregistration(provider, nodes, namespace, max_parallel).await;
}

/// Returns the deterministic two-thirds-TTL registration renewal point.
#[must_use]
pub fn renewal_delay(ttl_seconds: u64) -> Duration {
    Duration::from_secs(
        ttl_seconds
            .saturating_mul(2)
            .checked_div(3)
            .unwrap_or(0)
            .max(1),
    )
}

fn classify_address(
    address: &Multiaddr,
    expected_peer: PeerId,
    policy: CandidatePolicy,
) -> Option<RouteKind> {
    let protocols = address.iter().collect::<Vec<_>>();
    if protocols.is_empty()
        || protocols.iter().any(|protocol| {
            matches!(
                protocol,
                Protocol::Memory(_)
                    | Protocol::Unix(_)
                    | Protocol::Onion(_, _)
                    | Protocol::Onion3(_)
            )
        })
    {
        return None;
    }
    let relay = protocols
        .iter()
        .any(|protocol| matches!(protocol, Protocol::P2pCircuit));
    if relay {
        let circuit_index = protocols
            .iter()
            .position(|protocol| matches!(protocol, Protocol::P2pCircuit))?;
        if let Some(Protocol::P2p(peer)) = protocols[circuit_index + 1..]
            .iter()
            .find(|protocol| matches!(protocol, Protocol::P2p(_)))
            && peer != &expected_peer
        {
            return None;
        }
        return Some(RouteKind::Relay);
    }
    if let Some(Protocol::P2p(peer)) = protocols
        .iter()
        .rev()
        .find(|protocol| matches!(protocol, Protocol::P2p(_)))
        && peer != &expected_peer
    {
        return None;
    }
    if policy.privacy == PrivacyMode::RelayOnly {
        return None;
    }
    let private = protocols.iter().any(|protocol| match protocol {
        Protocol::Ip4(address) => {
            address.is_private()
                || address.is_loopback()
                || address.is_link_local()
                || address.is_unspecified()
                || address.is_multicast()
        }
        Protocol::Ip6(address) => {
            address.is_unique_local()
                || address.is_loopback()
                || address.is_unicast_link_local()
                || address.is_unspecified()
                || address.is_multicast()
        }
        _ => false,
    });
    if private {
        return policy.allow_lan.then_some(RouteKind::Lan);
    }
    if protocols
        .iter()
        .any(|protocol| matches!(protocol, Protocol::QuicV1 | Protocol::Quic))
    {
        Some(RouteKind::PublicQuic)
    } else if protocols
        .iter()
        .any(|protocol| matches!(protocol, Protocol::Tcp(_)))
    {
        Some(RouteKind::PublicTcp)
    } else {
        None
    }
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

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    #[test]
    fn namespace_debug_never_exposes_room_identifier() {
        let namespace = DiscoveryNamespace::from_room_id([0xab; 16]);
        assert_eq!(format!("{namespace:?}"), "DiscoveryNamespace([REDACTED])");
        assert!(!format!("{namespace:?}").contains("abab"));
    }

    #[test]
    fn candidate_set_rejects_malformed_routes_and_ranks_deterministically()
    -> Result<(), Box<dyn Error>> {
        let peer = PeerId::random();
        let wrong_peer = PeerId::random();
        let relay_peer = PeerId::random();
        let relay: Multiaddr =
            format!("/ip4/1.1.1.1/tcp/4001/p2p/{relay_peer}/p2p-circuit").parse()?;
        let quic: Multiaddr = "/ip4/8.8.8.8/udp/4001/quic-v1".parse()?;
        let tcp: Multiaddr = "/ip4/8.8.4.4/tcp/4001".parse()?;
        let wrong_identity: Multiaddr = format!("/ip4/9.9.9.9/tcp/1/p2p/{wrong_peer}").parse()?;
        let mut candidates = CandidateSet::new(CandidatePolicy::default())?;
        candidates.insert(DiscoveredPeer {
            peer,
            addresses: vec![
                relay,
                tcp.clone(),
                quic.clone(),
                quic,
                wrong_identity,
                "/memory/9".parse()?,
                "/ip4/127.0.0.1/tcp/2".parse()?,
            ],
        });
        let routes = candidates.into_ranked();
        assert_eq!(routes.len(), 3);
        assert_eq!(routes[0].kind, RouteKind::PublicQuic);
        assert_eq!(routes[1].kind, RouteKind::PublicTcp);
        assert_eq!(routes[2].kind, RouteKind::Relay);
        Ok(())
    }

    #[test]
    fn relay_only_policy_filters_every_direct_route() -> Result<(), Box<dyn Error>> {
        let peer = PeerId::random();
        let relay_peer = PeerId::random();
        let mut candidates = CandidateSet::new(CandidatePolicy {
            privacy: PrivacyMode::RelayOnly,
            allow_lan: true,
            ..CandidatePolicy::default()
        })?;
        candidates.insert(DiscoveredPeer {
            peer,
            addresses: vec![
                "/ip4/8.8.8.8/tcp/1".parse()?,
                "/ip4/127.0.0.1/tcp/1".parse()?,
                format!("/ip4/1.1.1.1/tcp/1/p2p/{relay_peer}/p2p-circuit").parse()?,
            ],
        });
        let routes = candidates.into_ranked();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].kind, RouteKind::Relay);
        Ok(())
    }

    #[test]
    fn candidate_and_renewal_bounds_are_enforced() {
        assert!(
            CandidateSet::new(CandidatePolicy {
                max_peers: 0,
                ..CandidatePolicy::default()
            })
            .is_err()
        );
        assert!(
            CandidateSet::new(CandidatePolicy {
                max_addresses_per_peer: 17,
                ..CandidatePolicy::default()
            })
            .is_err()
        );
        assert_eq!(renewal_delay(30), Duration::from_secs(20));
        assert_eq!(renewal_delay(0), Duration::from_secs(1));
    }

    #[tokio::test]
    async fn registration_maintenance_unregisters_on_cancellation() -> Result<(), Box<dyn Error>> {
        let provider = CountingProvider::default();
        let observed = provider.clone();
        let cancellation = CancellationToken::new();
        let stop = cancellation.clone();
        let namespace = DiscoveryNamespace::from_room_id([1; 16]);
        let nodes = vec![DiscoveryNode {
            peer: PeerId::random(),
            address: "/ip4/127.0.0.1/tcp/1".parse()?,
        }];
        let task = tokio::spawn(async move {
            maintain_registrations(&provider, &nodes, &namespace, 30, 1, stop).await;
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while observed.registered.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        cancellation.cancel();
        task.await?;
        assert_eq!(observed.registered.load(Ordering::SeqCst), 1);
        assert_eq!(observed.unregistered.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[derive(Clone, Default)]
    struct CountingProvider {
        registered: Arc<AtomicUsize>,
        unregistered: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl DiscoveryProvider for CountingProvider {
        async fn add_discovery_address(&self, _: Multiaddr) -> Result<(), NetworkError> {
            Ok(())
        }

        async fn register(
            &self,
            _: PeerId,
            _: Multiaddr,
            _: DiscoveryNamespace,
            _: u64,
        ) -> Result<(), NetworkError> {
            self.registered.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn discover(
            &self,
            _: PeerId,
            _: Multiaddr,
            _: DiscoveryNamespace,
        ) -> Result<(), NetworkError> {
            Ok(())
        }

        async fn unregister(&self, _: PeerId, _: DiscoveryNamespace) -> Result<(), NetworkError> {
            self.unregistered.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
}
