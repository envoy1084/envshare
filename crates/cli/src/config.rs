//! Cross-platform client configuration and precedence.

use std::{collections::BTreeMap, path::PathBuf, str::FromStr as _};

use app_core::{PrivateOutputOptions, read_bounded, write_private_atomic};
use network::DiscoveryNode;
use serde::{Deserialize, Serialize};

use crate::{
    CliFailure, ExitCode,
    args::{ConnectionArgs, SendArgs},
};

const CONFIG_VERSION: u8 = 1;
const MAX_NETWORKS: usize = 32;
const MAX_NODES: usize = 8;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ClientConfig {
    version: u8,
    default_network: String,
    defaults: ClientDefaults,
    networks: BTreeMap<String, NetworkProfile>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct ClientDefaults {
    share_ttl: String,
    relay_only: bool,
    mdns: bool,
}

/// One named, non-secret federation profile.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct NetworkProfile {
    pub network_id: String,
    pub require_relay: bool,
    pub rendezvous: Vec<String>,
    pub relays: Vec<String>,
}

pub(crate) struct LoadedConfig {
    pub path: PathBuf,
    pub value: ClientConfig,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            default_network: "public-v1".to_owned(),
            defaults: ClientDefaults::default(),
            networks: BTreeMap::new(),
        }
    }
}

impl Default for ClientDefaults {
    fn default() -> Self {
        Self {
            share_ttl: "10m".to_owned(),
            relay_only: false,
            mdns: false,
        }
    }
}

impl LoadedConfig {
    pub fn load(explicit_path: Option<PathBuf>) -> Result<Self, CliFailure> {
        Self::load_with_missing(explicit_path, false)
    }

    pub fn load_for_management(explicit_path: Option<PathBuf>) -> Result<Self, CliFailure> {
        Self::load_with_missing(explicit_path, true)
    }

    fn load_with_missing(
        explicit_path: Option<PathBuf>,
        allow_explicit_missing: bool,
    ) -> Result<Self, CliFailure> {
        let explicit = explicit_path.is_some() || std::env::var_os("ENVSHARE_CONFIG").is_some();
        let path = explicit_path
            .or_else(|| std::env::var_os("ENVSHARE_CONFIG").map(PathBuf::from))
            .unwrap_or_else(default_config_path);
        let mut value = match read_config_text(&path) {
            Ok(text) => toml::from_str(&text).map_err(|_| invalid_config())?,
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && (!explicit || allow_explicit_missing) =>
            {
                ClientConfig::default()
            }
            Err(_) => return Err(invalid_config()),
        };
        value.apply_environment()?;
        value.validate()?;
        Ok(Self { path, value })
    }

    pub fn save(&self) -> Result<(), CliFailure> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| invalid_config())?;
        }
        let text = toml::to_string_pretty(&self.value).map_err(|_| invalid_config())?;
        write_private_atomic(
            &self.path,
            text.as_bytes(),
            PrivateOutputOptions {
                replace: self.path.exists(),
                durable: true,
            },
        )
        .map_err(Into::into)
    }
}

impl ClientConfig {
    pub fn apply_send(&self, arguments: &mut SendArgs) -> Result<(), CliFailure> {
        let network = arguments
            .network
            .get_or_insert_with(|| self.default_network.clone())
            .clone();
        if arguments.expires.is_none() {
            arguments.expires = Some(
                humantime::parse_duration(&self.defaults.share_ttl)
                    .map_err(|_| invalid_config())?,
            );
        }
        self.apply_discovery(
            &network,
            &mut arguments.discovery.nodes,
            &mut arguments.discovery.mdns,
            &mut arguments.discovery.relay_only,
        )
    }

    pub fn apply_connection(&self, arguments: &mut ConnectionArgs) -> Result<(), CliFailure> {
        let network = arguments
            .network
            .get_or_insert_with(|| self.default_network.clone())
            .clone();
        self.apply_discovery(
            &network,
            &mut arguments.discovery.nodes,
            &mut arguments.discovery.mdns,
            &mut arguments.discovery.relay_only,
        )
    }

    pub fn profile_names(&self) -> impl Iterator<Item = &str> {
        self.networks.keys().map(String::as_str)
    }

    pub fn profile(&self, name: &str) -> Option<&NetworkProfile> {
        self.networks.get(name)
    }

    pub fn diagnostic_nodes(
        &self,
        selected: Option<&str>,
    ) -> Result<(String, Vec<DiscoveryNode>, bool), CliFailure> {
        let name = selected.unwrap_or(&self.default_network);
        validate_network_id(name)?;
        let profile = self.networks.get(name).ok_or_else(invalid_config)?;
        let nodes = profile
            .rendezvous
            .iter()
            .map(|address| DiscoveryNode::from_str(address).map_err(|_| invalid_config()))
            .collect::<Result<_, _>>()?;
        Ok((profile.network_id.clone(), nodes, profile.require_relay))
    }

    pub fn add_profile(
        &mut self,
        name: String,
        mut profile: NetworkProfile,
    ) -> Result<(), CliFailure> {
        validate_network_id(&name)?;
        if profile.network_id.is_empty() {
            profile.network_id.clone_from(&name);
        }
        self.networks.insert(name, profile);
        self.validate()
    }

    pub fn remove_profile(&mut self, name: &str) -> Result<(), CliFailure> {
        if self.networks.remove(name).is_none() || self.default_network == name {
            return Err(invalid_config());
        }
        Ok(())
    }

    pub fn use_profile(&mut self, name: &str) -> Result<(), CliFailure> {
        if !self.networks.contains_key(name) {
            return Err(invalid_config());
        }
        name.clone_into(&mut self.default_network);
        Ok(())
    }

    fn apply_environment(&mut self) -> Result<(), CliFailure> {
        if let Ok(network) = std::env::var("ENVSHARE_NETWORK") {
            self.default_network = network;
        }
        if let Ok(ttl) = std::env::var("ENVSHARE_SHARE_TTL") {
            humantime::parse_duration(&ttl).map_err(|_| invalid_config())?;
            self.defaults.share_ttl = ttl;
        }
        if let Ok(value) = std::env::var("ENVSHARE_MDNS") {
            self.defaults.mdns = parse_bool(&value)?;
        }
        if let Ok(value) = std::env::var("ENVSHARE_RELAY_ONLY") {
            self.defaults.relay_only = parse_bool(&value)?;
        }
        Ok(())
    }

    fn apply_discovery(
        &self,
        network: &str,
        nodes: &mut Vec<DiscoveryNode>,
        mdns: &mut bool,
        relay_only: &mut bool,
    ) -> Result<(), CliFailure> {
        *mdns |= self.defaults.mdns;
        *relay_only |= self.defaults.relay_only;
        if nodes.is_empty()
            && let Some(profile) = self.networks.get(network)
        {
            *nodes = profile
                .rendezvous
                .iter()
                .map(|address| DiscoveryNode::from_str(address).map_err(|_| invalid_config()))
                .collect::<Result<_, _>>()?;
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), CliFailure> {
        if self.version != CONFIG_VERSION || self.networks.len() > MAX_NETWORKS {
            return Err(invalid_config());
        }
        validate_network_id(&self.default_network)?;
        humantime::parse_duration(&self.defaults.share_ttl).map_err(|_| invalid_config())?;
        for (name, profile) in &self.networks {
            validate_network_id(name)?;
            validate_network_id(&profile.network_id)?;
            if profile.rendezvous.len() > MAX_NODES || profile.relays.len() > MAX_NODES {
                return Err(invalid_config());
            }
            for addresses in [&profile.rendezvous, &profile.relays] {
                let mut peers = std::collections::HashSet::new();
                for address in addresses {
                    let node = DiscoveryNode::from_str(address).map_err(|_| invalid_config())?;
                    if !peers.insert(node.peer) {
                        return Err(invalid_config());
                    }
                }
            }
        }
        Ok(())
    }
}

impl NetworkProfile {
    pub fn from_file(path: &std::path::Path) -> Result<Self, CliFailure> {
        let text = read_config_text(path).map_err(|_| invalid_config())?;
        toml::from_str(&text).map_err(|_| invalid_config())
    }
}

fn default_config_path() -> PathBuf {
    platform_config_root()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("envshare")
        .join("config.toml")
}

#[cfg(target_os = "windows")]
fn platform_config_root() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

#[cfg(target_os = "macos")]
fn platform_config_root() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library").join("Application Support"))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn platform_config_root() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".config"))
        })
}

fn read_config_text(path: &std::path::Path) -> Result<String, std::io::Error> {
    let file = std::fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "configuration is not a regular file",
        ));
    }
    let bytes = read_bounded(file, 256 * 1024)
        .map_err(|_| std::io::Error::other("configuration exceeds its bound"))?;
    String::from_utf8(bytes).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "configuration is not UTF-8",
        )
    })
}

fn validate_network_id(value: &str) -> Result<(), CliFailure> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(invalid_config());
    }
    Ok(())
}

fn parse_bool(value: &str) -> Result<bool, CliFailure> {
    match value {
        "1" | "true" | "yes" => Ok(true),
        "0" | "false" | "no" => Ok(false),
        _ => Err(invalid_config()),
    }
}

const fn invalid_config() -> CliFailure {
    CliFailure::new(ExitCode::Configuration, "configuration is invalid")
}

#[cfg(test)]
mod tests {
    use clap::Parser as _;

    use super::*;
    use crate::args::{Cli, Command};

    #[test]
    fn explicit_cli_values_win_even_when_equal_to_builtin_defaults()
    -> Result<(), Box<dyn std::error::Error>> {
        let config = ClientConfig {
            default_network: "configured".to_owned(),
            defaults: ClientDefaults {
                share_ttl: "20m".to_owned(),
                ..ClientDefaults::default()
            },
            ..ClientConfig::default()
        };
        let mut cli = Cli::try_parse_from([
            "envshare",
            "send",
            "source.env",
            "--network",
            "public-v1",
            "--expires",
            "10m",
        ])?;
        let Command::Send(arguments) = &mut cli.command else {
            return Err("send command expected".into());
        };

        config.apply_send(arguments)?;

        assert_eq!(arguments.network.as_deref(), Some("public-v1"));
        assert_eq!(arguments.expires, Some(std::time::Duration::from_mins(10)));
        Ok(())
    }

    #[test]
    fn omitted_cli_values_are_filled_from_configuration() -> Result<(), Box<dyn std::error::Error>>
    {
        let config = ClientConfig {
            default_network: "configured".to_owned(),
            defaults: ClientDefaults {
                share_ttl: "20m".to_owned(),
                ..ClientDefaults::default()
            },
            ..ClientConfig::default()
        };
        let mut cli = Cli::try_parse_from(["envshare", "send", "source.env"])?;
        let Command::Send(arguments) = &mut cli.command else {
            return Err("send command expected".into());
        };

        config.apply_send(arguments)?;

        assert_eq!(arguments.network.as_deref(), Some("configured"));
        assert_eq!(arguments.expires, Some(std::time::Duration::from_mins(20)));
        Ok(())
    }
}
