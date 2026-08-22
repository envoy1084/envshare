//! Canonical, length-prefixed security transcripts.

use crate::CryptoError;

/// Canonical byte builder for HMAC, HKDF info, and AEAD associated data.
pub struct Transcript {
    bytes: Vec<u8>,
}

impl Transcript {
    /// Starts a transcript with a length-prefixed domain separator.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::TranscriptTooLarge`] if the domain cannot fit in
    /// the protocol's hard transcript bound.
    pub fn new(domain: &'static [u8]) -> Result<Self, CryptoError> {
        let mut transcript = Self {
            bytes: Vec::with_capacity(256),
        };
        transcript.append_bytes(domain)?;
        Ok(transcript)
    }

    /// Appends a fixed-width big-endian unsigned 16-bit integer.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::TranscriptTooLarge`] at the hard size limit.
    pub fn append_u16(&mut self, value: u16) -> Result<(), CryptoError> {
        self.append_fixed(&value.to_be_bytes())
    }

    /// Appends a fixed-width big-endian unsigned 32-bit integer.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::TranscriptTooLarge`] at the hard size limit.
    pub fn append_u32(&mut self, value: u32) -> Result<(), CryptoError> {
        self.append_fixed(&value.to_be_bytes())
    }

    /// Appends a fixed-width big-endian unsigned 64-bit integer.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::TranscriptTooLarge`] at the hard size limit.
    pub fn append_u64(&mut self, value: u64) -> Result<(), CryptoError> {
        self.append_fixed(&value.to_be_bytes())
    }

    /// Appends a four-byte length followed by a byte string.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::TranscriptTooLarge`] before exceeding the hard
    /// transcript bound or an encodable 32-bit field length.
    pub fn append_bytes(&mut self, value: &[u8]) -> Result<(), CryptoError> {
        let length = u32::try_from(value.len()).map_err(|_| CryptoError::TranscriptTooLarge)?;
        self.ensure_capacity(4_usize.saturating_add(value.len()))?;
        self.bytes.extend_from_slice(&length.to_be_bytes());
        self.bytes.extend_from_slice(value);
        Ok(())
    }

    /// Returns the canonical transcript bytes.
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn append_fixed(&mut self, value: &[u8]) -> Result<(), CryptoError> {
        self.ensure_capacity(value.len())?;
        self.bytes.extend_from_slice(value);
        Ok(())
    }

    fn ensure_capacity(&self, additional: usize) -> Result<(), CryptoError> {
        let length = self
            .bytes
            .len()
            .checked_add(additional)
            .ok_or(CryptoError::TranscriptTooLarge)?;
        if length > protocol::MAX_TRANSCRIPT_BYTES {
            return Err(CryptoError::TranscriptTooLarge);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_has_fixed_canonical_encoding() -> Result<(), CryptoError> {
        let mut transcript = Transcript::new(b"domain")?;
        transcript.append_u16(0x0102)?;
        transcript.append_bytes(b"abc")?;
        assert_eq!(
            transcript.finish(),
            b"\0\0\0\x06domain\x01\x02\0\0\0\x03abc"
        );
        Ok(())
    }

    #[test]
    fn transcript_rejects_oversize_before_append() -> Result<(), CryptoError> {
        let mut transcript = Transcript::new(b"domain")?;
        let oversized = vec![0_u8; protocol::MAX_TRANSCRIPT_BYTES];
        assert_eq!(
            transcript.append_bytes(&oversized),
            Err(CryptoError::TranscriptTooLarge)
        );
        Ok(())
    }
}
