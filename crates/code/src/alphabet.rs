//! Crockford Base32 primitives for the fixed-width v1 secret.

use crate::{SECRET_BYTES, SECRET_SYMBOLS};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

pub(crate) const fn encode_symbol(value: u8) -> u8 {
    ALPHABET[value as usize]
}

pub(crate) const fn decode_symbol(symbol: u8) -> Option<u8> {
    let uppercase = symbol.to_ascii_uppercase();
    match uppercase {
        b'0' | b'O' => Some(0),
        b'1' | b'I' | b'L' => Some(1),
        b'2'..=b'9' => Some(uppercase - b'0'),
        b'A'..=b'H' => Some(uppercase - b'A' + 10),
        b'J'..=b'K' => Some(uppercase - b'J' + 18),
        b'M'..=b'N' => Some(uppercase - b'M' + 20),
        b'P'..=b'T' => Some(uppercase - b'P' + 22),
        b'V'..=b'Z' => Some(uppercase - b'V' + 27),
        _ => None,
    }
}

pub(crate) fn encode_secret(secret: &[u8; SECRET_BYTES]) -> [u8; SECRET_SYMBOLS] {
    let mut output = [0_u8; SECRET_SYMBOLS];
    let mut accumulator = 0_u16;
    let mut available_bits = 0_u8;
    let mut output_index = 0_usize;

    for byte in secret {
        accumulator = (accumulator << 8) | u16::from(*byte);
        available_bits += 8;
        while available_bits >= 5 {
            available_bits -= 5;
            output[output_index] = low_byte(accumulator >> available_bits) & 0x1f;
            output_index += 1;
        }
    }

    debug_assert_eq!(available_bits, 0);
    debug_assert_eq!(output_index, SECRET_SYMBOLS);
    output
}

pub(crate) fn decode_secret(symbols: [u8; SECRET_SYMBOLS]) -> [u8; SECRET_BYTES] {
    let mut output = [0_u8; SECRET_BYTES];
    let mut accumulator = 0_u16;
    let mut available_bits = 0_u8;
    let mut output_index = 0_usize;

    for symbol in symbols {
        accumulator = (accumulator << 5) | u16::from(symbol);
        available_bits += 5;
        if available_bits >= 8 {
            available_bits -= 8;
            output[output_index] = low_byte(accumulator >> available_bits);
            output_index += 1;
        }
    }

    debug_assert_eq!(available_bits, 0);
    debug_assert_eq!(output_index, SECRET_BYTES);
    output
}

const fn low_byte(value: u16) -> u8 {
    value.to_be_bytes()[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alphabet_omits_ambiguous_letters() {
        assert!(!ALPHABET.contains(&b'I'));
        assert!(!ALPHABET.contains(&b'L'));
        assert!(!ALPHABET.contains(&b'O'));
        assert!(!ALPHABET.contains(&b'U'));
    }

    #[test]
    fn aliases_decode_to_their_canonical_values() {
        assert_eq!(decode_symbol(b'O'), decode_symbol(b'0'));
        assert_eq!(decode_symbol(b'I'), decode_symbol(b'1'));
        assert_eq!(decode_symbol(b'L'), decode_symbol(b'1'));
    }
}
