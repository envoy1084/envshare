//! Purpose-specific secret keys and bounded protocol byte types.

use std::fmt;

use subtle::ConstantTimeEq;
use zeroize::{Zeroize, ZeroizeOnDrop};

macro_rules! secret_key_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Zeroize, ZeroizeOnDrop)]
        pub struct $name([u8; 32]);

        impl $name {
            pub(crate) const fn new(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }

            pub(crate) const fn expose(&self) -> &[u8; 32] {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!(stringify!($name), "([REDACTED])"))
            }
        }
    };
}

secret_key_type!(
    /// HMAC key proving possession of the capability.
    AuthenticationKey
);
secret_key_type!(
    /// Root material used only to derive one transfer session.
    SessionBaseKey
);
secret_key_type!(
    /// AEAD key used only for one payload claim.
    PayloadKey
);
secret_key_type!(
    /// HMAC key used only for one acknowledgement claim.
    AcknowledgementKey
);

macro_rules! metadata_type {
    ($(#[$meta:meta])* $name:ident, $size:expr) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Eq, PartialEq)]
        pub struct $name([u8; $size]);

        impl $name {
            /// Constructs the fixed-width protocol value.
            #[must_use]
            pub const fn new(bytes: [u8; $size]) -> Self {
                Self(bytes)
            }

            /// Returns the canonical bytes.
            #[must_use]
            pub const fn as_bytes(&self) -> &[u8; $size] {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!(stringify!($name), "([REDACTED])"))
            }
        }
    };
}

metadata_type!(
    /// Opaque 128-bit discovery identifier derived from the capability.
    RoomId,
    16
);
metadata_type!(
    /// Fresh receiver challenge generated once per receive command.
    ReceiverNonce,
    32
);
metadata_type!(
    /// Fresh sender challenge generated for the winning claim.
    SenderNonce,
    32
);
metadata_type!(
    /// Random identifier for the winning claim.
    ClaimId,
    16
);
metadata_type!(
    /// Unique XChaCha20-Poly1305 nonce for an encrypted offer.
    AeadNonce,
    24
);
metadata_type!(
    /// SHA-256 digest binding an AEAD nonce and ciphertext.
    CiphertextDigest,
    32
);

/// HMAC-SHA-256 authentication proof.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct AuthenticationProof([u8; 32]);

impl AuthenticationProof {
    pub(crate) const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the proof bytes for the wire representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Verifies a supplied proof without data-dependent early exit.
    #[must_use]
    pub fn verifies(&self, supplied: &[u8; 32]) -> bool {
        bool::from(self.0.ct_eq(supplied))
    }
}

impl fmt::Debug for AuthenticationProof {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AuthenticationProof([REDACTED])")
    }
}

/// Root capability derivation scoped to one discovery network.
pub struct DerivedRoot {
    pub(crate) room_id: RoomId,
    pub(crate) authentication_key: AuthenticationKey,
    pub(crate) session_base_key: SessionBaseKey,
}

impl DerivedRoot {
    /// Returns the non-secret room identifier used for discovery.
    #[must_use]
    pub const fn room_id(&self) -> RoomId {
        self.room_id
    }

    /// Borrows the capability authentication key.
    #[must_use]
    pub const fn authentication_key(&self) -> &AuthenticationKey {
        &self.authentication_key
    }

    /// Borrows the base key used to derive claim-specific session keys.
    #[must_use]
    pub const fn session_base_key(&self) -> &SessionBaseKey {
        &self.session_base_key
    }
}

impl fmt::Debug for DerivedRoot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DerivedRoot([REDACTED])")
    }
}

/// Claim-specific payload and acknowledgement keys.
pub struct SessionKeys {
    pub(crate) payload_key: PayloadKey,
    pub(crate) acknowledgement_key: AcknowledgementKey,
}

impl SessionKeys {
    /// Borrows the payload AEAD key.
    #[must_use]
    pub const fn payload_key(&self) -> &PayloadKey {
        &self.payload_key
    }

    /// Borrows the acknowledgement HMAC key.
    #[must_use]
    pub const fn acknowledgement_key(&self) -> &AcknowledgementKey {
        &self.acknowledgement_key
    }

    #[cfg(test)]
    pub(crate) fn keys_are_equal(&self) -> bool {
        bool::from(
            self.payload_key
                .expose()
                .ct_eq(self.acknowledgement_key.expose()),
        )
    }
}

impl fmt::Debug for SessionKeys {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SessionKeys([REDACTED])")
    }
}

/// Authenticated connection identities and capability routing context.
#[derive(Clone, Copy)]
pub struct PeerContext<'a> {
    /// Stable public network identifier.
    pub network_id: &'a str,
    /// Capability-derived room identifier.
    pub room_id: RoomId,
    /// Canonical bytes of the authenticated sender Peer ID.
    pub sender_peer_id: &'a [u8],
    /// Canonical bytes of the authenticated receiver Peer ID.
    pub receiver_peer_id: &'a [u8],
}

/// Fields authenticated by an Offer proof.
#[derive(Clone, Copy)]
pub struct OfferProofInput<'a> {
    /// Common authenticated peer context.
    pub context: PeerContext<'a>,
    /// Receiver challenge from Open.
    pub receiver_nonce: ReceiverNonce,
    /// Sender challenge generated for the claim.
    pub sender_nonce: SenderNonce,
    /// Winning claim identifier.
    pub claim_id: ClaimId,
    /// Sender-authoritative expiry timestamp for metadata.
    pub expires_at_unix_ms: u64,
    /// Stable wire content-type discriminant.
    pub content_type: u8,
    /// Authenticated plaintext byte length.
    pub plaintext_length: u32,
    /// Unique AEAD nonce.
    pub aead_nonce: AeadNonce,
    /// Digest of AEAD nonce and ciphertext.
    pub ciphertext_digest: CiphertextDigest,
}
