//! Envshare-specific, wire-compatible Rendezvous server hardening.

mod codec;
mod store;

use std::time::Duration;

use libp2p::{request_response, swarm::StreamProtocol};

pub(crate) use codec::{Request, Response};
pub(crate) use store::{Event, Handled, Store};

pub(crate) type Behaviour = request_response::Behaviour<codec::Codec>;
pub(crate) type ProtocolEvent = request_response::Event<Request, Response>;

pub(crate) fn behaviour() -> Behaviour {
    request_response::Behaviour::with_codec(
        codec::Codec,
        [(
            StreamProtocol::new("/rendezvous/1.0.0"),
            request_response::ProtocolSupport::Inbound,
        )],
        request_response::Config::default()
            .with_request_timeout(Duration::from_secs(10))
            .with_max_concurrent_streams(128),
    )
}
