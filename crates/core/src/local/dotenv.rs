//! Dotenv parsing used only for normalized selection and child execution.

use std::{collections::BTreeMap, io::Cursor};

use zeroize::{Zeroize, Zeroizing};

use crate::CoreError;

/// Parsed environment with deterministic last-declaration-wins semantics.
pub struct ParsedEnvironment(BTreeMap<String, String>);

/// How received variables interact with an existing dotenv document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DotenvMergeMode {
    /// Received values replace matching declarations and missing keys are added.
    ReplaceExisting,
    /// Existing values are retained and only missing keys are added.
    KeepExisting,
}

/// Non-secret counts produced by a dotenv merge.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DotenvMergeSummary {
    /// Variables added to the document.
    pub added: usize,
    /// Existing effective declarations replaced with received values.
    pub updated: usize,
    /// Received variables skipped because the document already defined them.
    pub kept: usize,
}

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

/// Merges received dotenv variables into an existing document while preserving
/// unrelated declarations, standalone comments, blank lines, and line endings.
///
/// Existing duplicate declarations retain their earlier text; when replacement
/// is requested, only the final effective declaration is rewritten. Newly added
/// variables use a deterministic quoted representation.
///
/// # Errors
///
/// Returns a generic transfer error when either document is malformed or the
/// merged result exceeds the protocol payload bound.
pub fn merge_dotenv(
    existing: &[u8],
    received: &[u8],
    mode: DotenvMergeMode,
) -> Result<(Vec<u8>, DotenvMergeSummary), CoreError> {
    let existing_environment = ParsedEnvironment::parse(existing)?;
    let received_environment = ParsedEnvironment::parse(received)?;
    let newline = if existing.windows(2).any(|window| window == b"\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let text = std::str::from_utf8(existing).map_err(|_| CoreError::Transfer)?;
    let mut lines = text
        .split_inclusive('\n')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if !text.is_empty() && !text.ends_with('\n') && lines.is_empty() {
        lines.push(text.to_owned());
    }
    let mut final_declarations = BTreeMap::new();
    for (index, line) in lines.iter().enumerate() {
        if let Some(key) = declaration_key(line) {
            final_declarations.insert(key.to_owned(), index);
        }
    }

    let mut summary = DotenvMergeSummary::default();
    let mut additions = Vec::new();
    for (key, value) in received_environment.variables() {
        if existing_environment.variables().contains_key(key) {
            match mode {
                DotenvMergeMode::ReplaceExisting => {
                    let index = final_declarations.get(key).ok_or(CoreError::Transfer)?;
                    lines[*index] = normalized_declaration(key, value, newline);
                    summary.updated += 1;
                }
                DotenvMergeMode::KeepExisting => summary.kept += 1,
            }
        } else {
            additions.push(normalized_declaration(key, value, newline));
            summary.added += 1;
        }
    }

    let mut merged = lines.concat();
    if !additions.is_empty() {
        if !merged.is_empty() && !merged.ends_with('\n') {
            merged.push_str(newline);
        }
        merged.push_str(&additions.concat());
    }
    if merged.len() > protocol::MAX_PAYLOAD_BYTES {
        return Err(CoreError::Transfer);
    }
    Ok((merged.into_bytes(), summary))
}

fn declaration_key(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let declaration = trimmed
        .strip_prefix("export ")
        .map_or(trimmed, str::trim_start);
    let (key, _) = declaration.split_once('=')?;
    let key = key.trim();
    (!key.is_empty()).then_some(key)
}

fn normalized_declaration(key: &str, value: &str, newline: &str) -> String {
    let mut output = String::with_capacity(key.len() + value.len() + newline.len() + 4);
    output.push_str(key);
    output.push_str("=\"");
    escape_value(&mut output, value);
    output.push('"');
    output.push_str(newline);
    output
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

    #[test]
    fn merge_updates_effective_values_and_preserves_document_structure() -> Result<(), CoreError> {
        let existing = b"# database\r\nDATABASE_URL=old\r\n\r\nTOKEN=first\r\nTOKEN=effective\r\n";
        let received = b"DATABASE_URL=new\nTOKEN='new token'\nEXTRA=value\n";
        let (merged, summary) = merge_dotenv(existing, received, DotenvMergeMode::ReplaceExisting)?;

        assert_eq!(
            merged,
            b"# database\r\nDATABASE_URL=\"new\"\r\n\r\nTOKEN=first\r\nTOKEN=\"new token\"\r\nEXTRA=\"value\"\r\n"
        );
        assert_eq!(
            summary,
            DotenvMergeSummary {
                added: 1,
                updated: 2,
                kept: 0,
            }
        );
        Ok(())
    }

    #[test]
    fn append_missing_keeps_existing_values_and_adds_only_new_keys() -> Result<(), CoreError> {
        let (merged, summary) = merge_dotenv(
            b"TOKEN=local\n",
            b"TOKEN=received\nEXTRA=value\n",
            DotenvMergeMode::KeepExisting,
        )?;

        assert_eq!(merged, b"TOKEN=local\nEXTRA=\"value\"\n");
        assert_eq!(
            summary,
            DotenvMergeSummary {
                added: 1,
                updated: 0,
                kept: 1,
            }
        );
        Ok(())
    }
}
