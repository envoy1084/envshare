//! Cryptographic protocol primitives for Envshare.

#![forbid(unsafe_code)]

/// Domain separation label used by the v1 root key derivation.
pub const ROOT_SALT_DOMAIN: &[u8] = b"envshare/root-salt/v1";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_domain_is_explicitly_versioned() {
        assert!(ROOT_SALT_DOMAIN.ends_with(b"/v1"));
    }
}
