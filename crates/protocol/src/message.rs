//! Strict numeric-key CBOR transfer messages.

use std::fmt;

use minicbor::{Decoder, Encoder};

use crate::{
    MAX_ACK_BODY_BYTES, MAX_CIPHERTEXT_BYTES, MAX_COMPLETED_BODY_BYTES, MAX_ERROR_BODY_BYTES,
    MAX_OFFER_BODY_BYTES, MAX_OPEN_BODY_BYTES, MAX_PAYLOAD_BYTES, ProtocolErrorCode, WireError,
};

const REQUEST_OPEN: u8 = 0;
const REQUEST_ACKNOWLEDGE: u8 = 1;
const RESPONSE_OFFER: u8 = 0;
const RESPONSE_COMPLETED: u8 = 1;
const RESPONSE_ERROR: u8 = 2;

/// Receiver authentication request.
#[derive(Clone, Eq, PartialEq)]
pub struct OpenRequest {
    /// Wire protocol version.
    pub protocol_version: u16,
    /// Capability-derived room identifier.
    pub room_id: [u8; 16],
    /// Fresh receiver challenge.
    pub receiver_nonce: [u8; 32],
    /// HMAC proof of capability possession.
    pub receiver_proof: [u8; 32],
}

impl fmt::Debug for OpenRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenRequest")
            .field("protocol_version", &self.protocol_version)
            .field("authentication", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

/// Sender's encrypted offer for the winning claim.
#[derive(Clone, Eq, PartialEq)]
pub struct OfferResponse {
    /// Wire protocol version.
    pub protocol_version: u16,
    /// Winning claim identifier.
    pub claim_id: [u8; 16],
    /// Fresh sender challenge.
    pub sender_nonce: [u8; 32],
    /// Unique XChaCha20-Poly1305 nonce.
    pub aead_nonce: [u8; 24],
    /// Sender-authoritative expiry metadata.
    pub expires_at_unix_ms: u64,
    /// Stable content-type discriminant.
    pub content_type: u8,
    /// Authenticated plaintext payload length.
    pub plaintext_length: u32,
    /// Application-encrypted envelope.
    pub ciphertext: Vec<u8>,
    /// Digest of AEAD nonce and ciphertext.
    pub ciphertext_digest: [u8; 32],
    /// HMAC proof authenticating the sender and offer.
    pub sender_proof: [u8; 32],
}

impl fmt::Debug for OfferResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OfferResponse")
            .field("protocol_version", &self.protocol_version)
            .field("plaintext_length", &self.plaintext_length)
            .field("ciphertext_length", &self.ciphertext.len())
            .field("authentication", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

/// Receiver acknowledgement after safe payload handling.
#[derive(Clone, Eq, PartialEq)]
pub struct AcknowledgeRequest {
    /// Wire protocol version.
    pub protocol_version: u16,
    /// Bound claim identifier.
    pub claim_id: [u8; 16],
    /// Digest of the offer that was handled.
    pub ciphertext_digest: [u8; 32],
    /// Claim-specific HMAC acknowledgement.
    pub acknowledgement_proof: [u8; 32],
}

impl fmt::Debug for AcknowledgeRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AcknowledgeRequest")
            .field("protocol_version", &self.protocol_version)
            .field("authentication", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

/// Idempotent successful acknowledgement response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletedResponse {
    /// Wire protocol version.
    pub protocol_version: u16,
    /// Completed claim identifier.
    pub claim_id: [u8; 16],
}

/// Stable machine-readable protocol failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolErrorResponse {
    /// Wire protocol version.
    pub protocol_version: u16,
    /// Safe error classification.
    pub code: ProtocolErrorCode,
}

/// Request carried by the transfer request-response behavior.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransferRequest {
    /// Attempt to authenticate and claim a share.
    Open(OpenRequest),
    /// Confirm safe handling of one encrypted offer.
    Acknowledge(AcknowledgeRequest),
}

/// Response carried by the transfer request-response behavior.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransferResponse {
    /// Encrypted share for the bound claim.
    Offer(OfferResponse),
    /// Idempotent acknowledgement completion.
    Completed(CompletedResponse),
    /// Secret-safe protocol failure.
    Error(ProtocolErrorResponse),
}

pub(crate) fn encode_request_body(request: &TransferRequest) -> Result<Vec<u8>, WireError> {
    let mut encoder = Encoder::new(Vec::with_capacity(256));
    encoder.array(2).map_err(|_| WireError::InvalidMessage)?;
    match request {
        TransferRequest::Open(request) => {
            encoder
                .u8(REQUEST_OPEN)
                .map_err(|_| WireError::InvalidMessage)?;
            encode_open(&mut encoder, request)?;
        }
        TransferRequest::Acknowledge(request) => {
            encoder
                .u8(REQUEST_ACKNOWLEDGE)
                .map_err(|_| WireError::InvalidMessage)?;
            encode_acknowledgement(&mut encoder, request)?;
        }
    }
    let body = encoder.into_writer();
    if body.len() > MAX_OPEN_BODY_BYTES.max(MAX_ACK_BODY_BYTES) {
        return Err(WireError::FrameTooLarge);
    }
    Ok(body)
}

pub(crate) fn decode_request_body(encoded: &[u8]) -> Result<TransferRequest, WireError> {
    if encoded.is_empty() || encoded.len() > MAX_OPEN_BODY_BYTES.max(MAX_ACK_BODY_BYTES) {
        return Err(WireError::FrameTooLarge);
    }
    let mut decoder = Decoder::new(encoded);
    expect_array(&mut decoder, 2)?;
    let request = match decoder.u8().map_err(|_| WireError::InvalidMessage)? {
        REQUEST_OPEN => TransferRequest::Open(decode_open(&mut decoder)?),
        REQUEST_ACKNOWLEDGE => TransferRequest::Acknowledge(decode_acknowledgement(&mut decoder)?),
        _ => return Err(WireError::InvalidMessage),
    };
    finish_decode(&decoder, encoded)?;
    Ok(request)
}

pub(crate) fn encode_response_body(response: &TransferResponse) -> Result<Vec<u8>, WireError> {
    let mut encoder = Encoder::new(Vec::with_capacity(256));
    encoder.array(2).map_err(|_| WireError::InvalidMessage)?;
    let limit = match response {
        TransferResponse::Offer(response) => {
            validate_offer(response)?;
            encoder
                .u8(RESPONSE_OFFER)
                .map_err(|_| WireError::InvalidMessage)?;
            encode_offer(&mut encoder, response)?;
            MAX_OFFER_BODY_BYTES
        }
        TransferResponse::Completed(response) => {
            encoder
                .u8(RESPONSE_COMPLETED)
                .map_err(|_| WireError::InvalidMessage)?;
            encode_completed(&mut encoder, response)?;
            MAX_COMPLETED_BODY_BYTES
        }
        TransferResponse::Error(response) => {
            encoder
                .u8(RESPONSE_ERROR)
                .map_err(|_| WireError::InvalidMessage)?;
            encode_error(&mut encoder, response)?;
            MAX_ERROR_BODY_BYTES
        }
    };
    let body = encoder.into_writer();
    if body.len() > limit {
        return Err(WireError::FrameTooLarge);
    }
    Ok(body)
}

pub(crate) fn decode_response_body(encoded: &[u8]) -> Result<TransferResponse, WireError> {
    if encoded.is_empty() || encoded.len() > MAX_OFFER_BODY_BYTES {
        return Err(WireError::FrameTooLarge);
    }
    let mut decoder = Decoder::new(encoded);
    expect_array(&mut decoder, 2)?;
    let (response, limit) = match decoder.u8().map_err(|_| WireError::InvalidMessage)? {
        RESPONSE_OFFER => {
            let offer = decode_offer(&mut decoder)?;
            validate_offer(&offer)?;
            (TransferResponse::Offer(offer), MAX_OFFER_BODY_BYTES)
        }
        RESPONSE_COMPLETED => (
            TransferResponse::Completed(decode_completed(&mut decoder)?),
            MAX_COMPLETED_BODY_BYTES,
        ),
        RESPONSE_ERROR => (
            TransferResponse::Error(decode_error(&mut decoder)?),
            MAX_ERROR_BODY_BYTES,
        ),
        _ => return Err(WireError::InvalidMessage),
    };
    if encoded.len() > limit {
        return Err(WireError::FrameTooLarge);
    }
    finish_decode(&decoder, encoded)?;
    Ok(response)
}

fn encode_open(encoder: &mut Encoder<Vec<u8>>, request: &OpenRequest) -> Result<(), WireError> {
    encoder
        .map(4)
        .and_then(|encoder| encoder.u8(0))
        .and_then(|encoder| encoder.u16(request.protocol_version))
        .and_then(|encoder| encoder.u8(1))
        .and_then(|encoder| encoder.bytes(&request.room_id))
        .and_then(|encoder| encoder.u8(2))
        .and_then(|encoder| encoder.bytes(&request.receiver_nonce))
        .and_then(|encoder| encoder.u8(3))
        .and_then(|encoder| encoder.bytes(&request.receiver_proof))
        .map_err(|_| WireError::InvalidMessage)?;
    Ok(())
}

fn decode_open(decoder: &mut Decoder<'_>) -> Result<OpenRequest, WireError> {
    expect_map(decoder, 4)?;
    expect_key(decoder, 0)?;
    let protocol_version = decoder.u16().map_err(|_| WireError::InvalidMessage)?;
    expect_key(decoder, 1)?;
    let room_id = decode_fixed(decoder)?;
    expect_key(decoder, 2)?;
    let receiver_nonce = decode_fixed(decoder)?;
    expect_key(decoder, 3)?;
    let receiver_proof = decode_fixed(decoder)?;
    Ok(OpenRequest {
        protocol_version,
        room_id,
        receiver_nonce,
        receiver_proof,
    })
}

fn encode_offer(encoder: &mut Encoder<Vec<u8>>, response: &OfferResponse) -> Result<(), WireError> {
    encoder
        .map(10)
        .and_then(|encoder| encoder.u8(0))
        .and_then(|encoder| encoder.u16(response.protocol_version))
        .and_then(|encoder| encoder.u8(1))
        .and_then(|encoder| encoder.bytes(&response.claim_id))
        .and_then(|encoder| encoder.u8(2))
        .and_then(|encoder| encoder.bytes(&response.sender_nonce))
        .and_then(|encoder| encoder.u8(3))
        .and_then(|encoder| encoder.bytes(&response.aead_nonce))
        .and_then(|encoder| encoder.u8(4))
        .and_then(|encoder| encoder.u64(response.expires_at_unix_ms))
        .and_then(|encoder| encoder.u8(5))
        .and_then(|encoder| encoder.u8(response.content_type))
        .and_then(|encoder| encoder.u8(6))
        .and_then(|encoder| encoder.u32(response.plaintext_length))
        .and_then(|encoder| encoder.u8(7))
        .and_then(|encoder| encoder.bytes(&response.ciphertext))
        .and_then(|encoder| encoder.u8(8))
        .and_then(|encoder| encoder.bytes(&response.ciphertext_digest))
        .and_then(|encoder| encoder.u8(9))
        .and_then(|encoder| encoder.bytes(&response.sender_proof))
        .map_err(|_| WireError::InvalidMessage)?;
    Ok(())
}

fn decode_offer(decoder: &mut Decoder<'_>) -> Result<OfferResponse, WireError> {
    expect_map(decoder, 10)?;
    expect_key(decoder, 0)?;
    let protocol_version = decoder.u16().map_err(|_| WireError::InvalidMessage)?;
    expect_key(decoder, 1)?;
    let claim_id = decode_fixed(decoder)?;
    expect_key(decoder, 2)?;
    let sender_nonce = decode_fixed(decoder)?;
    expect_key(decoder, 3)?;
    let aead_nonce = decode_fixed(decoder)?;
    expect_key(decoder, 4)?;
    let expires_at_unix_ms = decoder.u64().map_err(|_| WireError::InvalidMessage)?;
    expect_key(decoder, 5)?;
    let content_type = decoder.u8().map_err(|_| WireError::InvalidMessage)?;
    expect_key(decoder, 6)?;
    let plaintext_length = decoder.u32().map_err(|_| WireError::InvalidMessage)?;
    expect_key(decoder, 7)?;
    let ciphertext = decoder.bytes().map_err(|_| WireError::InvalidMessage)?;
    if ciphertext.len() > MAX_CIPHERTEXT_BYTES {
        return Err(WireError::FrameTooLarge);
    }
    let ciphertext = ciphertext.to_vec();
    expect_key(decoder, 8)?;
    let ciphertext_digest = decode_fixed(decoder)?;
    expect_key(decoder, 9)?;
    let sender_proof = decode_fixed(decoder)?;
    Ok(OfferResponse {
        protocol_version,
        claim_id,
        sender_nonce,
        aead_nonce,
        expires_at_unix_ms,
        content_type,
        plaintext_length,
        ciphertext,
        ciphertext_digest,
        sender_proof,
    })
}

fn validate_offer(response: &OfferResponse) -> Result<(), WireError> {
    if usize::try_from(response.plaintext_length).map_err(|_| WireError::FrameTooLarge)?
        > MAX_PAYLOAD_BYTES
        || response.ciphertext.len() > MAX_CIPHERTEXT_BYTES
    {
        return Err(WireError::FrameTooLarge);
    }
    Ok(())
}

fn encode_acknowledgement(
    encoder: &mut Encoder<Vec<u8>>,
    request: &AcknowledgeRequest,
) -> Result<(), WireError> {
    encoder
        .map(4)
        .and_then(|encoder| encoder.u8(0))
        .and_then(|encoder| encoder.u16(request.protocol_version))
        .and_then(|encoder| encoder.u8(1))
        .and_then(|encoder| encoder.bytes(&request.claim_id))
        .and_then(|encoder| encoder.u8(2))
        .and_then(|encoder| encoder.bytes(&request.ciphertext_digest))
        .and_then(|encoder| encoder.u8(3))
        .and_then(|encoder| encoder.bytes(&request.acknowledgement_proof))
        .map_err(|_| WireError::InvalidMessage)?;
    Ok(())
}

fn decode_acknowledgement(decoder: &mut Decoder<'_>) -> Result<AcknowledgeRequest, WireError> {
    expect_map(decoder, 4)?;
    expect_key(decoder, 0)?;
    let protocol_version = decoder.u16().map_err(|_| WireError::InvalidMessage)?;
    expect_key(decoder, 1)?;
    let claim_id = decode_fixed(decoder)?;
    expect_key(decoder, 2)?;
    let ciphertext_digest = decode_fixed(decoder)?;
    expect_key(decoder, 3)?;
    let acknowledgement_proof = decode_fixed(decoder)?;
    Ok(AcknowledgeRequest {
        protocol_version,
        claim_id,
        ciphertext_digest,
        acknowledgement_proof,
    })
}

fn encode_completed(
    encoder: &mut Encoder<Vec<u8>>,
    response: &CompletedResponse,
) -> Result<(), WireError> {
    encoder
        .map(2)
        .and_then(|encoder| encoder.u8(0))
        .and_then(|encoder| encoder.u16(response.protocol_version))
        .and_then(|encoder| encoder.u8(1))
        .and_then(|encoder| encoder.bytes(&response.claim_id))
        .map_err(|_| WireError::InvalidMessage)?;
    Ok(())
}

fn decode_completed(decoder: &mut Decoder<'_>) -> Result<CompletedResponse, WireError> {
    expect_map(decoder, 2)?;
    expect_key(decoder, 0)?;
    let protocol_version = decoder.u16().map_err(|_| WireError::InvalidMessage)?;
    expect_key(decoder, 1)?;
    let claim_id = decode_fixed(decoder)?;
    Ok(CompletedResponse {
        protocol_version,
        claim_id,
    })
}

fn encode_error(
    encoder: &mut Encoder<Vec<u8>>,
    response: &ProtocolErrorResponse,
) -> Result<(), WireError> {
    encoder
        .map(2)
        .and_then(|encoder| encoder.u8(0))
        .and_then(|encoder| encoder.u16(response.protocol_version))
        .and_then(|encoder| encoder.u8(1))
        .and_then(|encoder| encoder.u8(response.code as u8))
        .map_err(|_| WireError::InvalidMessage)?;
    Ok(())
}

fn decode_error(decoder: &mut Decoder<'_>) -> Result<ProtocolErrorResponse, WireError> {
    expect_map(decoder, 2)?;
    expect_key(decoder, 0)?;
    let protocol_version = decoder.u16().map_err(|_| WireError::InvalidMessage)?;
    expect_key(decoder, 1)?;
    let code = ProtocolErrorCode::try_from(decoder.u8().map_err(|_| WireError::InvalidMessage)?)?;
    Ok(ProtocolErrorResponse {
        protocol_version,
        code,
    })
}

fn expect_array(decoder: &mut Decoder<'_>, length: u64) -> Result<(), WireError> {
    if decoder.array().map_err(|_| WireError::InvalidMessage)? != Some(length) {
        return Err(WireError::InvalidMessage);
    }
    Ok(())
}

fn expect_map(decoder: &mut Decoder<'_>, length: u64) -> Result<(), WireError> {
    if decoder.map().map_err(|_| WireError::InvalidMessage)? != Some(length) {
        return Err(WireError::InvalidMessage);
    }
    Ok(())
}

fn expect_key(decoder: &mut Decoder<'_>, expected: u8) -> Result<(), WireError> {
    if decoder.u8().map_err(|_| WireError::InvalidMessage)? != expected {
        return Err(WireError::InvalidMessage);
    }
    Ok(())
}

fn decode_fixed<const N: usize>(decoder: &mut Decoder<'_>) -> Result<[u8; N], WireError> {
    decoder
        .bytes()
        .map_err(|_| WireError::InvalidMessage)?
        .try_into()
        .map_err(|_| WireError::InvalidMessage)
}

fn finish_decode(decoder: &Decoder<'_>, encoded: &[u8]) -> Result<(), WireError> {
    if decoder.position() != encoded.len() {
        return Err(WireError::InvalidMessage);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_request() -> TransferRequest {
        TransferRequest::Open(OpenRequest {
            protocol_version: 1,
            room_id: [1; 16],
            receiver_nonce: [2; 32],
            receiver_proof: [3; 32],
        })
    }

    #[test]
    fn open_round_trips_with_byte_strings() -> Result<(), WireError> {
        let request = open_request();
        let encoded = encode_request_body(&request)?;
        assert_eq!(decode_request_body(&encoded)?, request);
        Ok(())
    }

    #[test]
    fn trailing_and_indefinite_data_are_rejected() -> Result<(), WireError> {
        let mut encoded = encode_request_body(&open_request())?;
        encoded.push(0);
        assert_eq!(
            decode_request_body(&encoded),
            Err(WireError::InvalidMessage)
        );
        assert_eq!(
            decode_request_body(&[0x9f, 0xff]),
            Err(WireError::InvalidMessage)
        );
        Ok(())
    }

    #[test]
    fn oversized_offer_is_rejected_before_encoding() {
        let response = TransferResponse::Offer(OfferResponse {
            protocol_version: 1,
            claim_id: [0; 16],
            sender_nonce: [0; 32],
            aead_nonce: [0; 24],
            expires_at_unix_ms: 0,
            content_type: 0,
            plaintext_length: 0,
            ciphertext: vec![0; MAX_CIPHERTEXT_BYTES + 1],
            ciphertext_digest: [0; 32],
            sender_proof: [0; 32],
        });
        assert_eq!(
            encode_response_body(&response),
            Err(WireError::FrameTooLarge)
        );
    }
}
