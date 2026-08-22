//! Validated plaintext envelope encoded before application AEAD.

use std::fmt;

use minicbor::{Decoder, Encoder, data::Type};
use zeroize::Zeroize;

use crate::{MAX_ENVELOPE_BYTES, MAX_PAYLOAD_BYTES, MAX_SUGGESTED_NAME_BYTES};

/// Payload representation carried by an Envshare envelope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ContentType {
    /// Raw dotenv bytes preserving comments, quoting, ordering, and syntax.
    DotenvRaw = 0,
    /// Canonically reconstructed dotenv bytes after key selection.
    DotenvNormalized = 1,
}

impl TryFrom<u8> for ContentType {
    type Error = EnvelopeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::DotenvRaw),
            1 => Ok(Self::DotenvNormalized),
            _ => Err(EnvelopeError::InvalidEncoding),
        }
    }
}

/// Bounded sender-provided display name that is never used as a path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuggestedName(String);

impl SuggestedName {
    /// Validates display-only filename metadata.
    ///
    /// # Errors
    ///
    /// Returns [`EnvelopeError::InvalidSuggestedName`] for empty, oversized,
    /// control-character, or path-separator-bearing input.
    pub fn new(value: String) -> Result<Self, EnvelopeError> {
        if value.is_empty()
            || value.len() > MAX_SUGGESTED_NAME_BYTES
            || value.chars().any(char::is_control)
            || value.contains(['/', '\\'])
        {
            return Err(EnvelopeError::InvalidSuggestedName);
        }
        Ok(Self(value))
    }

    /// Returns the validated display metadata.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validated, secret-bearing envelope ready for AEAD.
pub struct SecretEnvelope {
    content_type: ContentType,
    suggested_name: Option<SuggestedName>,
    created_at_unix_ms: u64,
    expires_at_unix_ms: u64,
    payload: Vec<u8>,
}

impl SecretEnvelope {
    /// Constructs a bounded v1 envelope.
    ///
    /// # Errors
    ///
    /// Returns a typed envelope error when the payload is oversized or expiry
    /// precedes creation.
    pub fn new(
        content_type: ContentType,
        suggested_name: Option<SuggestedName>,
        created_at_unix_ms: u64,
        expires_at_unix_ms: u64,
        payload: Vec<u8>,
    ) -> Result<Self, EnvelopeError> {
        if payload.len() > MAX_PAYLOAD_BYTES {
            return Err(EnvelopeError::PayloadTooLarge);
        }
        if expires_at_unix_ms < created_at_unix_ms {
            return Err(EnvelopeError::InvalidTimestamps);
        }
        Ok(Self {
            content_type,
            suggested_name,
            created_at_unix_ms,
            expires_at_unix_ms,
            payload,
        })
    }

    /// Returns the payload representation.
    #[must_use]
    pub const fn content_type(&self) -> ContentType {
        self.content_type
    }

    /// Returns optional display-only name metadata.
    #[must_use]
    pub const fn suggested_name(&self) -> Option<&SuggestedName> {
        self.suggested_name.as_ref()
    }

    /// Returns the authenticated creation timestamp.
    #[must_use]
    pub const fn created_at_unix_ms(&self) -> u64 {
        self.created_at_unix_ms
    }

    /// Returns the authenticated sender expiry timestamp.
    #[must_use]
    pub const fn expires_at_unix_ms(&self) -> u64 {
        self.expires_at_unix_ms
    }

    /// Returns the secret payload bytes.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Transfers ownership of the secret payload to the receiver output boundary.
    #[must_use]
    pub fn into_payload(mut self) -> Vec<u8> {
        std::mem::take(&mut self.payload)
    }

    /// Encodes the strict v1 CBOR envelope.
    ///
    /// # Errors
    ///
    /// Returns [`EnvelopeError::EncodedTooLarge`] before returning an envelope
    /// above the authenticated envelope bound.
    pub fn encode(&self) -> Result<Vec<u8>, EnvelopeError> {
        let mut encoder = Encoder::new(Vec::with_capacity(self.payload.len() + 128));
        encoder.map(6).map_err(|_| EnvelopeError::InvalidEncoding)?;
        encoder
            .u8(0)
            .and_then(|encoder| encoder.u16(1))
            .map_err(|_| EnvelopeError::InvalidEncoding)?;
        encoder
            .u8(1)
            .and_then(|encoder| encoder.u8(self.content_type as u8))
            .map_err(|_| EnvelopeError::InvalidEncoding)?;
        encoder.u8(2).map_err(|_| EnvelopeError::InvalidEncoding)?;
        if let Some(name) = &self.suggested_name {
            encoder
                .str(name.as_str())
                .map_err(|_| EnvelopeError::InvalidEncoding)?;
        } else {
            encoder.null().map_err(|_| EnvelopeError::InvalidEncoding)?;
        }
        encoder
            .u8(3)
            .and_then(|encoder| encoder.u64(self.created_at_unix_ms))
            .and_then(|encoder| encoder.u8(4))
            .and_then(|encoder| encoder.u64(self.expires_at_unix_ms))
            .and_then(|encoder| encoder.u8(5))
            .and_then(|encoder| encoder.bytes(&self.payload))
            .map_err(|_| EnvelopeError::InvalidEncoding)?;
        let body = encoder.into_writer();
        if body.len() > MAX_ENVELOPE_BYTES {
            return Err(EnvelopeError::EncodedTooLarge);
        }
        Ok(body)
    }

    /// Decodes and validates a strict v1 CBOR envelope.
    ///
    /// # Errors
    ///
    /// Returns a generic encoding error for malformed, indefinite, unknown,
    /// reordered, or trailing data and a specific local limit error for payloads.
    pub fn decode(encoded: &[u8]) -> Result<Self, EnvelopeError> {
        if encoded.is_empty() || encoded.len() > MAX_ENVELOPE_BYTES {
            return Err(EnvelopeError::EncodedTooLarge);
        }
        let mut decoder = Decoder::new(encoded);
        if decoder.map().map_err(|_| EnvelopeError::InvalidEncoding)? != Some(6) {
            return Err(EnvelopeError::InvalidEncoding);
        }
        expect_key(&mut decoder, 0)?;
        if decoder.u16().map_err(|_| EnvelopeError::InvalidEncoding)? != 1 {
            return Err(EnvelopeError::InvalidEncoding);
        }
        expect_key(&mut decoder, 1)?;
        let content_type =
            ContentType::try_from(decoder.u8().map_err(|_| EnvelopeError::InvalidEncoding)?)?;
        expect_key(&mut decoder, 2)?;
        let suggested_name = match decoder
            .datatype()
            .map_err(|_| EnvelopeError::InvalidEncoding)?
        {
            Type::Null => {
                decoder.null().map_err(|_| EnvelopeError::InvalidEncoding)?;
                None
            }
            Type::String => Some(SuggestedName::new(
                decoder
                    .str()
                    .map_err(|_| EnvelopeError::InvalidEncoding)?
                    .to_owned(),
            )?),
            _ => return Err(EnvelopeError::InvalidEncoding),
        };
        expect_key(&mut decoder, 3)?;
        let created_at_unix_ms = decoder.u64().map_err(|_| EnvelopeError::InvalidEncoding)?;
        expect_key(&mut decoder, 4)?;
        let expires_at_unix_ms = decoder.u64().map_err(|_| EnvelopeError::InvalidEncoding)?;
        expect_key(&mut decoder, 5)?;
        let payload = decoder
            .bytes()
            .map_err(|_| EnvelopeError::InvalidEncoding)?;
        if payload.len() > MAX_PAYLOAD_BYTES {
            return Err(EnvelopeError::PayloadTooLarge);
        }
        if decoder.position() != encoded.len() {
            return Err(EnvelopeError::InvalidEncoding);
        }
        Self::new(
            content_type,
            suggested_name,
            created_at_unix_ms,
            expires_at_unix_ms,
            payload.to_vec(),
        )
    }
}

impl Drop for SecretEnvelope {
    fn drop(&mut self) {
        self.payload.zeroize();
    }
}

impl fmt::Debug for SecretEnvelope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretEnvelope")
            .field("content_type", &self.content_type)
            .field("payload", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

/// Envelope validation and strict decoding errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum EnvelopeError {
    /// Payload bytes exceed the v1 maximum.
    #[error("payload exceeds the protocol limit")]
    PayloadTooLarge,
    /// Suggested display metadata is unsafe or oversized.
    #[error("invalid suggested name")]
    InvalidSuggestedName,
    /// Authenticated timestamp ordering is invalid.
    #[error("invalid envelope timestamps")]
    InvalidTimestamps,
    /// The complete encoded envelope exceeds its hard bound.
    #[error("encoded envelope exceeds the protocol limit")]
    EncodedTooLarge,
    /// The CBOR representation is malformed or non-canonical.
    #[error("invalid encrypted envelope")]
    InvalidEncoding,
}

fn expect_key(decoder: &mut Decoder<'_>, expected: u8) -> Result<(), EnvelopeError> {
    if decoder.u8().map_err(|_| EnvelopeError::InvalidEncoding)? != expected {
        return Err(EnvelopeError::InvalidEncoding);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn debug_redacts_payload_and_name() -> Result<(), EnvelopeError> {
        let envelope = SecretEnvelope::new(
            ContentType::DotenvRaw,
            Some(SuggestedName::new("private.env".to_owned())?),
            1,
            2,
            b"SECRET=sentinel".to_vec(),
        )?;
        let debug = format!("{envelope:?}");
        assert!(!debug.contains("sentinel"));
        assert!(!debug.contains("private.env"));
        Ok(())
    }

    #[test]
    fn rejects_path_like_suggested_names() {
        for name in ["../secret", "folder/file", "folder\\file", "bad\nname"] {
            assert_eq!(
                SuggestedName::new(name.to_owned()),
                Err(EnvelopeError::InvalidSuggestedName)
            );
        }
    }

    proptest! {
        #[test]
        fn bounded_payload_round_trips(payload in proptest::collection::vec(any::<u8>(), 0..4096)) {
            let envelope = SecretEnvelope::new(ContentType::DotenvRaw, None, 10, 20, payload.clone());
            prop_assert!(envelope.is_ok());
            let encoded = envelope.and_then(|envelope| envelope.encode());
            prop_assert!(encoded.is_ok());
            let decoded = encoded.as_deref().map(SecretEnvelope::decode);
            prop_assert_eq!(
                decoded.map(|result| result.map(SecretEnvelope::into_payload)),
                Ok(Ok(payload)),
            );
        }
    }
}
