//! Secret-safe domain errors.

/// Errors returned across Envshare service boundaries.
///
/// These variants contain only bounded, non-secret classifications. Binaries may
/// attach operational context, but must never attach capabilities or payloads.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// The local capability representation was invalid.
    #[error("invalid share code")]
    InvalidCode,
    /// Discovery or authentication did not locate an authorized share.
    #[error("share not found or unauthorized")]
    NotFoundOrUnauthorized,
    /// The share expired, was claimed, or otherwise cannot be used.
    #[error("share is unavailable")]
    ShareUnavailable,
    /// Discovery, dialing, or transport failed.
    #[error("network operation failed")]
    Network,
    /// Authentication, framing, or decryption failed.
    #[error("secure transfer failed")]
    Transfer,
    /// Receiver output could not be safely completed.
    #[error("output operation failed")]
    Output,
    /// Local configuration was invalid.
    #[error("configuration is invalid")]
    Configuration,
    /// A non-secret internal invariant failed.
    #[error("internal software error")]
    Internal,
}
