//! Versioned wire protocol types and hard limits for Envshare.

#![forbid(unsafe_code)]

/// Current application protocol version.
pub const PROTOCOL_VERSION: u16 = 1;

/// Libp2p stream protocol identifier for Envshare transfer v1.
pub const TRANSFER_PROTOCOL: &str = "/envshare/transfer/1.0.0";

/// Maximum plaintext payload accepted by a v1 implementation.
pub const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
