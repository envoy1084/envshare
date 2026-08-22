//! Small bounded DNS-SD adapter isolated from libp2p transport internals.

use std::{net::IpAddr, str::FromStr as _};

use libp2p::multiaddr::Protocol;
use mdns_sd::{Receiver, ServiceDaemon, ServiceEvent, ServiceInfo};

use crate::{DiscoveredPeer, Multiaddr, NetworkError, PeerId};

const SERVICE_TYPE: &str = "_envshare._tcp.local.";

pub(crate) enum LanEvent {
    Discovered(DiscoveredPeer),
    Expired(PeerId),
}

pub(crate) struct LanDiscovery {
    daemon: ServiceDaemon,
    events: Receiver<ServiceEvent>,
    local_peer: PeerId,
    max_results: usize,
}

impl LanDiscovery {
    pub(crate) fn new(local_peer: PeerId, max_results: usize) -> Result<Self, NetworkError> {
        let daemon = ServiceDaemon::new().map_err(|_| NetworkError::Configuration)?;
        let events = daemon
            .browse(SERVICE_TYPE)
            .map_err(|_| NetworkError::Configuration)?;
        Ok(Self {
            daemon,
            events,
            local_peer,
            max_results,
        })
    }

    pub(crate) fn advertise(&self, address: &Multiaddr) -> Result<(), NetworkError> {
        let Some((ip, port)) = tcp_endpoint(address) else {
            return Ok(());
        };
        let peer = self.local_peer.to_string();
        let host = format!("{peer}.local.");
        let properties = [("peer", peer.as_str())];
        let mut info =
            ServiceInfo::new(SERVICE_TYPE, &peer, &host, ip, port, properties.as_slice())
                .map_err(|_| NetworkError::Configuration)?;
        if ip.is_unspecified() {
            info = info.enable_addr_auto();
        }
        self.daemon
            .register(info)
            .map_err(|_| NetworkError::Configuration)
    }

    pub(crate) async fn next(&self) -> Option<LanEvent> {
        loop {
            let event = self.events.recv_async().await.ok()?;
            match event {
                ServiceEvent::ServiceResolved(service) => {
                    let peer = service
                        .get_property_val_str("peer")
                        .and_then(|value| PeerId::from_str(value).ok());
                    let Some(peer) = peer.filter(|peer| peer != &self.local_peer) else {
                        continue;
                    };
                    let addresses = service
                        .get_addresses()
                        .iter()
                        .take(self.max_results)
                        .filter_map(|address| {
                            socket_multiaddr(address.to_ip_addr(), service.get_port())
                        })
                        .collect::<Vec<_>>();
                    if addresses.is_empty() {
                        continue;
                    }
                    return Some(LanEvent::Discovered(DiscoveredPeer { peer, addresses }));
                }
                ServiceEvent::ServiceRemoved(_, fullname) => {
                    if let Some(peer) = fullname
                        .split('.')
                        .next()
                        .and_then(|value| PeerId::from_str(value).ok())
                        .filter(|peer| peer != &self.local_peer)
                    {
                        return Some(LanEvent::Expired(peer));
                    }
                }
                _ => {}
            }
        }
    }
}

impl Drop for LanDiscovery {
    fn drop(&mut self) {
        let _ = self.daemon.shutdown();
    }
}

fn tcp_endpoint(address: &Multiaddr) -> Option<(IpAddr, u16)> {
    let mut ip = None;
    let mut port = None;
    for protocol in address {
        match protocol {
            Protocol::Ip4(address) => ip = Some(IpAddr::V4(address)),
            Protocol::Ip6(address) => ip = Some(IpAddr::V6(address)),
            Protocol::Tcp(value) => port = Some(value),
            _ => {}
        }
    }
    ip.zip(port).filter(|(_, port)| *port != 0)
}

fn socket_multiaddr(ip: IpAddr, port: u16) -> Option<Multiaddr> {
    match ip {
        IpAddr::V4(address) => format!("/ip4/{address}/tcp/{port}").parse().ok(),
        IpAddr::V6(address) => format!("/ip6/{address}/tcp/{port}").parse().ok(),
    }
}
