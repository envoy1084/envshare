//! Bounded Circuit Relay v2 and discovery node services.

#![forbid(unsafe_code)]

mod admission;
mod config;
mod identity;
mod operations;
mod relay;
mod rendezvous;
mod telemetry;

pub use config::{LogFormat, NodeConfig, TelemetryConfig};
pub use identity::{generate_identity, load_identity, save_identity};
pub use operations::OperationsServer;
pub use relay::{NodeEvent, NodeServer};
pub use telemetry::NodeStatus;

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
    /// The loopback operations listener could not start or serve.
    #[error("node operations endpoint failed")]
    Operations,
}
