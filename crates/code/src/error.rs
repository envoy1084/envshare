//! Secret-safe capability errors.

/// Failure to obtain capability entropy from the operating system.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum GenerateCodeError {
    /// The operating system did not provide cryptographically secure randomness.
    #[error("operating-system entropy is unavailable")]
    EntropyUnavailable,
}

/// Generic capability parsing failure.
///
/// All malformed representations use the same variant so callers cannot echo or
/// classify attacker-controlled capability fragments.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ParseCodeError {
    /// The representation, length, version, alphabet, or checksum was invalid.
    #[error("invalid share code")]
    Invalid,
}
