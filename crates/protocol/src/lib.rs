//! Versioned wire protocol types and hard limits for Envshare.

#![forbid(unsafe_code)]

mod envelope;
mod error;
mod frame;
mod limits;
mod message;

pub use envelope::{ContentType, EnvelopeError, SecretEnvelope, SuggestedName};
pub use error::{ProtocolErrorCode, WireError};
pub use frame::{
    decode_request_frame, decode_response_frame, encode_request_frame, encode_response_frame,
    parse_frame_length,
};
pub use limits::*;
pub use message::{
    AcknowledgeRequest, CompletedResponse, OfferResponse, OpenRequest, ProtocolErrorResponse,
    TransferRequest, TransferResponse,
};

/// Current application protocol version.
pub const PROTOCOL_VERSION: u16 = 1;

/// Libp2p stream protocol identifier for Envshare transfer v1.
pub const TRANSFER_PROTOCOL: &str = "/envshare/transfer/1.0.0";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_protocol_and_version_agree() {
        assert_eq!(PROTOCOL_VERSION, 1);
        assert_eq!(TRANSFER_PROTOCOL, "/envshare/transfer/1.0.0");
    }

    #[test]
    fn payload_limit_is_one_mebibyte() {
        assert_eq!(MAX_PAYLOAD_BYTES, 1_048_576);
    }
}
