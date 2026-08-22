//! Bounded source-IP connection admission before cryptographic negotiation.

use std::{
    collections::HashMap,
    convert::Infallible,
    net::IpAddr,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use libp2p::{
    Multiaddr, PeerId,
    core::{Endpoint, transport::PortUse},
    multiaddr::Protocol,
    swarm::{
        ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
        THandlerOutEvent, ToSwarm, dummy,
    },
};

use crate::NodeConfig;

const RATE_WINDOW: Duration = Duration::from_mins(1);

#[derive(Clone, Copy)]
struct RateBucket {
    started: Instant,
    attempts: u32,
}

/// A secret-safe reason for rejecting transport admission.
#[derive(Debug, thiserror::Error)]
#[error("inbound connection admission limit reached")]
struct AdmissionDenied;

pub(crate) struct Behaviour {
    max_connections_per_ip: usize,
    attempts_per_minute: u32,
    max_rate_ips: usize,
    pending: HashMap<ConnectionId, IpAddr>,
    established: HashMap<ConnectionId, IpAddr>,
    rate: HashMap<IpAddr, RateBucket>,
}

impl Behaviour {
    pub(crate) fn new(config: &NodeConfig) -> Self {
        Self {
            max_connections_per_ip: config.max_connections_per_ip,
            attempts_per_minute: config.connection_attempts_per_ip_per_minute,
            max_rate_ips: config.connection_rate_limit_ips,
            pending: HashMap::new(),
            established: HashMap::new(),
            rate: HashMap::new(),
        }
    }

    fn admit(
        &mut self,
        connection_id: ConnectionId,
        remote_addr: &Multiaddr,
        now: Instant,
    ) -> Result<(), ConnectionDenied> {
        let ip = source_ip(remote_addr).ok_or_else(denied)?;
        let connections = self
            .pending
            .values()
            .chain(self.established.values())
            .filter(|candidate| candidate == &&ip)
            .count();
        if connections >= self.max_connections_per_ip {
            return Err(denied());
        }
        if !self.rate.contains_key(&ip) && self.rate.len() == self.max_rate_ips {
            self.rate
                .retain(|_, bucket| now.duration_since(bucket.started) < RATE_WINDOW);
            if self.rate.len() == self.max_rate_ips {
                return Err(denied());
            }
        }
        let bucket = self.rate.entry(ip).or_insert(RateBucket {
            started: now,
            attempts: 0,
        });
        if now.duration_since(bucket.started) >= RATE_WINDOW {
            *bucket = RateBucket {
                started: now,
                attempts: 0,
            };
        }
        if bucket.attempts >= self.attempts_per_minute {
            return Err(denied());
        }
        bucket.attempts += 1;
        self.pending.insert(connection_id, ip);
        Ok(())
    }

    fn release(&mut self, connection_id: ConnectionId) {
        self.pending.remove(&connection_id);
        self.established.remove(&connection_id);
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = Infallible;

    fn handle_pending_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        _local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        self.admit(connection_id, remote_addr, Instant::now())
    }

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _address: &Multiaddr,
        _role: Endpoint,
        _port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        match event {
            FromSwarm::ConnectionEstablished(event) => {
                if let Some(ip) = self.pending.remove(&event.connection_id) {
                    self.established.insert(event.connection_id, ip);
                }
            }
            FromSwarm::ConnectionClosed(event) => self.release(event.connection_id),
            FromSwarm::ListenFailure(event) => self.release(event.connection_id),
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        _peer: PeerId,
        _connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        libp2p::core::util::unreachable(event);
    }

    fn poll(
        &mut self,
        _context: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        Poll::Pending
    }
}

fn source_ip(address: &Multiaddr) -> Option<IpAddr> {
    address.iter().find_map(|protocol| match protocol {
        Protocol::Ip4(ip) => Some(IpAddr::V4(ip)),
        Protocol::Ip6(ip) => Some(IpAddr::V6(ip)),
        _ => None,
    })
}

fn denied() -> ConnectionDenied {
    ConnectionDenied::new(AdmissionDenied)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> NodeConfig {
        NodeConfig {
            max_connections_per_ip: 2,
            connection_attempts_per_ip_per_minute: 2,
            connection_rate_limit_ips: 2,
            ..NodeConfig::default()
        }
    }

    #[test]
    fn per_ip_connection_and_attempt_limits_shed_overload() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut admission = Behaviour::new(&config());
        let now = Instant::now();
        let address = "/ip4/192.0.2.1/tcp/4001".parse()?;
        admission.admit(ConnectionId::new_unchecked(1), &address, now)?;
        admission.admit(ConnectionId::new_unchecked(2), &address, now)?;

        assert!(
            admission
                .admit(ConnectionId::new_unchecked(3), &address, now)
                .is_err()
        );
        admission.release(ConnectionId::new_unchecked(1));
        assert!(
            admission
                .admit(ConnectionId::new_unchecked(3), &address, now)
                .is_err()
        );
        assert_eq!(admission.rate.len(), 1);
        Ok(())
    }

    #[test]
    fn rate_bucket_cardinality_is_bounded_and_expired_entries_are_reused()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut admission = Behaviour::new(&config());
        let now = Instant::now();
        for (id, address) in ["/ip4/192.0.2.1/tcp/1", "/ip4/192.0.2.2/tcp/1"]
            .into_iter()
            .enumerate()
        {
            admission.admit(ConnectionId::new_unchecked(id), &address.parse()?, now)?;
        }
        let third = "/ip4/192.0.2.3/tcp/1".parse()?;
        assert!(
            admission
                .admit(ConnectionId::new_unchecked(3), &third, now)
                .is_err()
        );

        admission.admit(ConnectionId::new_unchecked(3), &third, now + RATE_WINDOW)?;
        assert_eq!(admission.rate.len(), 1);
        Ok(())
    }
}
