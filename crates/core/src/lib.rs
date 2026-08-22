//! Sender, receiver, and lifecycle services for Envshare.

#![forbid(unsafe_code)]

mod direct;
mod error;
mod local;
mod receiver;
mod sender;

pub use direct::{DirectReceiver, DirectSender, PendingDirectOffer};
pub use error::CoreError;
pub use local::{
    EnvironmentMode, ParsedEnvironment, PrivateOutputOptions, read_bounded, select_dotenv,
    spawn_child, wait_child_forwarding_interrupt, write_private_atomic,
};
pub use receiver::{ReceiverSession, VerifiedOffer};
pub use sender::{OfferEntropy, OsEntropy, SenderActor, SenderState};

/// Returns the wire protocol supported by this build.
#[must_use]
pub const fn protocol_version() -> u16 {
    protocol::PROTOCOL_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_protocol_crate_version() {
        assert_eq!(protocol_version(), protocol::PROTOCOL_VERSION);
    }
}
