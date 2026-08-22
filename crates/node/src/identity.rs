//! Stable Ed25519 node identity persistence.

use std::path::Path;

use app_core::{PrivateOutputOptions, read_bounded, write_private_atomic};
use libp2p::identity::{KeyType, Keypair};
use zeroize::Zeroizing;

use crate::NodeError;

const MAX_IDENTITY_BYTES: usize = 4 * 1024;

/// Generates a fresh stable Ed25519 node identity.
#[must_use]
pub fn generate_identity() -> Keypair {
    Keypair::generate_ed25519()
}

/// Loads and validates a protobuf-encoded Ed25519 node identity.
///
/// # Errors
///
/// Returns a safe identity failure for I/O, size, encoding, or key-type errors.
pub fn load_identity(path: &Path) -> Result<Keypair, NodeError> {
    let file = std::fs::File::open(path).map_err(|_| NodeError::Identity)?;
    let encoded =
        Zeroizing::new(read_bounded(file, MAX_IDENTITY_BYTES).map_err(|_| NodeError::Identity)?);
    let keypair = Keypair::from_protobuf_encoding(&encoded).map_err(|_| NodeError::Identity)?;
    if keypair.key_type() != KeyType::Ed25519 {
        return Err(NodeError::Identity);
    }
    Ok(keypair)
}

/// Persists an Ed25519 identity through private atomic output.
///
/// # Errors
///
/// Returns a safe identity failure for a non-Ed25519 key, encoding, permission,
/// no-clobber, flush, or persistence failure.
pub fn save_identity(path: &Path, keypair: &Keypair, replace: bool) -> Result<(), NodeError> {
    if keypair.key_type() != KeyType::Ed25519 {
        return Err(NodeError::Identity);
    }
    let encoded = Zeroizing::new(
        keypair
            .to_protobuf_encoding()
            .map_err(|_| NodeError::Identity)?,
    );
    write_private_atomic(
        path,
        &encoded,
        PrivateOutputOptions {
            replace,
            durable: true,
        },
    )
    .map_err(|_| NodeError::Identity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_identity_round_trips_and_refuses_clobber() -> Result<(), NodeError> {
        let directory = tempfile::tempdir().map_err(|_| NodeError::Identity)?;
        let path = directory.path().join("identity.key");
        let original = generate_identity();
        save_identity(&path, &original, false)?;
        assert!(save_identity(&path, &generate_identity(), false).is_err());
        let loaded = load_identity(&path)?;
        assert_eq!(original.public().to_peer_id(), loaded.public().to_peer_id());
        Ok(())
    }
}
