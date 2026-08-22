//! Bounded synchronous input reading.

use std::io::Read;

use zeroize::Zeroize;

use crate::CoreError;

/// Reads at most `maximum_bytes`, checking one additional byte for overflow.
///
/// # Errors
///
/// Returns a safe input error for I/O failure, arithmetic overflow, or an input
/// larger than the configured bound.
pub fn read_bounded(reader: impl Read, maximum_bytes: usize) -> Result<Vec<u8>, CoreError> {
    if maximum_bytes == 0 {
        return Err(CoreError::Configuration);
    }
    let read_limit = maximum_bytes.checked_add(1).ok_or(CoreError::Internal)?;
    let mut bytes = Vec::with_capacity(maximum_bytes.min(64 * 1024));
    reader
        .take(u64::try_from(read_limit).map_err(|_| CoreError::Internal)?)
        .read_to_end(&mut bytes)
        .map_err(|_| CoreError::Output)?;
    if bytes.len() > maximum_bytes {
        bytes.zeroize();
        return Err(CoreError::Transfer);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn preserves_raw_bytes_and_rejects_one_byte_over_limit() -> Result<(), CoreError> {
        let raw = b"A='x'\r\n# preserved\r\n";
        assert_eq!(read_bounded(Cursor::new(raw), raw.len())?, raw);
        assert!(matches!(
            read_bounded(Cursor::new(raw), raw.len() - 1),
            Err(CoreError::Transfer)
        ));
        Ok(())
    }
}
