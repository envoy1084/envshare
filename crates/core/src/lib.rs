//! Sender, receiver, and lifecycle services for Envshare.

#![forbid(unsafe_code)]

mod error;

pub use error::CoreError;

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
