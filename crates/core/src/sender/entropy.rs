//! Injectable cryptographic entropy for offer construction.

use crate::CoreError;

/// Entropy source used to generate all unique values in a winning offer.
pub trait OfferEntropy {
    /// Fills the complete destination with cryptographically secure random bytes.
    ///
    /// # Errors
    ///
    /// Returns a secret-safe error if entropy is unavailable.
    fn fill(&mut self, destination: &mut [u8]) -> Result<(), CoreError>;
}

/// Operating-system cryptographic entropy.
#[derive(Clone, Copy, Debug, Default)]
pub struct OsEntropy;

impl OfferEntropy for OsEntropy {
    fn fill(&mut self, destination: &mut [u8]) -> Result<(), CoreError> {
        getrandom::fill(destination).map_err(|_| CoreError::Internal)
    }
}
