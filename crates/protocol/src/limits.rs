//! Absolute v1 limits applied before allocation or resource admission.

/// Maximum plaintext payload accepted by a v1 implementation.
pub const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
/// Maximum encoded Open request body.
pub const MAX_OPEN_BODY_BYTES: usize = 1024;
/// Maximum encoded acknowledgement request body.
pub const MAX_ACK_BODY_BYTES: usize = 1024;
/// Maximum encoded Completed response body.
pub const MAX_COMPLETED_BODY_BYTES: usize = 1024;
/// Maximum encoded protocol error body.
pub const MAX_ERROR_BODY_BYTES: usize = 1024;
/// Metadata and authentication allowance above the plaintext payload limit.
pub const MAX_OFFER_OVERHEAD_BYTES: usize = 16 * 1024;
/// Maximum encoded Offer response body.
pub const MAX_OFFER_BODY_BYTES: usize = MAX_PAYLOAD_BYTES + MAX_OFFER_OVERHEAD_BYTES;
/// Maximum encoded plaintext envelope before AEAD encryption.
pub const MAX_ENVELOPE_BYTES: usize = MAX_PAYLOAD_BYTES + 1024;
/// Maximum encrypted envelope including the Poly1305 tag.
pub const MAX_CIPHERTEXT_BYTES: usize = MAX_ENVELOPE_BYTES + 16;
/// Maximum canonical authentication transcript.
pub const MAX_TRANSCRIPT_BYTES: usize = 8 * 1024;
/// Maximum configured network identifier length.
pub const MAX_NETWORK_ID_BYTES: usize = 64;
/// Maximum suggested filename metadata length.
pub const MAX_SUGGESTED_NAME_BYTES: usize = 128;
