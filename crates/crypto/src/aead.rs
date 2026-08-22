//! XChaCha20-Poly1305 application payload protection.

use chacha20poly1305::{
    Key, KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};

use crate::{AeadNonce, CryptoError, types::PayloadKey};

/// Generates a fresh 192-bit `XChaCha` nonce from the operating system.
///
/// # Errors
///
/// Returns [`CryptoError::EntropyUnavailable`] when secure randomness fails.
pub fn generate_aead_nonce() -> Result<AeadNonce, CryptoError> {
    let mut nonce = [0_u8; 24];
    getrandom::fill(&mut nonce).map_err(|_| CryptoError::EntropyUnavailable)?;
    Ok(AeadNonce::new(nonce))
}

/// Encrypts and authenticates one encoded envelope and its associated metadata.
///
/// # Errors
///
/// Returns [`CryptoError::Encryption`] if the AEAD implementation rejects the
/// input size or cannot produce ciphertext.
pub fn encrypt_payload(
    key: &PayloadKey,
    nonce: AeadNonce,
    plaintext: &[u8],
    associated_data: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = XChaCha20Poly1305::new(&Key::from(*key.expose()));
    cipher
        .encrypt(
            &XNonce::from(*nonce.as_bytes()),
            Payload {
                msg: plaintext,
                aad: associated_data,
            },
        )
        .map_err(|_| CryptoError::Encryption)
}

/// Authenticates and decrypts one encoded envelope.
///
/// # Errors
///
/// Returns the single [`CryptoError::Decryption`] classification for a wrong key,
/// nonce, associated data, tag, or corrupted ciphertext.
pub fn decrypt_payload(
    key: &PayloadKey,
    nonce: AeadNonce,
    ciphertext: &[u8],
    associated_data: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = XChaCha20Poly1305::new(&Key::from(*key.expose()));
    cipher
        .decrypt(
            &XNonce::from(*nonce.as_bytes()),
            Payload {
                msg: ciphertext,
                aad: associated_data,
            },
        )
        .map_err(|_| CryptoError::Decryption)
}
