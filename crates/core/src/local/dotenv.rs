//! Dotenv parsing used only for normalized selection and child execution.

use std::{collections::BTreeMap, io::Cursor};

use zeroize::{Zeroize, Zeroizing};

use crate::CoreError;

/// Parsed environment with deterministic last-declaration-wins semantics.
pub struct ParsedEnvironment(BTreeMap<String, String>);

impl ParsedEnvironment {
    /// Parses bounded dotenv bytes without mutating the current process environment.
    ///
    /// # Errors
    ///
    /// Returns a generic transfer error without revealing key names or values.
    pub fn parse(payload: &[u8]) -> Result<Self, CoreError> {
        if payload.len() > protocol::MAX_PAYLOAD_BYTES {
            return Err(CoreError::Transfer);
        }
        let mut variables = BTreeMap::new();
        for entry in dotenvy::from_read_iter(Cursor::new(payload)) {
            let (key, value) = entry.map_err(|_| CoreError::Transfer)?;
            if key.contains('\0') || value.contains('\0') {
                return Err(CoreError::Transfer);
            }
            variables.insert(key, value);
        }
        Ok(Self(variables))
    }

    /// Returns the normalized environment map.
    #[must_use]
    pub const fn variables(&self) -> &BTreeMap<String, String> {
        &self.0
    }

    /// Returns true when any received name already exists in the inherited environment.
    #[must_use]
    pub fn conflicts_with_inherited(&self) -> bool {
        self.0.keys().any(|key| std::env::var_os(key).is_some())
    }
}

/// Selects requested keys and emits deterministic normalized dotenv bytes.
///
/// Duplicate declarations use their final value. Comments and source ordering
/// are intentionally removed. Output keys are sorted and the payload ends in a
/// newline.
///
/// # Errors
///
/// Returns a generic transfer error for malformed dotenv, duplicate requested
/// keys, or a missing key when `allow_missing` is false.
pub fn select_dotenv(
    payload: &[u8],
    requested_keys: &[String],
    allow_missing: bool,
) -> Result<Vec<u8>, CoreError> {
    if requested_keys.is_empty() {
        return Err(CoreError::Configuration);
    }
    let parsed = ParsedEnvironment::parse(payload)?;
    let mut selected = BTreeMap::new();
    for key in requested_keys {
        if selected.contains_key(key) {
            return Err(CoreError::Configuration);
        }
        if let Some(value) = parsed.variables().get(key) {
            selected.insert(key, value);
        } else if !allow_missing {
            return Err(CoreError::Transfer);
        }
    }
    let mut normalized = Zeroizing::new(String::new());
    for (key, value) in selected {
        normalized.push_str(key);
        normalized.push_str("=\"");
        escape_value(&mut normalized, value);
        normalized.push_str("\"\n");
    }
    if normalized.len() > protocol::MAX_PAYLOAD_BYTES {
        return Err(CoreError::Transfer);
    }
    Ok(normalized.as_bytes().to_vec())
}

fn escape_value(output: &mut String, value: &str) {
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '$' => output.push_str("\\$"),
            _ => output.push(character),
        }
    }
}

impl std::fmt::Debug for ParsedEnvironment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ParsedEnvironment")
            .field("variable_count", &self.0.len())
            .field("values", &"[REDACTED]")
            .finish()
    }
}

impl Drop for ParsedEnvironment {
    fn drop(&mut self) {
        for (mut key, mut value) in std::mem::take(&mut self.0) {
            key.zeroize();
            value.zeroize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_is_sorted_last_wins_and_round_trips_escaping() -> Result<(), CoreError> {
        let payload = b"B=old\nA='first value'\nB='line $dollar'\n";
        let selected = select_dotenv(payload, &["B".to_owned(), "A".to_owned()], false)?;
        assert!(selected.starts_with(b"A=\"first value\"\nB=\""));
        let reparsed = ParsedEnvironment::parse(&selected)?;
        assert_eq!(
            reparsed.variables().get("B").map(String::as_str),
            Some("line $dollar")
        );
        Ok(())
    }

    #[test]
    fn missing_keys_are_generic_and_optionally_ignored() -> Result<(), CoreError> {
        assert!(matches!(
            select_dotenv(b"A=1\n", &["MISSING".to_owned()], false),
            Err(CoreError::Transfer)
        ));
        assert_eq!(select_dotenv(b"A=1\n", &["MISSING".to_owned()], true)?, b"");
        Ok(())
    }

    #[test]
    fn inherited_conflicts_are_detected_without_exposing_values() -> Result<(), CoreError> {
        assert!(std::env::var_os("PATH").is_some());
        let parsed = ParsedEnvironment::parse(b"PATH=untrusted\n")?;

        assert!(parsed.conflicts_with_inherited());
        assert!(!format!("{parsed:?}").contains("untrusted"));
        Ok(())
    }
}
