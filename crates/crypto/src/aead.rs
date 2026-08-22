//! XChaCha20-Poly1305 application payload protection.

use chacha20poly1305::{
    Key, KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};

use crate::{
    AeadNonce, CryptoError, Transcript,
    kdf::validate_context,
    types::{PayloadAadInput, PayloadKey},
};

const PAYLOAD_AAD_DOMAIN: &[u8] = b"envshare/payload-aad/v1";

/// Builds canonical associated data binding envelope metadata to both peers.
///
/// # Errors
///
/// Returns a typed context or transcript error for non-canonical input.
pub fn payload_associated_data(input: PayloadAadInput<'_>) -> Result<Vec<u8>, CryptoError> {
    validate_context(input.context)?;
    let mut transcript = Transcript::new(PAYLOAD_AAD_DOMAIN)?;
    transcript.append_u16(protocol::PROTOCOL_VERSION)?;
    transcript.append_bytes(input.context.network_id.as_bytes())?;
    transcript.append_bytes(input.context.room_id.as_bytes())?;
    transcript.append_bytes(input.context.sender_peer_id)?;
    transcript.append_bytes(input.context.receiver_peer_id)?;
    transcript.append_bytes(input.receiver_nonce.as_bytes())?;
    transcript.append_bytes(input.sender_nonce.as_bytes())?;
    transcript.append_bytes(input.claim_id.as_bytes())?;
    transcript.append_u64(input.expires_at_unix_ms)?;
    transcript.append_u16(u16::from(input.content_type))?;
    transcript.append_u32(input.plaintext_length)?;
    Ok(transcript.finish())
}

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
