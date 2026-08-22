//! Sender, receiver, and lifecycle services for Envshare.

#![forbid(unsafe_code)]

/// Returns the wire protocol supported by this build.
#[must_use]
pub const fn protocol_version() -> u16 {
    envshare_protocol::PROTOCOL_VERSION
}
