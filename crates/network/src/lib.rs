//! Libp2p networking and discovery for Envshare clients.

#![forbid(unsafe_code)]

mod behaviour;
mod codec;
mod config;
mod discovery;
mod driver;
mod error;

pub use config::NetworkConfig;
pub use discovery::{
    CandidatePolicy, CandidateSet, DiscoveredPeer, DiscoveryNamespace, DiscoveryNode,
    DiscoveryProvider, PrivacyMode, RouteCandidate, RouteKind, dispatch_discovery,
    dispatch_registration, dispatch_unregistration, maintain_registrations, renewal_delay,
};
pub use driver::{InboundRequestId, NetworkClient, NetworkDriver, NetworkEvent};
pub use error::NetworkError;
pub use libp2p::identity;
pub use libp2p::multiaddr::Protocol as MultiaddrProtocol;
pub use libp2p::{Multiaddr, PeerId};
