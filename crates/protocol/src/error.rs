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

impl TryFrom<u8> for ProtocolErrorCode {
    type Error = WireError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::NotFoundOrUnauthorized),
            1 => Ok(Self::UnsupportedVersion),
            2 => Ok(Self::InvalidMessage),
            3 => Ok(Self::ShareUnavailable),
            4 => Ok(Self::ShareExpired),
            5 => Ok(Self::ShareAlreadyClaimed),
            6 => Ok(Self::ClaimMismatch),
            7 => Ok(Self::PayloadTooLarge),
            8 => Ok(Self::InternalFailure),
            9 => Ok(Self::TemporarilyUnavailable),
            _ => Err(WireError::InvalidMessage),
        }
    }
}

/// Secret-safe framing and CBOR failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum WireError {
    /// A frame did not include a complete four-byte header.
    #[error("incomplete frame header")]
    IncompleteHeader,
    /// A declared frame length was zero.
    #[error("empty frame")]
    EmptyFrame,
    /// A declared or encoded message exceeded its hard limit.
    #[error("frame exceeds the protocol limit")]
    FrameTooLarge,
    /// The declared body length did not match the available bytes.
    #[error("incomplete or trailing frame bytes")]
    LengthMismatch,
    /// CBOR was malformed, non-canonical, unknown, or contained trailing data.
    #[error("invalid protocol message")]
    InvalidMessage,
}
