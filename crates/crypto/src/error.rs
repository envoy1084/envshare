//! Secret-safe cryptographic errors.

/// Failures at a cryptographic protocol boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CryptoError {
    /// The public network identifier violated the protocol constraints.
    #[error("invalid network identifier")]
    InvalidNetworkId,
    /// An authenticated peer identity was empty or too large for the transcript.
    #[error("invalid authenticated peer identity")]
    InvalidPeerIdentity,
    /// A canonical transcript exceeded its hard limit.
    #[error("authentication transcript is too large")]
    TranscriptTooLarge,
    /// HKDF could not produce an output of the required fixed size.
    #[error("key derivation failed")]
    KeyDerivation,
    /// The operating system could not generate a unique nonce.
    #[error("operating-system entropy is unavailable")]
    EntropyUnavailable,
    /// Application payload encryption failed.
    #[error("payload encryption failed")]
    Encryption,
    /// Authentication or payload decryption failed.
    #[error("payload authentication failed")]
    Decryption,
}
