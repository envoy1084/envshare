//! Direct-address sender and receiver workflows.

use std::time::Duration;

use network::{Multiaddr, NetworkClient, NetworkEvent, PeerId};
use protocol::{
    PROTOCOL_VERSION, ProtocolErrorCode, ProtocolErrorResponse, SecretEnvelope, TransferRequest,
    TransferResponse,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{CoreError, OsEntropy, ReceiverSession, SenderActor, SenderState, VerifiedOffer};

/// Runs one sender actor against inbound direct transfer streams.
pub struct DirectSender {
    client: NetworkClient,
    events: mpsc::Receiver<NetworkEvent>,
    actor: SenderActor,
}

impl DirectSender {
    /// Creates a direct sender from the network owner's event stream.
    #[must_use]
    pub const fn new(
        client: NetworkClient,
        events: mpsc::Receiver<NetworkEvent>,
        actor: SenderActor,
    ) -> Self {
        Self {
            client,
            events,
            actor,
        }
    }

    /// Serves requests until the actor reaches a terminal state or cancellation.
    ///
    /// # Errors
    ///
    /// Returns when the bounded network owner stops or cannot send a response.
    pub async fn run(mut self, cancellation: CancellationToken) -> Result<SenderState, CoreError> {
        let mut timer = tokio::time::interval(Duration::from_millis(250));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                () = cancellation.cancelled() => return Ok(self.actor.state()),
                _ = timer.tick() => self.actor.handle_timer(std::time::Instant::now()),
                event = self.events.recv() => {
                    let event = event.ok_or(CoreError::Network)?;
                    self.handle_event(event).await?;
                }
            }
            if matches!(
                self.actor.state(),
                SenderState::Consumed
                    | SenderState::Expired
                    | SenderState::DeliveryUnknown
                    | SenderState::FailedClosed
            ) {
                return Ok(self.actor.state());
            }
        }
    }

    async fn handle_event(&mut self, event: NetworkEvent) -> Result<(), CoreError> {
        let NetworkEvent::InboundRequest {
            peer,
            request_id,
            request,
        } = event
        else {
            return Ok(());
        };
        let peer_bytes = peer.to_bytes();
        let response = match request {
            TransferRequest::Open(open) => {
                let mut entropy = OsEntropy;
                match self.actor.handle_open(
                    &peer_bytes,
                    &open,
                    std::time::Instant::now(),
                    &mut entropy,
                ) {
                    Ok(offer) => TransferResponse::Offer(offer),
                    Err(code) => protocol_error(code),
                }
            }
            TransferRequest::Acknowledge(acknowledgement) => {
                match self
                    .actor
                    .handle_acknowledgement(&peer_bytes, &acknowledgement)
                {
                    Ok(completed) => TransferResponse::Completed(completed),
                    Err(code) => protocol_error(code),
                }
            }
        };
        self.client
            .respond(request_id, response)
            .await
            .map_err(|_| CoreError::Network)
    }
}

/// Direct receiver bound to one explicit authenticated sender address.
pub struct DirectReceiver {
    client: NetworkClient,
    session: ReceiverSession,
    sender_peer: PeerId,
    sender_address: Multiaddr,
}

impl DirectReceiver {
    /// Creates a direct receiver route.
    #[must_use]
    pub const fn new(
        client: NetworkClient,
        session: ReceiverSession,
        sender_peer: PeerId,
        sender_address: Multiaddr,
    ) -> Self {
        Self {
            client,
            session,
            sender_peer,
            sender_address,
        }
    }

    /// Opens and authenticates an encrypted offer without acknowledging it.
    ///
    /// # Errors
    ///
    /// Returns a safe discovery, availability, network, or transfer failure.
    pub async fn receive(self) -> Result<PendingDirectOffer, CoreError> {
        let open = self.session.open_request()?;
        let response = self
            .client
            .request(
                self.sender_peer,
                self.sender_address.clone(),
                TransferRequest::Open(open),
            )
            .await
            .map_err(|_| CoreError::Network)?;
        let offer = match response {
            TransferResponse::Offer(offer) => offer,
            TransferResponse::Error(error) => return Err(map_protocol_error(error.code)),
            TransferResponse::Completed(_) => return Err(CoreError::Transfer),
        };
        let verified = self.session.verify_offer(&offer)?;
        Ok(PendingDirectOffer {
            client: self.client,
            sender_peer: self.sender_peer,
            sender_address: self.sender_address,
            verified,
        })
    }
}

/// Verified offer awaiting caller-confirmed safe output handling.
pub struct PendingDirectOffer {
    client: NetworkClient,
    sender_peer: PeerId,
    sender_address: Multiaddr,
    verified: VerifiedOffer,
}

impl PendingDirectOffer {
    /// Borrows the authenticated envelope for safe local persistence or execution.
    #[must_use]
    pub const fn envelope(&self) -> &SecretEnvelope {
        self.verified.envelope()
    }

    /// Sends the acknowledgement and returns the envelope only after the sender
    /// confirms consumption.
    ///
    /// # Errors
    ///
    /// Returns a safe network, claim, or protocol failure.
    pub async fn acknowledge(self) -> Result<SecretEnvelope, CoreError> {
        let (envelope, acknowledgement) = self.verified.into_parts();
        let expected_claim = acknowledgement.claim_id;
        let response = self
            .client
            .request(
                self.sender_peer,
                self.sender_address,
                TransferRequest::Acknowledge(acknowledgement),
            )
            .await
            .map_err(|_| CoreError::Network)?;
        match response {
            TransferResponse::Completed(completed)
                if completed.protocol_version == PROTOCOL_VERSION
                    && completed.claim_id == expected_claim =>
            {
                Ok(envelope)
            }
            TransferResponse::Error(error) => Err(map_protocol_error(error.code)),
            TransferResponse::Offer(_) | TransferResponse::Completed(_) => Err(CoreError::Transfer),
        }
    }
}

fn protocol_error(code: ProtocolErrorCode) -> TransferResponse {
    TransferResponse::Error(ProtocolErrorResponse {
        protocol_version: PROTOCOL_VERSION,
        code,
    })
}

fn map_protocol_error(code: ProtocolErrorCode) -> CoreError {
    match code {
        ProtocolErrorCode::NotFoundOrUnauthorized => CoreError::NotFoundOrUnauthorized,
        ProtocolErrorCode::ShareUnavailable
        | ProtocolErrorCode::ShareExpired
        | ProtocolErrorCode::ShareAlreadyClaimed
        | ProtocolErrorCode::ClaimMismatch => CoreError::ShareUnavailable,
        ProtocolErrorCode::TemporarilyUnavailable => CoreError::Network,
        ProtocolErrorCode::UnsupportedVersion
        | ProtocolErrorCode::InvalidMessage
        | ProtocolErrorCode::PayloadTooLarge
        | ProtocolErrorCode::InternalFailure => CoreError::Transfer,
    }
}

impl std::fmt::Debug for DirectSender {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DirectSender")
            .field("actor", &self.actor)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for DirectReceiver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("DirectReceiver([REDACTED])")
    }
}

impl std::fmt::Debug for PendingDirectOffer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PendingDirectOffer([REDACTED])")
    }
}
