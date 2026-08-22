//! Stable, secret-safe protocol errors.

/// Machine-readable error codes carried on the transfer protocol.
///
/// Variants intentionally carry no free-form context so secret-bearing values
/// cannot accidentally cross the protocol boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ProtocolErrorCode {
    /// The room or proof was invalid. This is deliberately indistinguishable.
    NotFoundOrUnauthorized = 0,
    /// The peer does not implement this wire version.
    UnsupportedVersion = 1,
    /// The frame or decoded message was invalid.
    InvalidMessage = 2,
    /// The share cannot currently be opened.
    ShareUnavailable = 3,
    /// The sender-side lifetime elapsed.
    ShareExpired = 4,
    /// Another valid claim already won.
    ShareAlreadyClaimed = 5,
    /// The request did not match the bound claim.
    ClaimMismatch = 6,
    /// The payload exceeded a hard protocol limit.
    PayloadTooLarge = 7,
    /// The peer failed internally without disclosing details.
    InternalFailure = 8,
    /// A bounded resource is temporarily saturated.
    TemporarilyUnavailable = 9,
}
