//! Adversarial sender and receiver lifecycle tests.

use std::{
    sync::{Arc, Barrier, mpsc},
    thread,
    time::{Duration, Instant},
};

use app_core::{CoreError, OfferEntropy, ReceiverSession, SenderActor, SenderState};
use code::ShareCodeSecret;
use crypto::derive_root;
use protocol::{ContentType, ProtocolErrorCode, SecretEnvelope};

const NETWORK: &str = "public-v1";
const SENDER: &[u8] = b"sender-peer-id";
const RECEIVER_ONE: &[u8] = b"receiver-one-peer-id";
const RECEIVER_TWO: &[u8] = b"receiver-two-peer-id";

struct FixedEntropy {
    next: u8,
    fail_after: Option<usize>,
    calls: usize,
}

impl FixedEntropy {
    const fn working() -> Self {
        Self {
            next: 2,
            fail_after: None,
            calls: 0,
        }
    }

    const fn failing_immediately() -> Self {
        Self {
            next: 2,
            fail_after: Some(0),
            calls: 0,
        }
    }
}

impl OfferEntropy for FixedEntropy {
    fn fill(&mut self, destination: &mut [u8]) -> Result<(), CoreError> {
        if self.fail_after.is_some_and(|limit| self.calls >= limit) {
            return Err(CoreError::Internal);
        }
        destination.fill(self.next);
        self.next = self.next.wrapping_add(1);
        self.calls += 1;
        Ok(())
    }
}

fn envelope() -> Result<SecretEnvelope, protocol::EnvelopeError> {
    SecretEnvelope::new(
        ContentType::DotenvRaw,
        None,
        1_000,
        2_000,
        b"TOKEN=sentinel\n".to_vec(),
    )
}

fn receiver(peer_id: &[u8], nonce: u8) -> Result<ReceiverSession, CoreError> {
    let root =
        derive_root(&ShareCodeSecret::new([7; 20]), NETWORK).map_err(|_| CoreError::Internal)?;
    ReceiverSession::new(
        root,
        NETWORK.to_owned(),
        SENDER.to_vec(),
        peer_id.to_vec(),
        [nonce; 32],
    )
}

fn sender(now: Instant) -> Result<SenderActor, CoreError> {
    let root =
        derive_root(&ShareCodeSecret::new([7; 20]), NETWORK).map_err(|_| CoreError::Internal)?;
    SenderActor::new(
        root,
        NETWORK.to_owned(),
        SENDER.to_vec(),
        &envelope().map_err(|_| CoreError::Internal)?,
        now + Duration::from_mins(1),
        Duration::from_secs(10),
    )
}

#[test]
fn invalid_proof_does_not_claim_the_share() -> Result<(), CoreError> {
    let now = Instant::now();
    let mut sender = sender(now)?;
    let receiver = receiver(RECEIVER_ONE, 1)?;
    let mut open = receiver.open_request()?;
    open.receiver_proof[0] ^= 1;

    let rejected = sender.handle_open(RECEIVER_ONE, &open, now, &mut FixedEntropy::working());
    assert_eq!(rejected, Err(ProtocolErrorCode::NotFoundOrUnauthorized));
    assert_eq!(sender.state(), SenderState::Available);

    let valid = receiver.open_request()?;
    assert!(
        sender
            .handle_open(RECEIVER_ONE, &valid, now, &mut FixedEntropy::working())
            .is_ok()
    );
    assert_eq!(sender.state(), SenderState::Disclosed);
    Ok(())
}

#[test]
fn first_valid_receiver_wins_and_exact_retry_reuses_offer() -> Result<(), CoreError> {
    let now = Instant::now();
    let mut sender = sender(now)?;
    let first = receiver(RECEIVER_ONE, 1)?;
    let first_open = first.open_request()?;
    let mut entropy = FixedEntropy::working();
    let offer = sender
        .handle_open(RECEIVER_ONE, &first_open, now, &mut entropy)
        .map_err(|_| CoreError::Transfer)?;
    let retried = sender
        .handle_open(
            RECEIVER_ONE,
            &first_open,
            now + Duration::from_secs(1),
            &mut entropy,
        )
        .map_err(|_| CoreError::Transfer)?;
    assert_eq!(offer, retried);
    assert_eq!(entropy.calls, 3);

    let second = receiver(RECEIVER_TWO, 9)?;
    let second_open = second.open_request()?;
    assert_eq!(
        sender.handle_open(RECEIVER_TWO, &second_open, now, &mut entropy),
        Err(ProtocolErrorCode::ShareAlreadyClaimed)
    );
    Ok(())
}

#[test]
fn verified_offer_acknowledges_once_and_duplicate_is_idempotent() -> Result<(), CoreError> {
    let now = Instant::now();
    let mut sender = sender(now)?;
    let receiver = receiver(RECEIVER_ONE, 1)?;
    let open = receiver.open_request()?;
    let offer = sender
        .handle_open(RECEIVER_ONE, &open, now, &mut FixedEntropy::working())
        .map_err(|_| CoreError::Transfer)?;
    let verified = receiver.verify_offer(&offer)?;
    assert_eq!(verified.envelope().payload(), b"TOKEN=sentinel\n");

    let first = sender
        .handle_acknowledgement(RECEIVER_ONE, verified.acknowledgement())
        .map_err(|_| CoreError::Transfer)?;
    let duplicate = sender
        .handle_acknowledgement(RECEIVER_ONE, verified.acknowledgement())
        .map_err(|_| CoreError::Transfer)?;
    assert_eq!(first, duplicate);
    assert_eq!(sender.state(), SenderState::Consumed);
    Ok(())
}

#[test]
fn disclosed_share_never_reopens_after_acknowledgement_timeout() -> Result<(), CoreError> {
    let now = Instant::now();
    let mut sender = sender(now)?;
    let receiver = receiver(RECEIVER_ONE, 1)?;
    let open = receiver.open_request()?;
    assert!(
        sender
            .handle_open(RECEIVER_ONE, &open, now, &mut FixedEntropy::working())
            .is_ok()
    );
    sender.handle_timer(now + Duration::from_secs(11));
    assert_eq!(sender.state(), SenderState::DeliveryUnknown);
    assert_eq!(
        sender.handle_open(
            RECEIVER_ONE,
            &open,
            now + Duration::from_secs(12),
            &mut FixedEntropy::working()
        ),
        Err(ProtocolErrorCode::NotFoundOrUnauthorized)
    );
    Ok(())
}

#[test]
fn entropy_failure_after_valid_claim_fails_closed() -> Result<(), CoreError> {
    let now = Instant::now();
    let mut sender = sender(now)?;
    let receiver = receiver(RECEIVER_ONE, 1)?;
    let open = receiver.open_request()?;
    assert_eq!(
        sender.handle_open(
            RECEIVER_ONE,
            &open,
            now,
            &mut FixedEntropy::failing_immediately()
        ),
        Err(ProtocolErrorCode::InternalFailure)
    );
    assert_eq!(sender.state(), SenderState::FailedClosed);
    assert_eq!(
        sender.handle_open(RECEIVER_ONE, &open, now, &mut FixedEntropy::working()),
        Err(ProtocolErrorCode::NotFoundOrUnauthorized)
    );
    Ok(())
}

#[test]
fn tampering_any_authenticated_offer_field_is_rejected() -> Result<(), CoreError> {
    let now = Instant::now();
    let mut sender = sender(now)?;
    let receiver = receiver(RECEIVER_ONE, 1)?;
    let open = receiver.open_request()?;
    let offer = sender
        .handle_open(RECEIVER_ONE, &open, now, &mut FixedEntropy::working())
        .map_err(|_| CoreError::Transfer)?;

    let mut tampered = offer.clone();
    tampered.plaintext_length = tampered.plaintext_length.saturating_add(1);
    assert!(matches!(
        receiver.verify_offer(&tampered),
        Err(CoreError::Transfer)
    ));
    let mut tampered = offer;
    tampered.ciphertext[0] ^= 1;
    assert!(matches!(
        receiver.verify_offer(&tampered),
        Err(CoreError::Transfer)
    ));
    Ok(())
}

#[test]
fn concurrent_receivers_are_serialized_to_exactly_one_winner() -> Result<(), CoreError> {
    let now = Instant::now();
    let mut sender = sender(now)?;
    let barrier = Arc::new(Barrier::new(3));
    let (requests, incoming) = mpsc::channel();
    thread::scope(|scope| {
        for (peer_id, nonce) in [(RECEIVER_ONE, 1), (RECEIVER_TWO, 2)] {
            let requests = requests.clone();
            let barrier = Arc::clone(&barrier);
            scope.spawn(move || {
                barrier.wait();
                let request = receiver(peer_id, nonce).and_then(|session| session.open_request());
                let _ = requests.send((peer_id, request));
            });
        }
        barrier.wait();
    });
    drop(requests);

    let mut entropy = FixedEntropy::working();
    let mut winners = 0;
    let mut already_claimed = 0;
    for _ in 0..2 {
        let (peer_id, request) = incoming.recv().map_err(|_| CoreError::Internal)?;
        match sender.handle_open(peer_id, &request?, now, &mut entropy) {
            Ok(_) => winners += 1,
            Err(ProtocolErrorCode::ShareAlreadyClaimed) => already_claimed += 1,
            Err(_) => return Err(CoreError::Transfer),
        }
    }
    assert_eq!(winners, 1);
    assert_eq!(already_claimed, 1);
    assert_eq!(sender.state(), SenderState::Disclosed);
    Ok(())
}
