//! Ciphertext digest binding the nonce and complete encrypted envelope.

use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::{AeadNonce, CiphertextDigest};

/// Computes `SHA-256(aead_nonce || ciphertext)`.
#[must_use]
pub fn ciphertext_digest(nonce: AeadNonce, ciphertext: &[u8]) -> CiphertextDigest {
    let bytes = Sha256::new()
        .chain_update(nonce.as_bytes())
        .chain_update(ciphertext)
        .finalize();
    let mut digest = [0_u8; 32];
    digest.copy_from_slice(&bytes);
    CiphertextDigest::new(digest)
}

/// Verifies the digest without data-dependent early exit.
#[must_use]
pub fn verify_ciphertext_digest(
    expected: CiphertextDigest,
    nonce: AeadNonce,
    ciphertext: &[u8],
) -> bool {
    bool::from(
        ciphertext_digest(nonce, ciphertext)
            .as_bytes()
            .ct_eq(expected.as_bytes()),
    )
}
