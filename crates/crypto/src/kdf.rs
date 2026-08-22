//! Domain-separated root and claim-specific key derivation.

use code::ShareCodeSecret;
use hkdf::Hkdf;
use sha2::{Digest, Sha256};

use crate::{
    ClaimId, CryptoError, DerivedRoot, PeerContext, ReceiverNonce, RoomId, SenderNonce,
    SessionKeys, Transcript,
    types::{AcknowledgementKey, AuthenticationKey, PayloadKey, SessionBaseKey},
};

const ROOM_INFO_DOMAIN: &[u8] = b"envshare/room-id/v1";
const AUTH_INFO_DOMAIN: &[u8] = b"envshare/auth-key/v1";
const SESSION_BASE_INFO_DOMAIN: &[u8] = b"envshare/session-base-key/v1";
const SESSION_SALT_DOMAIN: &[u8] = b"envshare/session-salt/v1";
const PAYLOAD_KEY_DOMAIN: &[u8] = b"envshare/payload-key/v1";
const ACK_KEY_DOMAIN: &[u8] = b"envshare/ack-key/v1";

/// Derives discovery and root key material for one public network.
///
/// # Errors
///
/// Returns [`CryptoError::InvalidNetworkId`] when the network identifier is not
/// bounded printable ASCII, or [`CryptoError::KeyDerivation`] on impossible HKDF
/// output failure.
pub fn derive_root(secret: &ShareCodeSecret, network_id: &str) -> Result<DerivedRoot, CryptoError> {
    let network_id = validate_network_id(network_id)?;
    let root_salt = Sha256::digest(crate::ROOT_SALT_DOMAIN);
    let hkdf = Hkdf::<Sha256>::new(Some(&root_salt), secret.expose_secret());

    let room_id = RoomId::new(expand::<16>(
        &hkdf,
        &network_info(ROOM_INFO_DOMAIN, network_id)?,
    )?);
    let authentication_key = AuthenticationKey::new(expand::<32>(
        &hkdf,
        &network_info(AUTH_INFO_DOMAIN, network_id)?,
    )?);
    let session_base_key = SessionBaseKey::new(expand::<32>(
        &hkdf,
        &network_info(SESSION_BASE_INFO_DOMAIN, network_id)?,
    )?);

    Ok(DerivedRoot {
        room_id,
        authentication_key,
        session_base_key,
    })
}

/// Derives purpose-separated payload and acknowledgement keys for one claim.
///
/// # Errors
///
/// Returns a typed validation, transcript, or HKDF error when authenticated
/// context cannot be represented canonically.
pub fn derive_session(
    session_base_key: &SessionBaseKey,
    context: PeerContext<'_>,
    receiver_nonce: ReceiverNonce,
    sender_nonce: SenderNonce,
    claim_id: ClaimId,
) -> Result<SessionKeys, CryptoError> {
    validate_context(context)?;
    let session_salt = Sha256::new()
        .chain_update(SESSION_SALT_DOMAIN)
        .chain_update(receiver_nonce.as_bytes())
        .chain_update(sender_nonce.as_bytes())
        .finalize();
    let hkdf = Hkdf::<Sha256>::new(Some(&session_salt), session_base_key.expose());

    let payload_info = session_info(PAYLOAD_KEY_DOMAIN, context, claim_id)?;
    let acknowledgement_info = session_info(ACK_KEY_DOMAIN, context, claim_id)?;
    Ok(SessionKeys {
        payload_key: PayloadKey::new(expand::<32>(&hkdf, &payload_info)?),
        acknowledgement_key: AcknowledgementKey::new(expand::<32>(&hkdf, &acknowledgement_info)?),
    })
}

pub(crate) fn validate_context(context: PeerContext<'_>) -> Result<(), CryptoError> {
    validate_network_id(context.network_id)?;
    if context.sender_peer_id.is_empty() || context.receiver_peer_id.is_empty() {
        return Err(CryptoError::InvalidPeerIdentity);
    }
    Ok(())
}

pub(crate) fn validate_network_id(network_id: &str) -> Result<&[u8], CryptoError> {
    let bytes = network_id.as_bytes();
    if bytes.is_empty()
        || bytes.len() > protocol::MAX_NETWORK_ID_BYTES
        || !bytes.iter().all(u8::is_ascii_graphic)
    {
        return Err(CryptoError::InvalidNetworkId);
    }
    Ok(bytes)
}

fn network_info(domain: &[u8], network_id: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let length = u32::try_from(network_id.len()).map_err(|_| CryptoError::InvalidNetworkId)?;
    let mut info = Vec::with_capacity(domain.len() + 4 + network_id.len());
    info.extend_from_slice(domain);
    info.extend_from_slice(&length.to_be_bytes());
    info.extend_from_slice(network_id);
    Ok(info)
}

fn session_info(
    domain: &'static [u8],
    context: PeerContext<'_>,
    claim_id: ClaimId,
) -> Result<Vec<u8>, CryptoError> {
    let mut transcript = Transcript::new(domain)?;
    transcript.append_bytes(context.network_id.as_bytes())?;
    transcript.append_bytes(context.room_id.as_bytes())?;
    transcript.append_bytes(context.sender_peer_id)?;
    transcript.append_bytes(context.receiver_peer_id)?;
    transcript.append_bytes(claim_id.as_bytes())?;
    Ok(transcript.finish())
}

fn expand<const N: usize>(hkdf: &Hkdf<Sha256>, info: &[u8]) -> Result<[u8; N], CryptoError> {
    let mut output = [0_u8; N];
    hkdf.expand(info, &mut output)
        .map_err(|_| CryptoError::KeyDerivation)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use code::ShareCodeSecret;

    use super::*;

    #[test]
    fn zero_secret_matches_independent_golden_vector() -> Result<(), CryptoError> {
        let root = derive_root(&ShareCodeSecret::new([0; 20]), "public-v1")?;
        assert_eq!(
            root.room_id().as_bytes(),
            &[
                0x85, 0x1e, 0xaa, 0xa4, 0x8e, 0xfa, 0x61, 0x18, 0xd3, 0x38, 0x59, 0x2d, 0xdc, 0xef,
                0xa7, 0x54
            ]
        );
        assert_eq!(
            root.authentication_key.expose(),
            &[
                0x36, 0x30, 0x6f, 0xab, 0xfb, 0x5d, 0x7e, 0x79, 0x83, 0x47, 0x17, 0x02, 0x0c, 0x62,
                0x7f, 0x11, 0x3f, 0xbb, 0xc8, 0x15, 0xb9, 0xd3, 0x46, 0x5b, 0xd7, 0x50, 0x74, 0x2c,
                0xc0, 0x26, 0xf0, 0xd3
            ]
        );
        assert_eq!(
            root.session_base_key.expose(),
            &[
                0x5c, 0x2a, 0x7a, 0xa5, 0x62, 0xc1, 0x67, 0x07, 0xe4, 0x7b, 0x5e, 0x4d, 0x64, 0xe0,
                0x5a, 0x1b, 0x73, 0x30, 0x31, 0xb9, 0xc5, 0xdd, 0xe3, 0x25, 0x58, 0xa6, 0x89, 0x10,
                0x5c, 0xe1, 0x8c, 0xba
            ]
        );

        let context = PeerContext {
            network_id: "public-v1",
            room_id: root.room_id(),
            sender_peer_id: b"sender-peer-id",
            receiver_peer_id: b"receiver-peer-id",
        };
        let session = derive_session(
            root.session_base_key(),
            context,
            ReceiverNonce::new([1; 32]),
            SenderNonce::new([2; 32]),
            ClaimId::new([3; 16]),
        )?;
        assert_eq!(
            session.payload_key.expose(),
            &[
                0xc4, 0x31, 0xbf, 0x90, 0xfa, 0x26, 0x46, 0x11, 0x40, 0xff, 0x5d, 0x34, 0x60, 0x38,
                0x84, 0xe3, 0x5e, 0x06, 0x62, 0xd1, 0xab, 0xc4, 0xdf, 0xb6, 0x12, 0x1c, 0x4d, 0x03,
                0x59, 0xa9, 0x56, 0x38
            ]
        );
        assert_eq!(
            session.acknowledgement_key.expose(),
            &[
                0xc0, 0xe3, 0x00, 0xda, 0xf0, 0x4f, 0x1b, 0x56, 0xbe, 0x9b, 0x25, 0xda, 0xe5, 0x0c,
                0xaa, 0x25, 0x96, 0xd4, 0x0a, 0xe8, 0xda, 0xa5, 0x32, 0xee, 0x13, 0x0b, 0x6c, 0x1a,
                0x4e, 0x76, 0x15, 0x2f
            ]
        );
        Ok(())
    }

    #[test]
    fn network_identifier_is_strictly_bounded_ascii() {
        assert!(matches!(
            derive_root(&ShareCodeSecret::new([0; 20]), ""),
            Err(CryptoError::InvalidNetworkId)
        ));
        assert!(matches!(
            derive_root(&ShareCodeSecret::new([0; 20]), "contains space"),
            Err(CryptoError::InvalidNetworkId)
        ));
    }
}
