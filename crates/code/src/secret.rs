//! Owned capability secret with redacted formatting and best-effort clearing.

use std::fmt;

use subtle::ConstantTimeEq;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::SECRET_BYTES;

/// The 160-bit bearer capability behind a share code.
///
/// This type is intentionally not `Clone`, `Copy`, `Display`, `Serialize`, or
/// `Deserialize`. Its destructor clears the owned byte array on a best-effort
/// basis.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct ShareCodeSecret([u8; SECRET_BYTES]);

impl ShareCodeSecret {
    /// Takes ownership of exactly 160 bits of capability material.
    #[must_use]
    pub const fn new(bytes: [u8; SECRET_BYTES]) -> Self {
        Self(bytes)
    }

    /// Exposes the secret only to narrow encoding and cryptographic boundaries.
    #[must_use]
    pub const fn expose_secret(&self) -> &[u8; SECRET_BYTES] {
        &self.0
    }

    /// Compares two capabilities without data-dependent early exit.
    #[must_use]
    pub fn ct_eq(&self, other: &Self) -> bool {
        bool::from(self.0.ct_eq(&other.0))
    }
}

impl fmt::Debug for ShareCodeSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ShareCodeSecret([REDACTED])")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_exposes_bytes() {
        let secret = ShareCodeSecret::new([0xAB; SECRET_BYTES]);
        let debug = format!("{secret:?}");
        assert_eq!(debug, "ShareCodeSecret([REDACTED])");
        assert!(!debug.contains("AB"));
    }
}
