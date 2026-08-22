//! Versioned capability checksum.

use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::SECRET_BYTES;

const CHECKSUM_DOMAIN: &[u8] = b"envshare/code-checksum/v1";

pub(crate) fn checksum_symbols(secret: &[u8; SECRET_BYTES]) -> [u8; 2] {
    let digest = Sha256::new()
        .chain_update(CHECKSUM_DOMAIN)
        .chain_update(secret)
        .finalize();
    [digest[0] >> 3, ((digest[0] & 0x07) << 2) | (digest[1] >> 6)]
}

pub(crate) fn checksum_matches(secret: &[u8; SECRET_BYTES], supplied: [u8; 2]) -> bool {
    bool::from(checksum_symbols(secret).ct_eq(&supplied))
}
