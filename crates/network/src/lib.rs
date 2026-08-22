//! Libp2p networking and discovery for Envshare clients.

#![forbid(unsafe_code)]

mod behaviour;
mod codec;
mod config;
mod discovery;
mod driver;
mod error;

pub use config::NetworkConfig;
pub use discovery::{DiscoveredPeer, DiscoveryNamespace, DiscoveryProvider};
pub use driver::{InboundRequestId, NetworkClient, NetworkDriver, NetworkEvent};
pub use error::NetworkError;
pub use libp2p::identity;
pub use libp2p::{Multiaddr, PeerId};
