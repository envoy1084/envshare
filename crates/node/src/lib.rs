//! Bounded Circuit Relay v2 and discovery node services.

#![forbid(unsafe_code)]

mod admission;
mod config;
mod identity;
mod relay;
mod rendezvous;

pub use config::NodeConfig;
pub use identity::{generate_identity, load_identity, save_identity};
pub use relay::{NodeEvent, NodeServer};

/// Secret-safe node service failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum NodeError {
    /// Identity encoding, validation, or persistence failed.
    #[error("node identity operation failed")]
    Identity,
    /// Node bounds or transport configuration was invalid.
    #[error("node configuration is invalid")]
    Configuration,
    /// A listener could not be started.
    #[error("node listen operation failed")]
    Listen,
}
