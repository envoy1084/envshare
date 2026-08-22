//! Cryptographic protocol primitives for Envshare.

#![forbid(unsafe_code)]

mod aead;
mod digest;
mod error;
mod kdf;
mod proof;
mod transcript;
mod types;

pub use aead::{decrypt_payload, encrypt_payload, generate_aead_nonce};
pub use digest::{ciphertext_digest, verify_ciphertext_digest};
pub use error::CryptoError;
pub use kdf::{derive_root, derive_session};
pub use proof::{acknowledgement_proof, offer_proof, open_proof};
pub use transcript::Transcript;
pub use types::{
    AcknowledgementKey, AeadNonce, AuthenticationKey, AuthenticationProof, CiphertextDigest,
    ClaimId, DerivedRoot, OfferProofInput, PayloadKey, PeerContext, ReceiverNonce, RoomId,
    SenderNonce, SessionBaseKey, SessionKeys,
};

/// Domain separation label used by the v1 root key derivation.
pub const ROOT_SALT_DOMAIN: &[u8] = b"envshare/root-salt/v1";

#[cfg(test)]
mod tests {
    use code::ShareCodeSecret;

    use super::*;

    fn context<'a>(room_id: RoomId) -> PeerContext<'a> {
        PeerContext {
            network_id: "public-v1",
            room_id,
            sender_peer_id: b"sender-peer-id",
            receiver_peer_id: b"receiver-peer-id",
        }
    }

    #[test]
    fn root_domain_is_explicitly_versioned() {
        assert!(ROOT_SALT_DOMAIN.ends_with(b"/v1"));
    }

    #[test]
    fn root_and_session_keys_are_deterministic_and_separated() -> Result<(), CryptoError> {
        let root = derive_root(&ShareCodeSecret::new([7; 20]), "public-v1")?;
        let context = context(root.room_id());
        let session = derive_session(
            root.session_base_key(),
            context,
            ReceiverNonce::new([1; 32]),
            SenderNonce::new([2; 32]),
            ClaimId::new([3; 16]),
        )?;

        assert!(!session.keys_are_equal());
        Ok(())
    }

    #[test]
    fn payload_round_trip_authenticates_associated_data() -> Result<(), CryptoError> {
        let root = derive_root(&ShareCodeSecret::new([0; 20]), "public-v1")?;
        let session = derive_session(
            root.session_base_key(),
            context(root.room_id()),
            ReceiverNonce::new([1; 32]),
            SenderNonce::new([2; 32]),
            ClaimId::new([3; 16]),
        )?;
        let nonce = AeadNonce::new([4; 24]);
        let ciphertext = encrypt_payload(session.payload_key(), nonce, b"SECRET=sentinel", b"aad")?;
        assert_eq!(
            ciphertext,
            [
                0xbd, 0x12, 0xb0, 0x14, 0xf6, 0xf9, 0x0c, 0xca, 0xd9, 0x3c, 0x50, 0xdd, 0x36, 0xca,
                0xa5, 0x20, 0xd3, 0x3f, 0xa5, 0x13, 0x96, 0x3e, 0x66, 0x93, 0xe9, 0xec, 0x78, 0xdd,
                0x9b, 0x6a, 0xcb,
            ]
        );
        let digest = ciphertext_digest(nonce, &ciphertext);
        assert_eq!(
            digest.as_bytes(),
            &[
                0xf1, 0x75, 0xa8, 0x44, 0x14, 0x16, 0x68, 0x64, 0x50, 0xf0, 0x73, 0xef, 0xc0, 0x49,
                0xa6, 0x06, 0x5d, 0x2c, 0x4d, 0x7c, 0xd1, 0x2f, 0xf5, 0xda, 0x14, 0x2a, 0x54, 0x23,
                0x71, 0x47, 0x53, 0x99
            ]
        );
        assert!(verify_ciphertext_digest(digest, nonce, &ciphertext));
        let plaintext = decrypt_payload(session.payload_key(), nonce, &ciphertext, b"aad")?;
        assert_eq!(plaintext, b"SECRET=sentinel");
        assert!(decrypt_payload(session.payload_key(), nonce, &ciphertext, b"changed").is_err());
        Ok(())
    }
}
