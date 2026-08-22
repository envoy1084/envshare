# Client configuration

Envshare applies configuration in this order: built-in safety defaults, the
selected profile in the client TOML file, `ENVSHARE_*` environment variables,
then explicit command-line flags. Capability codes and payload values are never
valid configuration fields.

The default path follows the platform configuration directory. Override it with
`--config <PATH>` or `ENVSHARE_CONFIG`. Supported environment overrides are
`ENVSHARE_NETWORK`, `ENVSHARE_SHARE_TTL`, `ENVSHARE_MDNS`, and
`ENVSHARE_RELAY_ONLY`.

```toml
version = 1
default_network = "team"

[defaults]
share_ttl = "10m"
relay_only = false
mdns = false

[networks.team]
network_id = "team"
require_relay = true
rendezvous = [
  "/dns4/node.example/udp/4001/quic-v1/p2p/12D3KooW...",
]
relays = [
  "/dns4/node.example/udp/4001/quic-v1/p2p/12D3KooW...",
]
```

Manage profiles with `envshare network list|show|add|remove|use`. `network add`
accepts a file containing one profile body (the fields below `[networks.NAME]`
in the example). Config writes are atomic and owner-private.

Generate installation assets without checking in stale generated copies:

```console
envshare completions bash > envshare.bash
envshare completions zsh > _envshare
envshare completions man > envshare.1
```
