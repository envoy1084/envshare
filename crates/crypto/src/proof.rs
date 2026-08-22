//! Endpoint-bound Open, Offer, and acknowledgement proofs.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::{
    AuthenticationProof, CiphertextDigest, ClaimId, CryptoError, OfferProofInput, PeerContext,
    ReceiverNonce, Transcript,
    kdf::validate_context,
    types::{AcknowledgementKey, AuthenticationKey},
};

const OPEN_DOMAIN: &[u8] = b"envshare/open/v1";
const OFFER_DOMAIN: &[u8] = b"envshare/offer/v1";
const ACK_DOMAIN: &[u8] = b"envshare/ack/v1";

/// Creates the receiver proof carried by an Open request.
///
/// # Errors
///
/// Returns a typed context or transcript error when inputs are not canonical.
pub fn open_proof(
    key: &AuthenticationKey,
    context: PeerContext<'_>,
    receiver_nonce: ReceiverNonce,
) -> Result<AuthenticationProof, CryptoError> {
    validate_context(context)?;
    let mut transcript = common_transcript(OPEN_DOMAIN, context)?;
    transcript.append_bytes(receiver_nonce.as_bytes())?;
    authenticate(key.expose(), &transcript.finish())
}

/// Creates the sender proof binding all authenticated Offer metadata.
///
/// # Errors
///
/// Returns a typed context or transcript error when inputs are not canonical.
pub fn offer_proof(
    key: &AuthenticationKey,
    input: OfferProofInput<'_>,
) -> Result<AuthenticationProof, CryptoError> {
    validate_context(input.context)?;
    let mut transcript = common_transcript(OFFER_DOMAIN, input.context)?;
    transcript.append_bytes(input.receiver_nonce.as_bytes())?;
    transcript.append_bytes(input.sender_nonce.as_bytes())?;
    transcript.append_bytes(input.claim_id.as_bytes())?;
    transcript.append_u64(input.expires_at_unix_ms)?;
    transcript.append_u16(u16::from(input.content_type))?;
    transcript.append_u32(input.plaintext_length)?;
    transcript.append_bytes(input.aead_nonce.as_bytes())?;
    transcript.append_bytes(input.ciphertext_digest.as_bytes())?;
    authenticate(key.expose(), &transcript.finish())
}

/// Creates the receiver acknowledgement proof for one persisted claim.
///
/// # Errors
///
/// Returns a typed context or transcript error when inputs are not canonical.
pub fn acknowledgement_proof(
    key: &AcknowledgementKey,
    context: PeerContext<'_>,
    claim_id: ClaimId,
    ciphertext_digest: CiphertextDigest,
) -> Result<AuthenticationProof, CryptoError> {
    validate_context(context)?;
    let mut transcript = common_transcript(ACK_DOMAIN, context)?;
    transcript.append_bytes(claim_id.as_bytes())?;
    transcript.append_bytes(ciphertext_digest.as_bytes())?;
    authenticate(key.expose(), &transcript.finish())
}

fn common_transcript(
    domain: &'static [u8],
    context: PeerContext<'_>,
) -> Result<Transcript, CryptoError> {
    let mut transcript = Transcript::new(domain)?;
    transcript.append_u16(protocol::PROTOCOL_VERSION)?;
    transcript.append_bytes(context.network_id.as_bytes())?;
    transcript.append_bytes(context.room_id.as_bytes())?;
    transcript.append_bytes(context.sender_peer_id)?;
    transcript.append_bytes(context.receiver_peer_id)?;
    Ok(transcript)
}

fn authenticate(key: &[u8; 32], transcript: &[u8]) -> Result<AuthenticationProof, CryptoError> {
    let mut hmac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| CryptoError::KeyDerivation)?;
    hmac.update(transcript);
    let bytes = hmac.finalize().into_bytes();
    let mut proof = [0_u8; 32];
    proof.copy_from_slice(&bytes);
    Ok(AuthenticationProof::new(proof))
}

#[cfg(test)]
mod tests {
    use code::ShareCodeSecret;

    use super::*;
    use crate::{AeadNonce, SenderNonce, ciphertext_digest, derive_root, derive_session};

    #[test]
    fn open_proof_matches_independent_golden_vector() -> Result<(), CryptoError> {
        let root = derive_root(&ShareCodeSecret::new([0; 20]), "public-v1")?;
        let context = PeerContext {
            network_id: "public-v1",
            room_id: root.room_id(),
            sender_peer_id: b"sender-peer-id",
            receiver_peer_id: b"receiver-peer-id",
        };
        let proof = open_proof(
            root.authentication_key(),
            context,
            ReceiverNonce::new([1; 32]),
        )?;
        assert_eq!(
            proof.as_bytes(),
            &[
                0x8b, 0x13, 0xb6, 0x61, 0xd5, 0x99, 0xd9, 0x83, 0xbb, 0x29, 0x27, 0x3a, 0xce, 0xfa,
                0x6f, 0x7a, 0xe3, 0x9e, 0xc8, 0x3d, 0x93, 0x1f, 0x80, 0x31, 0x79, 0x5a, 0x7f, 0xbc,
                0x5c, 0x79, 0x39, 0xc8
            ]
        );
        assert!(proof.verifies(proof.as_bytes()));
        assert!(!proof.verifies(&[0; 32]));
        Ok(())
    }

    #[test]
    fn empty_authenticated_peer_id_is_rejected() -> Result<(), CryptoError> {
        let root = derive_root(&ShareCodeSecret::new([0; 20]), "public-v1")?;
        let context = PeerContext {
            network_id: "public-v1",
            room_id: root.room_id(),
            sender_peer_id: b"",
            receiver_peer_id: b"receiver-peer-id",
        };
        assert_eq!(
            open_proof(
                root.authentication_key(),
                context,
                ReceiverNonce::new([1; 32])
            ),
            Err(CryptoError::InvalidPeerIdentity)
        );
        Ok(())
    }

    #[test]
    fn offer_and_acknowledgement_match_independent_vectors() -> Result<(), CryptoError> {
        let root = derive_root(&ShareCodeSecret::new([0; 20]), "public-v1")?;
        let context = PeerContext {
            network_id: "public-v1",
            room_id: root.room_id(),
            sender_peer_id: b"sender-peer-id",
            receiver_peer_id: b"receiver-peer-id",
        };
        let receiver_nonce = ReceiverNonce::new([1; 32]);
        let sender_nonce = SenderNonce::new([2; 32]);
        let claim_id = ClaimId::new([3; 16]);
        let nonce = AeadNonce::new([4; 24]);
        let ciphertext = [
            0xbd, 0x12, 0xb0, 0x14, 0xf6, 0xf9, 0x0c, 0xca, 0xd9, 0x3c, 0x50, 0xdd, 0x36, 0xca,
            0xa5, 0x20, 0xd3, 0x3f, 0xa5, 0x13, 0x96, 0x3e, 0x66, 0x93, 0xe9, 0xec, 0x78, 0xdd,
            0x9b, 0x6a, 0xcb,
        ];
        let digest = ciphertext_digest(nonce, &ciphertext);
        let offer = offer_proof(
            root.authentication_key(),
            OfferProofInput {
                context,
                receiver_nonce,
                sender_nonce,
                claim_id,
                expires_at_unix_ms: 1_234_567_890_000,
                content_type: 0,
                plaintext_length: 15,
                aead_nonce: nonce,
                ciphertext_digest: digest,
            },
        )?;
        assert_eq!(
            offer.as_bytes(),
            &[
                0x40, 0xc6, 0x28, 0x5b, 0xd0, 0xea, 0xd5, 0xc5, 0x3e, 0xab, 0xfb, 0x6d, 0x47, 0xb3,
                0xa1, 0x9e, 0x09, 0xb0, 0x3e, 0xd9, 0x67, 0xda, 0x7e, 0xee, 0x93, 0x13, 0x82, 0xdb,
                0x23, 0x31, 0x3e, 0x0b
            ]
        );

        let session = derive_session(
            root.session_base_key(),
            context,
            receiver_nonce,
            sender_nonce,
            claim_id,
        )?;
        let acknowledgement =
            acknowledgement_proof(session.acknowledgement_key(), context, claim_id, digest)?;
        assert_eq!(
            acknowledgement.as_bytes(),
            &[
                0xb5, 0xe6, 0x97, 0x91, 0x33, 0xa2, 0x19, 0xf8, 0x8e, 0xce, 0x9e, 0xc9, 0x6f, 0x45,
                0xbe, 0x35, 0x21, 0xca, 0x7d, 0xc4, 0x86, 0x85, 0xb8, 0xf9, 0xfc, 0xae, 0xf5, 0xf8,
                0x62, 0x9d, 0x01, 0xf2
            ]
        );
        Ok(())
    }
}
