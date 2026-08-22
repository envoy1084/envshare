//! Validated relay service bounds.

use std::{net::SocketAddr, path::Path, time::Duration};

use app_core::read_bounded;
use libp2p::Multiaddr;
use serde::{Deserialize, Deserializer};

use crate::NodeError;

const MAX_CONFIG_BYTES: usize = 256 * 1024;
const MAX_LISTENERS: usize = 16;
const MAX_PROCESS_MEMORY_BYTES: usize = 64_usize.saturating_mul(1024 * 1024 * 1024);

/// Absolute safety ceilings for one relay node.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NodeConfig {
    /// Transport addresses to bind.
    #[serde(deserialize_with = "deserialize_multiaddresses")]
    pub listen_addresses: Vec<Multiaddr>,
    /// Maximum simultaneous reservations.
    pub max_reservations: usize,
    /// Maximum reservations owned by one peer.
    pub max_reservations_per_peer: usize,
    /// Reservation lifetime advertised to clients.
    #[serde(deserialize_with = "deserialize_duration")]
    pub reservation_duration: Duration,
    /// Maximum simultaneous relay circuits.
    pub max_circuits: usize,
    /// Maximum circuits associated with one peer.
    pub max_circuits_per_peer: usize,
    /// Hard duration for one circuit.
    #[serde(deserialize_with = "deserialize_duration")]
    pub max_circuit_duration: Duration,
    /// Hard byte count for one relayed circuit.
    pub max_circuit_bytes: u64,
    /// Maximum established transport connections.
    pub max_connections: u32,
    /// Maximum transport connections for one peer.
    pub max_connections_per_peer: u32,
    /// Maximum simultaneous inbound connections from one source IP.
    pub max_connections_per_ip: usize,
    /// Maximum inbound transport attempts accepted from one IP each minute.
    pub connection_attempts_per_ip_per_minute: u32,
    /// Maximum source-IP rate buckets retained in memory.
    pub connection_rate_limit_ips: usize,
    /// Process memory threshold for rejecting new connections.
    pub max_process_memory_bytes: usize,
    /// Capacity of the safe operational event stream.
    pub event_capacity: usize,
    /// Loopback HTTP address for health, readiness, and `OpenMetrics`.
    pub operations_address: Option<SocketAddr>,
    /// Maximum graceful shutdown drain period.
    #[serde(deserialize_with = "deserialize_duration")]
    pub shutdown_grace_period: Duration,
    /// Minimum accepted discovery registration lifetime.
    pub discovery_min_ttl_seconds: u64,
    /// Maximum accepted discovery registration lifetime.
    pub discovery_max_ttl_seconds: u64,
    /// Maximum discovery registrations owned by one peer.
    pub discovery_registrations_per_peer: usize,
    /// Absolute registration and maximum response-result bound.
    pub discovery_registrations_total: usize,
    /// Maximum simultaneous registrations in one opaque namespace.
    pub discovery_registrations_per_namespace: usize,
    /// Maximum incremental-discovery cookies retained in memory.
    pub discovery_cookies: usize,
    /// Maximum addresses accepted in one signed peer record.
    pub discovery_addresses_per_registration: usize,
    /// Maximum encoded signed peer-record size.
    pub discovery_record_bytes: usize,
    /// Maximum registrations returned by one discovery request.
    pub discovery_results: usize,
    /// Maximum register/unregister requests accepted per peer each minute.
    pub discovery_register_requests_per_minute: u32,
    /// Maximum discover requests accepted per peer each minute.
    pub discovery_discover_requests_per_minute: u32,
    /// Maximum peer rate buckets retained in memory.
    pub discovery_rate_limit_peers: usize,
    /// Admit private, loopback, and link-local registrations for private nodes.
    pub discovery_allow_private_addresses: bool,
    /// Safe local logging and optional trace export configuration.
    pub telemetry: TelemetryConfig,
}

/// Node logging and trace-export settings.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TelemetryConfig {
    /// Local log encoding.
    pub log_format: LogFormat,
    /// Static tracing filter directive.
    pub log_filter: String,
    /// OTLP/HTTP collector base URL; disabled when absent.
    pub otlp_endpoint: Option<String>,
    /// Fraction of root traces exported when OTLP is enabled.
    pub otlp_sample_ratio: f64,
}

/// Supported local log encodings.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Compact text logs for interactive operation.
    #[default]
    Text,
    /// Newline-delimited structured JSON logs.
    Json,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            log_format: LogFormat::Text,
            log_filter: "info,libp2p=warn".to_owned(),
            otlp_endpoint: None,
            otlp_sample_ratio: 0.01,
        }
    }
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            listen_addresses: vec![
                "/ip4/0.0.0.0/tcp/4001"
                    .parse()
                    .unwrap_or_else(|_| Multiaddr::empty()),
                "/ip4/0.0.0.0/udp/4001/quic-v1"
                    .parse()
                    .unwrap_or_else(|_| Multiaddr::empty()),
            ],
            max_reservations: 128,
            max_reservations_per_peer: 2,
            reservation_duration: Duration::from_hours(1),
            max_circuits: 64,
            max_circuits_per_peer: 4,
            max_circuit_duration: Duration::from_mins(2),
            max_circuit_bytes: 2 * 1024 * 1024,
            max_connections: 512,
            max_connections_per_peer: 8,
            max_connections_per_ip: 32,
            connection_attempts_per_ip_per_minute: 120,
            connection_rate_limit_ips: 4_096,
            max_process_memory_bytes: 1024 * 1024 * 1024,
            event_capacity: 256,
            operations_address: Some(([127, 0, 0, 1], 9_090).into()),
            shutdown_grace_period: Duration::from_secs(30),
            discovery_min_ttl_seconds: 30,
            discovery_max_ttl_seconds: 300,
            discovery_registrations_per_peer: 8,
            discovery_registrations_total: 256,
            discovery_registrations_per_namespace: 32,
            discovery_cookies: 512,
            discovery_addresses_per_registration: 8,
            discovery_record_bytes: 16 * 1024,
            discovery_results: 32,
            discovery_register_requests_per_minute: 12,
            discovery_discover_requests_per_minute: 30,
            discovery_rate_limit_peers: 1_024,
            discovery_allow_private_addresses: false,
            telemetry: TelemetryConfig::default(),
        }
    }
}

impl NodeConfig {
    /// Loads a bounded TOML configuration from a regular file.
    ///
    /// # Errors
    ///
    /// Returns a generic configuration failure for I/O, decoding, unknown
    /// fields, invalid multiaddresses, or bounds outside absolute ceilings.
    pub fn load(path: &Path) -> Result<Self, NodeError> {
        let file = std::fs::File::open(path).map_err(|_| NodeError::Configuration)?;
        if !file
            .metadata()
            .map_err(|_| NodeError::Configuration)?
            .is_file()
        {
            return Err(NodeError::Configuration);
        }
        let bytes = read_bounded(file, MAX_CONFIG_BYTES).map_err(|_| NodeError::Configuration)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| NodeError::Configuration)?;
        let config: Self = toml::from_str(text).map_err(|_| NodeError::Configuration)?;
        config
            .validate()
            .then_some(config)
            .ok_or(NodeError::Configuration)
    }

    pub(crate) fn validate(&self) -> bool {
        !self.listen_addresses.is_empty()
            && self.listen_addresses.len() <= MAX_LISTENERS
            && self
                .listen_addresses
                .iter()
                .all(|address| !address.is_empty())
            && self.max_reservations > 0
            && self.max_reservations <= 4_096
            && self.max_reservations_per_peer > 0
            && self.max_reservations_per_peer <= 64
            && self.max_reservations_per_peer <= self.max_reservations
            && !self.reservation_duration.is_zero()
            && self.reservation_duration <= Duration::from_hours(24)
            && self.max_circuits > 0
            && self.max_circuits <= 4_096
            && self.max_circuits_per_peer > 0
            && self.max_circuits_per_peer <= 64
            && self.max_circuits_per_peer <= self.max_circuits
            && !self.max_circuit_duration.is_zero()
            && self.max_circuit_duration <= Duration::from_hours(1)
            && (1024..=1024 * 1024 * 1024).contains(&self.max_circuit_bytes)
            && self.max_connections > 0
            && self.max_connections <= 8_192
            && self.max_connections_per_peer > 0
            && self.max_connections_per_peer <= 128
            && self.max_connections_per_peer <= self.max_connections
            && (1..=256).contains(&self.max_connections_per_ip)
            && (1..=1_200).contains(&self.connection_attempts_per_ip_per_minute)
            && (1..=16_384).contains(&self.connection_rate_limit_ips)
            && self.max_process_memory_bytes >= 64 * 1024 * 1024
            && self.max_process_memory_bytes <= MAX_PROCESS_MEMORY_BYTES
            && (1..=8_192).contains(&self.event_capacity)
            && self
                .operations_address
                .is_none_or(|address| address.ip().is_loopback())
            && self.shutdown_grace_period <= Duration::from_mins(5)
            && self.discovery_min_ttl_seconds > 0
            && self.discovery_min_ttl_seconds <= self.discovery_max_ttl_seconds
            && self.discovery_max_ttl_seconds <= 86_400
            && self.discovery_registrations_per_peer > 0
            && self.discovery_registrations_per_peer <= self.discovery_registrations_total
            && self.discovery_registrations_total <= 4_096
            && self.discovery_registrations_per_namespace > 0
            && self.discovery_registrations_per_namespace <= self.discovery_registrations_total
            && self.discovery_cookies > 0
            && self.discovery_cookies <= 8_192
            && self.discovery_addresses_per_registration > 0
            && self.discovery_addresses_per_registration <= 16
            && (512..=16 * 1024).contains(&self.discovery_record_bytes)
            && self.discovery_results > 0
            && self.discovery_results <= 64
            && self.discovery_results <= self.discovery_registrations_total
            && self
                .discovery_results
                .checked_mul(self.discovery_record_bytes)
                .is_some_and(|bytes| bytes <= 900 * 1024)
            && self.discovery_register_requests_per_minute > 0
            && self.discovery_register_requests_per_minute <= 120
            && self.discovery_discover_requests_per_minute > 0
            && self.discovery_discover_requests_per_minute <= 240
            && self.discovery_rate_limit_peers > 0
            && self.discovery_rate_limit_peers <= 4_096
            && validate_telemetry(&self.telemetry)
    }
}

fn validate_telemetry(config: &TelemetryConfig) -> bool {
    let valid_filter = !config.log_filter.is_empty()
        && config.log_filter.len() <= 256
        && !config.log_filter.chars().any(char::is_control)
        && tracing_subscriber::EnvFilter::try_new(&config.log_filter).is_ok();
    let valid_endpoint = config.otlp_endpoint.as_deref().is_none_or(|endpoint| {
        if endpoint.is_empty() {
            return true;
        }
        let authority = endpoint
            .strip_prefix("https://")
            .or_else(|| endpoint.strip_prefix("http://"))
            .and_then(|rest| rest.split('/').next());
        endpoint.len() <= 2_048
            && authority.is_some_and(|value| !value.is_empty())
            && !endpoint.chars().any(char::is_whitespace)
            && !endpoint
                .chars()
                .any(|value| matches!(value, '@' | '?' | '#'))
    });
    valid_filter
        && valid_endpoint
        && config.otlp_sample_ratio.is_finite()
        && (0.0..=0.1).contains(&config.otlp_sample_ratio)
}

fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    humantime::parse_duration(&value).map_err(serde::de::Error::custom)
}

fn deserialize_multiaddresses<'de, D>(deserializer: D) -> Result<Vec<Multiaddr>, D::Error>
where
    D: Deserializer<'de>,
{
    Vec::<String>::deserialize(deserializer)?
        .into_iter()
        .map(|value| value.parse().map_err(serde::de::Error::custom))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_is_strict_and_human_durations_are_parsed() -> Result<(), Box<dyn std::error::Error>> {
        let config: NodeConfig = toml::from_str(
            r#"
listen_addresses = ["/ip4/127.0.0.1/tcp/0"]
reservation_duration = "30m"
max_circuit_duration = "45s"
operations_address = "127.0.0.1:0"
shutdown_grace_period = "5s"
"#,
        )?;

        assert_eq!(config.reservation_duration, Duration::from_mins(30));
        assert_eq!(config.max_circuit_duration, Duration::from_secs(45));
        assert_eq!(config.operations_address, Some(([127, 0, 0, 1], 0).into()));
        assert!(toml::from_str::<NodeConfig>("unknown = 1").is_err());
        Ok(())
    }

    #[test]
    fn absolute_safety_ceilings_cannot_be_disabled() {
        let config = NodeConfig {
            max_connections: 8_193,
            ..NodeConfig::default()
        };
        assert!(!config.validate());

        let config = NodeConfig {
            telemetry: TelemetryConfig {
                log_filter: "info[".to_owned(),
                ..TelemetryConfig::default()
            },
            ..NodeConfig::default()
        };
        assert!(!config.validate());

        let config = NodeConfig {
            telemetry: TelemetryConfig {
                otlp_endpoint: Some("https://token@example.com".to_owned()),
                ..TelemetryConfig::default()
            },
            ..NodeConfig::default()
        };
        assert!(!config.validate());

        let config = NodeConfig {
            max_circuit_bytes: 1024 * 1024 * 1024 + 1,
            ..NodeConfig::default()
        };
        assert!(!config.validate());

        let config = NodeConfig {
            listen_addresses: vec![Multiaddr::empty(); MAX_LISTENERS + 1],
            ..NodeConfig::default()
        };
        assert!(!config.validate());
    }

    #[test]
    fn loader_accepts_regular_bounded_files_only() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("node.toml");
        std::fs::write(&path, "listen_addresses = [\"/ip4/127.0.0.1/tcp/0\"]\n")?;

        assert_eq!(NodeConfig::load(&path)?.listen_addresses.len(), 1);
        assert!(matches!(
            NodeConfig::load(directory.path()),
            Err(NodeError::Configuration)
        ));
        Ok(())
    }
}
