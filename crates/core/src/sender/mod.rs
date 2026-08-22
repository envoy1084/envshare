//! Single-owner sender lifecycle.

mod actor;
mod entropy;
mod state;

pub use actor::SenderActor;
pub use entropy::{OfferEntropy, OsEntropy};
pub use state::SenderState;
