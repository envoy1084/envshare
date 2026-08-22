//! Fixed-width bounded framing for transfer messages.

use crate::{
    MAX_ACK_BODY_BYTES, MAX_OFFER_BODY_BYTES, MAX_OPEN_BODY_BYTES, TransferRequest,
    TransferResponse, WireError,
    message::{
        decode_request_body, decode_response_body, encode_request_body, encode_response_body,
    },
};

const HEADER_BYTES: usize = 4;

/// Parses the unsigned big-endian body length from a four-byte frame header.
///
/// This function does not allocate. Callers must compare its result with the
/// request or response hard limit before allocating a body buffer.
///
/// # Errors
///
/// Returns [`WireError::IncompleteHeader`] or [`WireError::EmptyFrame`] for an
/// invalid header.
pub fn parse_frame_length(header: &[u8]) -> Result<usize, WireError> {
    let bytes: [u8; HEADER_BYTES] = header
        .get(..HEADER_BYTES)
        .ok_or(WireError::IncompleteHeader)?
        .try_into()
        .map_err(|_| WireError::IncompleteHeader)?;
    let length =
        usize::try_from(u32::from_be_bytes(bytes)).map_err(|_| WireError::FrameTooLarge)?;
    if length == 0 {
        return Err(WireError::EmptyFrame);
    }
    Ok(length)
}

/// Encodes a complete request frame.
///
/// # Errors
///
/// Returns a bounded wire error if the request cannot be represented canonically.
pub fn encode_request_frame(request: &TransferRequest) -> Result<Vec<u8>, WireError> {
    let body = encode_request_body(request)?;
    encode_frame(&body)
}

/// Decodes one complete request frame with no trailing bytes.
///
/// # Errors
///
/// Returns before body decoding when the declared request length exceeds its hard
/// limit.
pub fn decode_request_frame(frame: &[u8]) -> Result<TransferRequest, WireError> {
    let body = checked_body(frame, MAX_OPEN_BODY_BYTES.max(MAX_ACK_BODY_BYTES))?;
    decode_request_body(body)
}

/// Encodes a complete response frame.
///
/// # Errors
///
/// Returns a bounded wire error if the response cannot be represented canonically.
pub fn encode_response_frame(response: &TransferResponse) -> Result<Vec<u8>, WireError> {
    let body = encode_response_body(response)?;
    encode_frame(&body)
}

/// Decodes one complete response frame with no trailing bytes.
///
/// # Errors
///
/// Returns before body decoding when the declared response length exceeds the
/// maximum Offer response bound.
pub fn decode_response_frame(frame: &[u8]) -> Result<TransferResponse, WireError> {
    let body = checked_body(frame, MAX_OFFER_BODY_BYTES)?;
    decode_response_body(body)
}

fn encode_frame(body: &[u8]) -> Result<Vec<u8>, WireError> {
    if body.is_empty() {
        return Err(WireError::EmptyFrame);
    }
    let length = u32::try_from(body.len()).map_err(|_| WireError::FrameTooLarge)?;
    let mut frame = Vec::with_capacity(HEADER_BYTES + body.len());
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(body);
    Ok(frame)
}

fn checked_body(frame: &[u8], limit: usize) -> Result<&[u8], WireError> {
    let declared = parse_frame_length(frame)?;
    if declared > limit {
        return Err(WireError::FrameTooLarge);
    }
    let expected = HEADER_BYTES
        .checked_add(declared)
        .ok_or(WireError::FrameTooLarge)?;
    if frame.len() != expected {
        return Err(WireError::LengthMismatch);
    }
    Ok(&frame[HEADER_BYTES..])
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::{CompletedResponse, OpenRequest};

    #[test]
    fn declared_oversize_is_rejected_with_header_only() {
        let declared = u32::try_from(MAX_OPEN_BODY_BYTES + 1).unwrap_or(u32::MAX);
        assert_eq!(
            decode_request_frame(&declared.to_be_bytes()),
            Err(WireError::FrameTooLarge)
        );
    }

    #[test]
    fn length_mismatch_and_trailing_bytes_are_rejected() -> Result<(), WireError> {
        let request = TransferRequest::Open(OpenRequest {
            protocol_version: 1,
            room_id: [1; 16],
            receiver_nonce: [2; 32],
            receiver_proof: [3; 32],
        });
        let mut frame = encode_request_frame(&request)?;
        frame.push(0);
        assert_eq!(decode_request_frame(&frame), Err(WireError::LengthMismatch));
        Ok(())
    }

    proptest! {
        #[test]
        fn completed_response_round_trips(claim_id in any::<[u8; 16]>()) {
            let response = TransferResponse::Completed(CompletedResponse {
                protocol_version: 1,
                claim_id,
            });
            let frame = encode_response_frame(&response);
            prop_assert!(frame.is_ok());
            let decoded = frame.as_deref().map(decode_response_frame);
            prop_assert_eq!(decoded, Ok(Ok(response)));
        }

        #[test]
        fn arbitrary_short_input_never_decodes(bytes in proptest::collection::vec(any::<u8>(), 0..4)) {
            prop_assert!(decode_request_frame(&bytes).is_err());
            prop_assert!(decode_response_frame(&bytes).is_err());
        }
    }
}
