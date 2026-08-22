//! Capability code generation and parsing for Envshare.

#![forbid(unsafe_code)]

mod alphabet;
mod checksum;
mod error;
mod secret;

use std::{fmt, fmt::Write as _, str::FromStr};

pub use error::{GenerateCodeError, ParseCodeError};
pub use secret::ShareCodeSecret;

use crate::{
    alphabet::{decode_secret, decode_symbol, encode_secret, encode_symbol},
    checksum::checksum_symbols,
};

/// Human-readable prefix for version 1 share codes.
pub const CODE_PREFIX: &str = "esh1";
/// Number of random bytes in a v1 capability.
pub const SECRET_BYTES: usize = 20;
/// Number of Base32 symbols encoding the secret.
pub const SECRET_SYMBOLS: usize = 32;
/// Number of checksum symbols.
pub const CHECKSUM_SYMBOLS: usize = 2;
const DATA_SYMBOLS: usize = SECRET_SYMBOLS + CHECKSUM_SYMBOLS;
const NORMALIZED_SYMBOLS: usize = CODE_PREFIX.len() + DATA_SYMBOLS;

/// A validated v1 share code owning its capability secret.
pub struct ShareCode {
    secret: ShareCodeSecret,
}

impl ShareCode {
    /// Generates a new capability from the operating-system CSPRNG.
    ///
    /// # Errors
    ///
    /// Returns [`GenerateCodeError::EntropyUnavailable`] if the operating system
    /// cannot provide cryptographically secure random bytes.
    pub fn generate() -> Result<Self, GenerateCodeError> {
        let mut bytes = [0_u8; SECRET_BYTES];
        getrandom::fill(&mut bytes).map_err(|_| GenerateCodeError::EntropyUnavailable)?;
        Ok(Self::from_secret(ShareCodeSecret::new(bytes)))
    }

    /// Constructs a displayable code from owned secret bytes.
    #[must_use]
    pub const fn from_secret(secret: ShareCodeSecret) -> Self {
        Self { secret }
    }

    /// Borrows the validated capability secret for key derivation.
    #[must_use]
    pub const fn secret(&self) -> &ShareCodeSecret {
        &self.secret
    }

    fn symbols(&self) -> [u8; DATA_SYMBOLS] {
        let mut symbols = [0_u8; DATA_SYMBOLS];
        let encoded = encode_secret(self.secret.expose_secret());
        for (output, value) in symbols.iter_mut().zip(encoded) {
            *output = encode_symbol(value);
        }
        let checksum = checksum_symbols(self.secret.expose_secret());
        symbols[SECRET_SYMBOLS] = encode_symbol(checksum[0]);
        symbols[SECRET_SYMBOLS + 1] = encode_symbol(checksum[1]);
        symbols
    }
}

impl FromStr for ShareCode {
    type Err = ParseCodeError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut normalized = [0_u8; NORMALIZED_SYMBOLS];
        let mut normalized_len = 0_usize;

        for byte in input.bytes() {
            if byte == b'-' {
                continue;
            }
            if !byte.is_ascii() || normalized_len == NORMALIZED_SYMBOLS {
                return Err(ParseCodeError::Invalid);
            }
            normalized[normalized_len] = byte.to_ascii_uppercase();
            normalized_len += 1;
        }

        if normalized_len != NORMALIZED_SYMBOLS || &normalized[..CODE_PREFIX.len()] != b"ESH1" {
            return Err(ParseCodeError::Invalid);
        }

        let encoded = &normalized[CODE_PREFIX.len()..];
        let mut secret_symbols = [0_u8; SECRET_SYMBOLS];
        for (output, symbol) in secret_symbols.iter_mut().zip(&encoded[..SECRET_SYMBOLS]) {
            *output = decode_symbol(*symbol).ok_or(ParseCodeError::Invalid)?;
        }

        let secret_bytes = decode_secret(secret_symbols);
        let supplied_checksum = [
            decode_symbol(encoded[SECRET_SYMBOLS]).ok_or(ParseCodeError::Invalid)?,
            decode_symbol(encoded[SECRET_SYMBOLS + 1]).ok_or(ParseCodeError::Invalid)?,
        ];
        if !checksum::checksum_matches(&secret_bytes, supplied_checksum) {
            return Err(ParseCodeError::Invalid);
        }

        Ok(Self::from_secret(ShareCodeSecret::new(secret_bytes)))
    }
}

impl fmt::Display for ShareCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(CODE_PREFIX)?;
        for (index, symbol) in self.symbols().iter().enumerate() {
            if index % 4 == 0 {
                formatter.write_char('-')?;
            }
            formatter.write_char(char::from(*symbol))?;
        }
        Ok(())
    }
}

impl fmt::Debug for ShareCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ShareCode([REDACTED])")
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn v1_prefix_is_stable() {
        assert_eq!(CODE_PREFIX, "esh1");
    }

    #[test]
    fn debug_is_redacted() {
        let code = ShareCode::from_secret(ShareCodeSecret::new([0xAB; SECRET_BYTES]));
        assert_eq!(format!("{code:?}"), "ShareCode([REDACTED])");
        assert!(!format!("{code:?}").contains("AB"));
    }

    #[test]
    fn canonical_zero_secret_matches_the_golden_vector() {
        let code = ShareCode::from_secret(ShareCodeSecret::new([0; SECRET_BYTES]));
        assert_eq!(
            code.to_string(),
            "esh1-0000-0000-0000-0000-0000-0000-0000-0000-QR"
        );
    }

    #[test]
    fn parser_accepts_case_separators_and_crockford_aliases() -> Result<(), ParseCodeError> {
        let code = ShareCode::from_secret(ShareCodeSecret::new([0; SECRET_BYTES])).to_string();
        let aliased = code.to_lowercase().replace('0', "o");
        let parsed: ShareCode = aliased.parse()?;
        assert!(
            parsed
                .secret()
                .ct_eq(&ShareCodeSecret::new([0; SECRET_BYTES]))
        );
        Ok(())
    }

    #[test]
    fn parser_returns_one_generic_error() {
        for invalid in ["", "esh2-0000", "esh1-not-valid", "esh1-💥"] {
            assert_eq!(
                invalid.parse::<ShareCode>().err(),
                Some(ParseCodeError::Invalid)
            );
        }
    }

    proptest! {
        #[test]
        fn every_secret_round_trips(bytes in any::<[u8; SECRET_BYTES]>()) {
            let code = ShareCode::from_secret(ShareCodeSecret::new(bytes));
            let encoded = code.to_string();
            let parsed: Result<ShareCode, _> = encoded.parse();
            prop_assert_eq!(
                parsed.as_ref().map(|parsed| parsed.secret().ct_eq(&ShareCodeSecret::new(bytes))),
                Ok(true),
            );
        }

        #[test]
        fn a_changed_checksum_is_rejected(bytes in any::<[u8; SECRET_BYTES]>()) {
            let code = ShareCode::from_secret(ShareCodeSecret::new(bytes));
            let mut encoded = code.to_string().into_bytes();
            if let Some(last) = encoded.last_mut() {
                *last = if *last == b'0' { b'1' } else { b'0' };
            }
            let changed = String::from_utf8_lossy(&encoded);
            prop_assert_eq!(changed.parse::<ShareCode>().err(), Some(ParseCodeError::Invalid));
        }
    }
}
