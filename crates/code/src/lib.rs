//! Capability code generation and parsing for Envshare.

#![forbid(unsafe_code)]

/// Human-readable prefix for version 1 share codes.
pub const CODE_PREFIX: &str = "esh1";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_prefix_is_stable() {
        assert_eq!(CODE_PREFIX, "esh1");
    }
}
