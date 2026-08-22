//! In-memory discovery state with hard admission and cardinality bounds.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    net::{Ipv4Addr, Ipv6Addr},
    time::{Duration, Instant},
};

use libp2p::{Multiaddr, PeerId, core::PeerRecord, multiaddr::Protocol};

use crate::NodeConfig;

use super::codec::{DiscoveryPage, Request, Response, Status, WireRegistration};

const WINDOW: Duration = Duration::from_mins(1);
const NAMESPACE_PREFIX: &str = "envshare-v1-";
const ROOM_HEX_BYTES: usize = 32;

pub(crate) struct Store {
    config: Limits,
    registrations: HashMap<(PeerId, String), Registration>,
    cookies: HashMap<Vec<u8>, CookieState>,
    cookie_order: VecDeque<Vec<u8>>,
    rates: HashMap<PeerId, RateState>,
    next_registration: u64,
    next_cookie: u64,
}

pub(crate) struct Handled {
    pub response: Option<Response>,
    pub event: Event,
}

pub(crate) enum Event {
    Registered { peer: PeerId },
    Unregistered { peer: PeerId },
    Served { peer: PeerId, count: usize },
    Rejected { peer: PeerId },
}

struct Limits {
    min_ttl: u64,
    max_ttl: u64,
    per_peer: usize,
    total: usize,
    per_namespace: usize,
    cookies: usize,
    addresses: usize,
    record_bytes: usize,
    results: usize,
    register_rate: u32,
    discover_rate: u32,
    rate_peers: usize,
    allow_private: bool,
}

struct Registration {
    id: u64,
    signed_record: Vec<u8>,
    ttl: u64,
    expires_at: Instant,
}

struct CookieState {
    namespace: String,
    seen: HashSet<u64>,
}

struct RateState {
    window_started: Instant,
    registrations: u32,
    discoveries: u32,
}

#[derive(Clone, Copy)]
enum Operation {
    Register,
    Unregister,
    Discover,
}

impl Store {
    pub(crate) fn len(&self) -> usize {
        self.registrations.len()
    }

    pub fn new(config: &NodeConfig) -> Self {
        Self {
            config: Limits {
                min_ttl: config.discovery_min_ttl_seconds,
                max_ttl: config.discovery_max_ttl_seconds,
                per_peer: config.discovery_registrations_per_peer,
                total: config.discovery_registrations_total,
                per_namespace: config.discovery_registrations_per_namespace,
                cookies: config.discovery_cookies,
                addresses: config.discovery_addresses_per_registration,
                record_bytes: config.discovery_record_bytes,
                results: config.discovery_results,
                register_rate: config.discovery_register_requests_per_minute,
                discover_rate: config.discovery_discover_requests_per_minute,
                rate_peers: config.discovery_rate_limit_peers,
                allow_private: config.discovery_allow_private_addresses,
            },
            registrations: HashMap::new(),
            cookies: HashMap::new(),
            cookie_order: VecDeque::new(),
            rates: HashMap::new(),
            next_registration: 1,
            next_cookie: 1,
        }
    }

    pub fn handle(&mut self, peer: PeerId, request: Request, now: Instant) -> Handled {
        let operation = match request {
            Request::Discover { .. } => Operation::Discover,
            Request::Register { .. } => Operation::Register,
            Request::Unregister { .. } => Operation::Unregister,
        };
        if !self.admit(peer, operation, now) {
            return rejected(peer, response_for_rate_limited(operation));
        }
        match request {
            Request::Register {
                namespace,
                signed_record,
                ttl,
            } => self.register(peer, &namespace, signed_record, ttl, now),
            Request::Unregister { namespace } => self.unregister(peer, namespace),
            Request::Discover {
                namespace,
                cookie,
                limit,
            } => self.discover(peer, namespace, cookie, limit),
        }
    }

    pub fn expire(&mut self, now: Instant) -> Vec<PeerId> {
        let expired = self
            .registrations
            .iter()
            .filter_map(|(key, registration)| {
                (registration.expires_at <= now).then_some(key.clone())
            })
            .collect::<Vec<_>>();
        let mut peers = Vec::with_capacity(expired.len());
        for key in expired {
            if let Some(registration) = self.registrations.remove(&key) {
                self.remove_from_cookies(registration.id);
                peers.push(key.0);
            }
        }
        peers
    }

    fn register(
        &mut self,
        peer: PeerId,
        namespace: &str,
        signed_record: Vec<u8>,
        ttl: Option<u64>,
        now: Instant,
    ) -> Handled {
        if !valid_namespace(namespace) {
            return rejected(
                peer,
                Some(Response::Register(Err(Status::InvalidNamespace))),
            );
        }
        let ttl = ttl.unwrap_or(self.config.max_ttl);
        if !(self.config.min_ttl..=self.config.max_ttl).contains(&ttl) {
            return rejected(peer, Some(Response::Register(Err(Status::InvalidTtl))));
        }
        match self.validate_record(peer, &signed_record) {
            Ok(()) => {}
            Err(status) => return rejected(peer, Some(Response::Register(Err(status)))),
        }
        let key = (peer, namespace.to_owned());
        let replacing = self.registrations.contains_key(&key);
        if !replacing
            && (self.registrations.len() >= self.config.total
                || self
                    .registrations
                    .keys()
                    .filter(|(registered_peer, _)| registered_peer == &peer)
                    .count()
                    >= self.config.per_peer
                || self
                    .registrations
                    .keys()
                    .filter(|(_, registered_namespace)| registered_namespace == namespace)
                    .count()
                    >= self.config.per_namespace)
        {
            return rejected(peer, Some(Response::Register(Err(Status::Unavailable))));
        }
        if let Some(previous) = self.registrations.remove(&key) {
            self.remove_from_cookies(previous.id);
        }
        let id = self.next_registration;
        self.next_registration = self.next_registration.wrapping_add(1).max(1);
        self.registrations.insert(
            key,
            Registration {
                id,
                signed_record,
                ttl,
                expires_at: now + Duration::from_secs(ttl),
            },
        );
        Handled {
            response: Some(Response::Register(Ok(ttl))),
            event: Event::Registered { peer },
        }
    }

    fn unregister(&mut self, peer: PeerId, namespace: String) -> Handled {
        if !valid_namespace(&namespace) {
            return rejected(peer, None);
        }
        if let Some(registration) = self.registrations.remove(&(peer, namespace)) {
            self.remove_from_cookies(registration.id);
        }
        Handled {
            response: None,
            event: Event::Unregistered { peer },
        }
    }

    fn discover(
        &mut self,
        peer: PeerId,
        namespace: Option<String>,
        cookie: Option<Vec<u8>>,
        limit: Option<u64>,
    ) -> Handled {
        let Some(namespace) = namespace.filter(|namespace| valid_namespace(namespace)) else {
            return rejected(
                peer,
                Some(Response::Discover(Err(Status::InvalidNamespace))),
            );
        };
        let mut seen = match cookie {
            Some(cookie) => match self.cookies.get(&cookie) {
                Some(state) if state.namespace == namespace => state.seen.clone(),
                _ => {
                    return rejected(peer, Some(Response::Discover(Err(Status::InvalidCookie))));
                }
            },
            None => HashSet::new(),
        };
        let requested = limit
            .and_then(|limit| usize::try_from(limit).ok())
            .unwrap_or(self.config.results)
            .min(self.config.results);
        let mut registrations = self
            .registrations
            .iter()
            .filter(|((_, registered_namespace), registration)| {
                registered_namespace == &namespace && !seen.contains(&registration.id)
            })
            .map(|((_, registered_namespace), registration)| {
                (
                    registration.id,
                    WireRegistration {
                        namespace: registered_namespace.clone(),
                        signed_record: registration.signed_record.clone(),
                        ttl: registration.ttl,
                    },
                )
            })
            .collect::<Vec<_>>();
        registrations.sort_by_key(|(id, _)| *id);
        registrations.truncate(requested);
        seen.extend(registrations.iter().map(|(id, _)| *id));
        let registrations = registrations
            .into_iter()
            .map(|(_, registration)| registration)
            .collect::<Vec<_>>();
        let cookie = self.insert_cookie(namespace, seen);
        let count = registrations.len();
        Handled {
            response: Some(Response::Discover(Ok(DiscoveryPage {
                registrations,
                cookie,
            }))),
            event: Event::Served { peer, count },
        }
    }

    fn validate_record(&self, peer: PeerId, bytes: &[u8]) -> Result<(), Status> {
        if bytes.len() > self.config.record_bytes {
            return Err(Status::InvalidSignedPeerRecord);
        }
        let envelope = libp2p::core::SignedEnvelope::from_protobuf_encoding(bytes)
            .map_err(|_| Status::InvalidSignedPeerRecord)?;
        let record = PeerRecord::from_signed_envelope(envelope)
            .map_err(|_| Status::InvalidSignedPeerRecord)?;
        if record.peer_id() != peer {
            return Err(Status::NotAuthorized);
        }
        if record.addresses().is_empty()
            || record.addresses().len() > self.config.addresses
            || !record
                .addresses()
                .iter()
                .all(|address| valid_address(address, peer, self.config.allow_private))
        {
            return Err(Status::InvalidSignedPeerRecord);
        }
        Ok(())
    }

    fn admit(&mut self, peer: PeerId, operation: Operation, now: Instant) -> bool {
        self.rates
            .retain(|_, state| now.saturating_duration_since(state.window_started) < WINDOW);
        if !self.rates.contains_key(&peer) && self.rates.len() >= self.config.rate_peers {
            return false;
        }
        let state = self.rates.entry(peer).or_insert(RateState {
            window_started: now,
            registrations: 0,
            discoveries: 0,
        });
        match operation {
            Operation::Register | Operation::Unregister
                if state.registrations < self.config.register_rate =>
            {
                state.registrations += 1;
                true
            }
            Operation::Discover if state.discoveries < self.config.discover_rate => {
                state.discoveries += 1;
                true
            }
            Operation::Register | Operation::Unregister | Operation::Discover => false,
        }
    }

    fn insert_cookie(&mut self, namespace: String, seen: HashSet<u64>) -> Vec<u8> {
        while self.cookies.len() >= self.config.cookies {
            if let Some(oldest) = self.cookie_order.pop_front() {
                self.cookies.remove(&oldest);
            } else {
                break;
            }
        }
        let id = self.next_cookie;
        self.next_cookie = self.next_cookie.wrapping_add(1).max(1);
        let mut cookie = Vec::with_capacity(8 + namespace.len());
        cookie.extend_from_slice(&id.to_be_bytes());
        cookie.extend_from_slice(namespace.as_bytes());
        self.cookies
            .insert(cookie.clone(), CookieState { namespace, seen });
        self.cookie_order.push_back(cookie.clone());
        cookie
    }

    fn remove_from_cookies(&mut self, registration_id: u64) {
        self.cookies.retain(|_, state| {
            state.seen.remove(&registration_id);
            !state.seen.is_empty()
        });
        self.cookie_order
            .retain(|cookie| self.cookies.contains_key(cookie));
    }
}

fn valid_namespace(namespace: &str) -> bool {
    namespace.len() == NAMESPACE_PREFIX.len() + ROOM_HEX_BYTES
        && namespace.starts_with(NAMESPACE_PREFIX)
        && namespace[NAMESPACE_PREFIX.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_address(address: &Multiaddr, peer: PeerId, allow_private: bool) -> bool {
    let protocols = address.iter().collect::<Vec<_>>();
    if !protocols.iter().any(|protocol| {
        matches!(
            protocol,
            Protocol::Tcp(_) | Protocol::Quic | Protocol::QuicV1 | Protocol::P2pCircuit
        )
    }) {
        return false;
    }
    if !allow_private
        && protocols.iter().any(|protocol| match protocol {
            Protocol::Ip4(address) => unusable_v4(*address),
            Protocol::Ip6(address) => unusable_v6(*address),
            _ => false,
        })
    {
        return false;
    }
    let circuit = protocols
        .iter()
        .position(|protocol| matches!(protocol, Protocol::P2pCircuit));
    match circuit {
        Some(index) => protocols[index + 1..]
            .iter()
            .find_map(|protocol| match protocol {
                Protocol::P2p(route_peer) => Some(route_peer == &peer),
                _ => None,
            })
            .unwrap_or(true),
        None => protocols
            .iter()
            .rev()
            .find_map(|protocol| match protocol {
                Protocol::P2p(route_peer) => Some(route_peer == &peer),
                _ => None,
            })
            .unwrap_or(true),
    }
}

fn unusable_v4(address: Ipv4Addr) -> bool {
    address.is_private()
        || address.is_loopback()
        || address.is_link_local()
        || address.is_unspecified()
        || address.is_multicast()
        || address.is_documentation()
}

fn unusable_v6(address: Ipv6Addr) -> bool {
    address.is_unique_local()
        || address.is_loopback()
        || address.is_unicast_link_local()
        || address.is_unspecified()
        || address.is_multicast()
}

fn response_for_rate_limited(operation: Operation) -> Option<Response> {
    match operation {
        Operation::Register => Some(Response::Register(Err(Status::Unavailable))),
        Operation::Unregister => None,
        Operation::Discover => Some(Response::Discover(Err(Status::Unavailable))),
    }
}

fn rejected(peer: PeerId, response: Option<Response>) -> Handled {
    Handled {
        response,
        event: Event::Rejected { peer },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn namespace(byte: char) -> String {
        format!("{NAMESPACE_PREFIX}{}", byte.to_string().repeat(32))
    }

    fn registration(
        keypair: &libp2p::identity::Keypair,
        namespace: String,
        addresses: Vec<Multiaddr>,
    ) -> Result<Request, Box<dyn std::error::Error>> {
        let record = PeerRecord::new(keypair, addresses)?;
        Ok(Request::Register {
            namespace,
            signed_record: record.into_signed_envelope().into_protobuf_encoding(),
            ttl: Some(30),
        })
    }

    #[test]
    fn namespaces_and_public_addresses_are_strictly_validated()
    -> Result<(), Box<dyn std::error::Error>> {
        assert!(valid_namespace(&format!(
            "{NAMESPACE_PREFIX}{}",
            "a".repeat(32)
        )));
        assert!(!valid_namespace("anything"));
        let peer = PeerId::random();
        assert!(valid_address(
            &"/ip4/203.0.113.20/tcp/4001".parse()?,
            peer,
            true
        ));
        assert!(!valid_address(
            &"/ip4/127.0.0.1/tcp/4001".parse()?,
            peer,
            false
        ));
        Ok(())
    }

    #[test]
    fn per_namespace_and_rate_maps_are_bounded() -> Result<(), Box<dyn std::error::Error>> {
        let config = NodeConfig {
            discovery_registrations_total: 2,
            discovery_registrations_per_peer: 1,
            discovery_registrations_per_namespace: 1,
            discovery_rate_limit_peers: 2,
            discovery_register_requests_per_minute: 1,
            discovery_allow_private_addresses: true,
            ..NodeConfig::default()
        };
        let mut store = Store::new(&config);
        let first = libp2p::identity::Keypair::generate_ed25519();
        let second = libp2p::identity::Keypair::generate_ed25519();
        let third = libp2p::identity::Keypair::generate_ed25519();
        let address: Multiaddr = "/ip4/127.0.0.1/tcp/4001".parse()?;
        let now = Instant::now();

        let accepted = store.handle(
            first.public().to_peer_id(),
            registration(&first, namespace('a'), vec![address.clone()])?,
            now,
        );
        assert!(matches!(
            accepted.response,
            Some(Response::Register(Ok(30)))
        ));

        let same_peer_over_rate = store.handle(
            first.public().to_peer_id(),
            registration(&first, namespace('b'), vec![address.clone()])?,
            now,
        );
        assert!(matches!(
            same_peer_over_rate.response,
            Some(Response::Register(Err(Status::Unavailable)))
        ));
        let namespace_full = store.handle(
            second.public().to_peer_id(),
            registration(&second, namespace('a'), vec![address])?,
            now,
        );
        assert!(matches!(
            namespace_full.response,
            Some(Response::Register(Err(Status::Unavailable)))
        ));
        let untracked_peer = store.handle(
            third.public().to_peer_id(),
            registration(
                &third,
                namespace('c'),
                vec!["/ip4/127.0.0.1/tcp/4001".parse()?],
            )?,
            now,
        );
        assert!(matches!(
            untracked_peer.response,
            Some(Response::Register(Err(Status::Unavailable)))
        ));
        assert_eq!(store.rates.len(), 2);
        assert_eq!(store.registrations.len(), 1);
        Ok(())
    }

    #[test]
    fn private_records_and_cookie_growth_are_bounded() -> Result<(), Box<dyn std::error::Error>> {
        let mut config = NodeConfig {
            discovery_cookies: 1,
            ..NodeConfig::default()
        };
        let mut store = Store::new(&config);
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let peer = keypair.public().to_peer_id();
        let rejected = store.handle(
            peer,
            registration(
                &keypair,
                namespace('a'),
                vec!["/ip4/127.0.0.1/tcp/4001".parse()?],
            )?,
            Instant::now(),
        );
        assert!(matches!(
            rejected.response,
            Some(Response::Register(Err(Status::InvalidSignedPeerRecord)))
        ));

        config.discovery_allow_private_addresses = true;
        let mut store = Store::new(&config);
        for _ in 0..2 {
            let handled = store.handle(
                peer,
                Request::Discover {
                    namespace: Some(namespace('a')),
                    cookie: None,
                    limit: None,
                },
                Instant::now(),
            );
            assert!(matches!(handled.response, Some(Response::Discover(Ok(_)))));
        }
        assert_eq!(store.cookies.len(), 1);
        assert_eq!(store.cookie_order.len(), 1);
        Ok(())
    }

    #[test]
    fn namespace_address_and_record_limits_reject_before_storage()
    -> Result<(), Box<dyn std::error::Error>> {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let peer = keypair.public().to_peer_id();
        let mut config = NodeConfig {
            discovery_allow_private_addresses: true,
            discovery_addresses_per_registration: 1,
            ..NodeConfig::default()
        };
        let mut store = Store::new(&config);
        let malformed_namespace = store.handle(
            peer,
            Request::Register {
                namespace: "untrusted".to_owned(),
                signed_record: vec![0; 64],
                ttl: Some(30),
            },
            Instant::now(),
        );
        assert!(matches!(
            malformed_namespace.response,
            Some(Response::Register(Err(Status::InvalidNamespace)))
        ));
        let too_many_addresses = store.handle(
            peer,
            registration(
                &keypair,
                namespace('a'),
                vec![
                    "/ip4/127.0.0.1/tcp/4001".parse()?,
                    "/ip4/127.0.0.1/tcp/4002".parse()?,
                ],
            )?,
            Instant::now(),
        );
        assert!(matches!(
            too_many_addresses.response,
            Some(Response::Register(Err(Status::InvalidSignedPeerRecord)))
        ));

        config.discovery_record_bytes = 1;
        let mut store = Store::new(&config);
        let oversized_record = store.handle(
            peer,
            registration(
                &keypair,
                namespace('b'),
                vec!["/ip4/127.0.0.1/tcp/4001".parse()?],
            )?,
            Instant::now(),
        );
        assert!(matches!(
            oversized_record.response,
            Some(Response::Register(Err(Status::InvalidSignedPeerRecord)))
        ));
        assert!(store.registrations.is_empty());
        Ok(())
    }
}
