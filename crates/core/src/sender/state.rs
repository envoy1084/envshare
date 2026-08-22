//! Sender lifecycle states.

/// Observable state of a single-use share.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SenderState {
    /// The share can accept its first authenticated claim.
    Available,
    /// A valid claim won and an offer is being prepared.
    PreparingOffer,
    /// The encrypted offer was disclosed and can only be resumed by its winner.
    Disclosed,
    /// The winning receiver acknowledged safe handling.
    Consumed,
    /// No valid claim arrived before the sender deadline.
    Expired,
    /// An offer was disclosed but no acknowledgement arrived in time.
    DeliveryUnknown,
    /// Preparation failed after a claim won; the share remains permanently closed.
    FailedClosed,
}
