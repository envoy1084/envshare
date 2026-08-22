//! Offer verification and decryption.

use crypto::{
    AeadNonce, ClaimId, DerivedRoot, OfferProofInput, PayloadAadInput, PeerContext, ReceiverNonce,
    SenderNonce, acknowledgement_proof, decrypt_payload, derive_session, offer_proof, open_proof,
    payload_associated_data, verify_ciphertext_digest,
};
use protocol::{
    AcknowledgeRequest, ContentType, OfferResponse, OpenRequest, PROTOCOL_VERSION, SecretEnvelope,
};
use zeroize::Zeroizing;

use crate::CoreError;

/// State required to authenticate one sender and its encrypted offer.
pub struct ReceiverSession {
    root: DerivedRoot,
    network_id: String,
    sender_peer_id: Vec<u8>,
    receiver_peer_id: Vec<u8>,
    receiver_nonce: ReceiverNonce,
}

/// Authenticated plaintext plus an acknowledgement that must only be sent after
/// the caller safely handles the envelope.
pub struct VerifiedOffer {
    envelope: SecretEnvelope,
    acknowledgement: AcknowledgeRequest,
}

impl ReceiverSession {
    /// Creates a receiver session bound to authenticated connection identities.
    ///
    /// # Errors
    ///
    /// Returns a configuration error for empty identities or network names.
    pub fn new(
        root: DerivedRoot,
        network_id: String,
        sender_peer_id: Vec<u8>,
        receiver_peer_id: Vec<u8>,
        receiver_nonce: [u8; 32],
    ) -> Result<Self, CoreError> {
        if network_id.is_empty() || sender_peer_id.is_empty() || receiver_peer_id.is_empty() {
            return Err(CoreError::Configuration);
        }
        Ok(Self {
            root,
            network_id,
            sender_peer_id,
            receiver_peer_id,
            receiver_nonce: ReceiverNonce::new(receiver_nonce),
        })
    }

    /// Builds the capability proof used to request an offer.
    ///
    /// # Errors
    ///
    /// Returns a transfer error if local authenticated context is invalid.
    pub fn open_request(&self) -> Result<OpenRequest, CoreError> {
        let proof = open_proof(
            self.root.authentication_key(),
            self.context(),
            self.receiver_nonce,
        )
        .map_err(|_| CoreError::Transfer)?;
        Ok(OpenRequest {
            protocol_version: PROTOCOL_VERSION,
            room_id: *self.root.room_id().as_bytes(),
            receiver_nonce: *self.receiver_nonce.as_bytes(),
            receiver_proof: *proof.as_bytes(),
        })
    }

    /// Authenticates metadata and ciphertext before decoding the secret envelope.
    ///
    /// # Errors
    ///
    /// Every remote authentication, integrity, decryption, and envelope failure
    /// maps to the same secret-safe transfer error.
    pub fn verify_offer(&self, offer: &OfferResponse) -> Result<VerifiedOffer, CoreError> {
        if offer.protocol_version != PROTOCOL_VERSION {
            return Err(CoreError::Transfer);
        }
        ContentType::try_from(offer.content_type).map_err(|_| CoreError::Transfer)?;
        let context = self.context();
        let sender_nonce = SenderNonce::new(offer.sender_nonce);
        let claim_id = ClaimId::new(offer.claim_id);
        let aead_nonce = AeadNonce::new(offer.aead_nonce);
        let digest = crypto::CiphertextDigest::new(offer.ciphertext_digest);
        let expected_offer = offer_proof(
            self.root.authentication_key(),
            OfferProofInput {
                context,
                receiver_nonce: self.receiver_nonce,
                sender_nonce,
                claim_id,
                expires_at_unix_ms: offer.expires_at_unix_ms,
                content_type: offer.content_type,
                plaintext_length: offer.plaintext_length,
                aead_nonce,
                ciphertext_digest: digest,
            },
        )
        .map_err(|_| CoreError::Transfer)?;
        if !expected_offer.verifies(&offer.sender_proof)
            || !verify_ciphertext_digest(digest, aead_nonce, &offer.ciphertext)
        {
            return Err(CoreError::Transfer);
        }
        let session = derive_session(
            self.root.session_base_key(),
            context,
            self.receiver_nonce,
            sender_nonce,
            claim_id,
        )
        .map_err(|_| CoreError::Transfer)?;
        let aad = payload_associated_data(PayloadAadInput {
            context,
            receiver_nonce: self.receiver_nonce,
            sender_nonce,
            claim_id,
            expires_at_unix_ms: offer.expires_at_unix_ms,
            content_type: offer.content_type,
            plaintext_length: offer.plaintext_length,
        })
        .map_err(|_| CoreError::Transfer)?;
        let plaintext = Zeroizing::new(
            decrypt_payload(session.payload_key(), aead_nonce, &offer.ciphertext, &aad)
                .map_err(|_| CoreError::Transfer)?,
        );
        let envelope = SecretEnvelope::decode(&plaintext).map_err(|_| CoreError::Transfer)?;
        if envelope.content_type() as u8 != offer.content_type
            || envelope.expires_at_unix_ms() != offer.expires_at_unix_ms
            || u32::try_from(envelope.payload().len()).map_err(|_| CoreError::Transfer)?
                != offer.plaintext_length
        {
            return Err(CoreError::Transfer);
        }
        let acknowledgement_proof =
            acknowledgement_proof(session.acknowledgement_key(), context, claim_id, digest)
                .map_err(|_| CoreError::Transfer)?;
        Ok(VerifiedOffer {
            envelope,
            acknowledgement: AcknowledgeRequest {
                protocol_version: PROTOCOL_VERSION,
                claim_id: offer.claim_id,
                ciphertext_digest: offer.ciphertext_digest,
                acknowledgement_proof: *acknowledgement_proof.as_bytes(),
            },
        })
    }

    fn context(&self) -> PeerContext<'_> {
        PeerContext {
            network_id: &self.network_id,
            room_id: self.root.room_id(),
            sender_peer_id: &self.sender_peer_id,
            receiver_peer_id: &self.receiver_peer_id,
        }
    }
}

impl VerifiedOffer {
    /// Borrows the authenticated secret envelope.
    #[must_use]
    pub const fn envelope(&self) -> &SecretEnvelope {
        &self.envelope
    }

    /// Borrows the acknowledgement to send after safe output handling.
    #[must_use]
    pub const fn acknowledgement(&self) -> &AcknowledgeRequest {
        &self.acknowledgement
    }

    /// Separates the envelope and acknowledgement after verification.
    #[must_use]
    pub fn into_parts(self) -> (SecretEnvelope, AcknowledgeRequest) {
        (self.envelope, self.acknowledgement)
    }
}

impl std::fmt::Debug for ReceiverSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ReceiverSession([REDACTED])")
    }
}

impl std::fmt::Debug for VerifiedOffer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("VerifiedOffer([REDACTED])")
    }
}
