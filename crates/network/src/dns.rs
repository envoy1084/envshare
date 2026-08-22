//! Bounded platform DNS expansion for explicit multiaddresses.

use std::{collections::HashSet, time::Duration};

use libp2p::{Multiaddr, multiaddr::Protocol};

use crate::NetworkError;

const MAX_RESOLVED_ADDRESSES: usize = 8;
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy)]
enum Family {
    Any,
    V4,
    V6,
}

pub(crate) async fn resolve(address: Multiaddr) -> Result<Vec<Multiaddr>, NetworkError> {
    let dns = address
        .iter()
        .enumerate()
        .filter_map(|(index, protocol)| dns_component(index, protocol))
        .collect::<Vec<_>>();
    if dns.len() > 1 {
        return Err(NetworkError::Dial);
    }
    let Some((dns_index, host, family)) = dns.into_iter().next() else {
        return Ok(vec![address]);
    };
    if host.is_empty() || host.len() > 253 {
        return Err(NetworkError::Dial);
    }
    let resolved =
        tokio::time::timeout(LOOKUP_TIMEOUT, tokio::net::lookup_host((host.as_str(), 0)))
            .await
            .map_err(|_| NetworkError::Dial)?
            .map_err(|_| NetworkError::Dial)?;
    let mut seen = HashSet::new();
    let mut addresses = Vec::new();
    for socket in resolved {
        let ip = socket.ip();
        if !matches!(
            (family, ip),
            (Family::Any, _)
                | (Family::V4, std::net::IpAddr::V4(_))
                | (Family::V6, std::net::IpAddr::V6(_))
        ) || !seen.insert(ip)
        {
            continue;
        }
        let mut expanded = Multiaddr::empty();
        for (index, protocol) in address.iter().enumerate() {
            if index == dns_index {
                match ip {
                    std::net::IpAddr::V4(ip) => expanded.push(Protocol::Ip4(ip)),
                    std::net::IpAddr::V6(ip) => expanded.push(Protocol::Ip6(ip)),
                }
            } else {
                expanded.push(protocol);
            }
        }
        addresses.push(expanded);
        if addresses.len() == MAX_RESOLVED_ADDRESSES {
            break;
        }
    }
    if addresses.is_empty() {
        return Err(NetworkError::Dial);
    }
    Ok(addresses)
}

fn dns_component(index: usize, protocol: Protocol<'_>) -> Option<(usize, String, Family)> {
    match protocol {
        Protocol::Dns(host) => Some((index, host.into_owned(), Family::Any)),
        Protocol::Dns4(host) => Some((index, host.into_owned(), Family::V4)),
        Protocol::Dns6(host) => Some((index, host.into_owned(), Family::V6)),
        // DNSADDR needs TXT-record expansion and recursive multiaddr validation. It is
        // deliberately unsupported instead of silently treating it as host DNS.
        Protocol::Dnsaddr(_) => Some((index, String::new(), Family::Any)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dns4_expands_without_changing_the_transport_suffix()
    -> Result<(), Box<dyn std::error::Error>> {
        let resolved = resolve("/dns4/localhost/tcp/4001".parse()?).await?;

        assert!(!resolved.is_empty());
        assert!(resolved.iter().all(|address| {
            matches!(address.iter().next(), Some(Protocol::Ip4(_)))
                && matches!(address.iter().nth(1), Some(Protocol::Tcp(4001)))
        }));
        Ok(())
    }

    #[tokio::test]
    async fn dnsaddr_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let address = "/dnsaddr/bootstrap.libp2p.io".parse()?;

        assert_eq!(resolve(address).await, Err(NetworkError::Dial));
        Ok(())
    }
}
