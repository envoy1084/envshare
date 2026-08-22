//! Atomic single-use sender actor.

use std::time::{Duration, Instant};

use crypto::{
    AeadNonce, AuthenticationProof, CiphertextDigest, ClaimId, DerivedRoot, OfferProofInput,
    PayloadAadInput, PeerContext, ReceiverNonce, SenderNonce, acknowledgement_proof,
    ciphertext_digest, derive_session, encrypt_payload, offer_proof, open_proof,
    payload_associated_data,
};
use protocol::{
    AcknowledgeRequest, CompletedResponse, OfferResponse, OpenRequest, PROTOCOL_VERSION,
    ProtocolErrorCode, SecretEnvelope,
};
use zeroize::Zeroizing;

use super::{OfferEntropy, SenderState};
use crate::CoreError;

struct BoundClaim {
    receiver_peer_id: Vec<u8>,
    receiver_nonce: ReceiverNonce,
    claim_id: ClaimId,
    digest: CiphertextDigest,
    expected_acknowledgement: AuthenticationProof,
    offer: OfferResponse,
    resume_deadline: Instant,
}

/// Single-owner state machine for one single-use share.
///
/// The actor must be driven serially by its owning task. It deliberately has no
/// internal lock, which makes the first transition out of `Available` atomic.
pub struct SenderActor {
    state: SenderState,
    root: Option<DerivedRoot>,
    room_id: [u8; 16],
    network_id: String,
    sender_peer_id: Vec<u8>,
    encoded_envelope: Option<Zeroizing<Vec<u8>>>,
    content_type: u8,
    plaintext_length: u32,
    expires_at_unix_ms: u64,
    available_deadline: Instant,
    resume_duration: Duration,
    claim: Option<BoundClaim>,
}

impl SenderActor {
    /// Creates an available share after encoding and bounding its secret envelope.
    ///
    /// # Errors
    ///
    /// Returns a safe local error if the envelope cannot be represented or a
    /// peer identity is empty.
    pub fn new(
        root: DerivedRoot,
        network_id: String,
        sender_peer_id: Vec<u8>,
        envelope: &SecretEnvelope,
        available_deadline: Instant,
        resume_duration: Duration,
    ) -> Result<Self, CoreError> {
        if sender_peer_id.is_empty() || network_id.is_empty() {
            return Err(CoreError::Configuration);
        }
        let plaintext_length =
            u32::try_from(envelope.payload().len()).map_err(|_| CoreError::Internal)?;
        let encoded_envelope = envelope.encode().map_err(|_| CoreError::Transfer)?;
        Ok(Self {
            state: SenderState::Available,
            room_id: *root.room_id().as_bytes(),
            root: Some(root),
            network_id,
            sender_peer_id,
            encoded_envelope: Some(Zeroizing::new(encoded_envelope)),
            content_type: envelope.content_type() as u8,
            plaintext_length,
            expires_at_unix_ms: envelope.expires_at_unix_ms(),
            available_deadline,
            resume_duration,
            claim: None,
        })
    }

    /// Returns the current lifecycle state without exposing claim metadata.
    #[must_use]
    pub const fn state(&self) -> SenderState {
        self.state
    }

    /// Authenticates an Open and either creates or resumes the winning offer.
    ///
    /// # Errors
    ///
    /// Returns only stable, secret-safe protocol classifications.
    pub fn handle_open(
        &mut self,
        receiver_peer_id: &[u8],
        request: &OpenRequest,
        now: Instant,
        entropy: &mut impl OfferEntropy,
    ) -> Result<OfferResponse, ProtocolErrorCode> {
        if request.protocol_version != PROTOCOL_VERSION {
            return Err(ProtocolErrorCode::UnsupportedVersion);
        }
        if !self.authenticates_open(receiver_peer_id, request) {
            return Err(ProtocolErrorCode::NotFoundOrUnauthorized);
        }

        match self.state {
            SenderState::Available if now >= self.available_deadline => {
                self.close(SenderState::Expired);
                return Err(ProtocolErrorCode::ShareExpired);
            }
            SenderState::Available => {}
            SenderState::Disclosed => {
                if self
                    .claim
                    .as_ref()
                    .is_some_and(|claim| now >= claim.resume_deadline)
                {
                    self.close(SenderState::DeliveryUnknown);
                    return Err(ProtocolErrorCode::ShareUnavailable);
                }
                return self.resume_offer(receiver_peer_id, request);
            }
            SenderState::PreparingOffer => return Err(ProtocolErrorCode::TemporarilyUnavailable),
            SenderState::Consumed
            | SenderState::Expired
            | SenderState::DeliveryUnknown
            | SenderState::FailedClosed => return Err(ProtocolErrorCode::ShareUnavailable),
        }

        self.state = SenderState::PreparingOffer;
        if let Ok((offer, claim)) = self.prepare_offer(receiver_peer_id, request, now, entropy) {
            self.claim = Some(claim);
            self.state = SenderState::Disclosed;
            Ok(offer)
        } else {
            self.close(SenderState::FailedClosed);
            Err(ProtocolErrorCode::InternalFailure)
        }
    }

    /// Validates a winning receiver acknowledgement idempotently.
    ///
    /// # Errors
    ///
    /// Returns a safe claim or availability classification.
    pub fn handle_acknowledgement(
        &mut self,
        receiver_peer_id: &[u8],
        request: &AcknowledgeRequest,
    ) -> Result<CompletedResponse, ProtocolErrorCode> {
        if request.protocol_version != PROTOCOL_VERSION {
            return Err(ProtocolErrorCode::UnsupportedVersion);
        }
        if !matches!(self.state, SenderState::Disclosed | SenderState::Consumed) {
            return Err(ProtocolErrorCode::ShareUnavailable);
        }
        let claim = self
            .claim
            .as_ref()
            .ok_or(ProtocolErrorCode::ShareUnavailable)?;
        if receiver_peer_id != claim.receiver_peer_id
            || request.claim_id != *claim.claim_id.as_bytes()
            || request.ciphertext_digest != *claim.digest.as_bytes()
            || !claim
                .expected_acknowledgement
                .verifies(&request.acknowledgement_proof)
        {
            return Err(ProtocolErrorCode::ClaimMismatch);
        }
        let completed = CompletedResponse {
            protocol_version: PROTOCOL_VERSION,
            claim_id: request.claim_id,
        };
        if self.state == SenderState::Disclosed {
            self.state = SenderState::Consumed;
            self.wipe_source();
        }
        Ok(completed)
    }

    /// Applies sender-owned expiry and acknowledgement deadlines.
    pub fn handle_timer(&mut self, now: Instant) {
        if self.state == SenderState::Available && now >= self.available_deadline {
            self.close(SenderState::Expired);
        } else if self.state == SenderState::Disclosed
            && self
                .claim
                .as_ref()
                .is_some_and(|claim| now >= claim.resume_deadline)
        {
            self.close(SenderState::DeliveryUnknown);
        }
    }

    fn authenticates_open(&self, receiver_peer_id: &[u8], request: &OpenRequest) -> bool {
        let Some(root) = self.root.as_ref() else {
            return false;
        };
        if request.room_id != self.room_id {
            return false;
        }
        let context = self.context(receiver_peer_id);
        open_proof(
            root.authentication_key(),
            context,
            ReceiverNonce::new(request.receiver_nonce),
        )
        .is_ok_and(|expected| expected.verifies(&request.receiver_proof))
    }

    fn resume_offer(
        &self,
        receiver_peer_id: &[u8],
        request: &OpenRequest,
    ) -> Result<OfferResponse, ProtocolErrorCode> {
        let claim = self
            .claim
            .as_ref()
            .ok_or(ProtocolErrorCode::ShareUnavailable)?;
        if receiver_peer_id == claim.receiver_peer_id
            && request.receiver_nonce == *claim.receiver_nonce.as_bytes()
        {
            Ok(claim.offer.clone())
        } else {
            Err(ProtocolErrorCode::ShareAlreadyClaimed)
        }
    }

    fn prepare_offer(
        &self,
        receiver_peer_id: &[u8],
        request: &OpenRequest,
        now: Instant,
        entropy: &mut impl OfferEntropy,
    ) -> Result<(OfferResponse, BoundClaim), ()> {
        let root = self.root.as_ref().ok_or(())?;
        let plaintext = self.encoded_envelope.as_ref().ok_or(())?;
        let receiver_nonce = ReceiverNonce::new(request.receiver_nonce);
        let mut sender_nonce_bytes = [0_u8; 32];
        let mut claim_id_bytes = [0_u8; 16];
        let mut aead_nonce_bytes = [0_u8; 24];
        entropy.fill(&mut sender_nonce_bytes).map_err(|_| ())?;
        entropy.fill(&mut claim_id_bytes).map_err(|_| ())?;
        entropy.fill(&mut aead_nonce_bytes).map_err(|_| ())?;
        let sender_nonce = SenderNonce::new(sender_nonce_bytes);
        let claim_id = ClaimId::new(claim_id_bytes);
        let aead_nonce = AeadNonce::new(aead_nonce_bytes);
        let context = self.context(receiver_peer_id);
        let session = derive_session(
            root.session_base_key(),
            context,
            receiver_nonce,
            sender_nonce,
            claim_id,
        )
        .map_err(|_| ())?;
        let aad = payload_associated_data(PayloadAadInput {
            context,
            receiver_nonce,
            sender_nonce,
            claim_id,
            expires_at_unix_ms: self.expires_at_unix_ms,
            content_type: self.content_type,
            plaintext_length: self.plaintext_length,
        })
        .map_err(|_| ())?;
        let ciphertext =
            encrypt_payload(session.payload_key(), aead_nonce, plaintext, &aad).map_err(|_| ())?;
        let digest = ciphertext_digest(aead_nonce, &ciphertext);
        let sender_proof = offer_proof(
            root.authentication_key(),
            OfferProofInput {
                context,
                receiver_nonce,
                sender_nonce,
                claim_id,
                expires_at_unix_ms: self.expires_at_unix_ms,
                content_type: self.content_type,
                plaintext_length: self.plaintext_length,
                aead_nonce,
                ciphertext_digest: digest,
            },
        )
        .map_err(|_| ())?;
        let expected_acknowledgement =
            acknowledgement_proof(session.acknowledgement_key(), context, claim_id, digest)
                .map_err(|_| ())?;
        let offer = OfferResponse {
            protocol_version: PROTOCOL_VERSION,
            claim_id: claim_id_bytes,
            sender_nonce: sender_nonce_bytes,
            aead_nonce: aead_nonce_bytes,
            expires_at_unix_ms: self.expires_at_unix_ms,
            content_type: self.content_type,
            plaintext_length: self.plaintext_length,
            ciphertext,
            ciphertext_digest: *digest.as_bytes(),
            sender_proof: *sender_proof.as_bytes(),
        };
        let claim = BoundClaim {
            receiver_peer_id: receiver_peer_id.to_vec(),
            receiver_nonce,
            claim_id,
            digest,
            expected_acknowledgement,
            offer: offer.clone(),
            resume_deadline: now.checked_add(self.resume_duration).ok_or(())?,
        };
        Ok((offer, claim))
    }

    fn context<'a>(&'a self, receiver_peer_id: &'a [u8]) -> PeerContext<'a> {
        PeerContext {
            network_id: &self.network_id,
            room_id: crypto::RoomId::new(self.room_id),
            sender_peer_id: &self.sender_peer_id,
            receiver_peer_id,
        }
    }

    fn close(&mut self, state: SenderState) {
        self.state = state;
        self.wipe_source();
    }

    fn wipe_source(&mut self) {
        self.root = None;
        self.encoded_envelope = None;
    }
}

impl std::fmt::Debug for SenderActor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SenderActor")
            .field("state", &self.state)
            .field("secret_material", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}
