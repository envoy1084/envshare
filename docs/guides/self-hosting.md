# Self-host an Envshare node

`envshare-node` provides federated Rendezvous discovery and Circuit Relay v2.
It does not accept share codes or store payloads.

Use one bounded public node for an initial deployment. The default configuration
already limits reservations, circuits, connections, discovery records, memory,
and request rates.

## Requirements

- A Linux host with a stable public IP.
- A DNS record such as `node.example.com` pointing to the host.
- Inbound TCP and UDP port `4001`.
- A persistent directory for the Ed25519 identity.
- A reverse proxy or monitoring agent that can reach loopback port `9100`.

## Build and install

```sh
cargo build --release --locked --package envshare-node
sudo install -m 0755 target/release/envshare-node /usr/local/bin/envshare-node
```

Create a service account and directories according to local policy. Keep the
identity directory writable only by that account.

## Create the identity

```sh
envshare-node key generate \
  --output /var/lib/envshare-node/identity.key
```

Record the printed Peer ID. Clients include it at the end of every node
multiaddress. Do not regenerate the key during upgrades.

## Configure the node

Start from [`deploy/config/node.example.toml`](../../deploy/config/node.example.toml):

```sh
sudo install -m 0640 deploy/config/node.example.toml /etc/envshare-node/node.toml
envshare-node config check --config /etc/envshare-node/node.toml
```

Keep `operations_address` on loopback. Public nodes should leave
`discovery_allow_private_addresses = false`.

## Start and verify

```sh
envshare-node serve \
  --identity /var/lib/envshare-node/identity.key \
  --config /etc/envshare-node/node.toml
```

From the same host:

```sh
envshare-node healthcheck \
  --url http://127.0.0.1:9100/health/ready
```

The operations server exposes:

- `/health/live` for process liveness;
- `/health/ready` for readiness; and
- `/metrics` in OpenMetrics format.

Use the maintained [systemd unit](../../deploy/systemd/envshare-node.service) or
[Docker files](../../deploy/docker/) for deployment. Review their paths, user,
resource limits, and image name before use.

## Client profile

Use the node Peer ID in a profile:

```toml
network_id = "team-v1"
require_relay = true
relay_only = true
rendezvous = [
  "/dns4/node.example.com/udp/4001/quic-v1/p2p/PEER_ID",
]
relays = [
  "/dns4/node.example.com/tcp/4001/p2p/PEER_ID",
]
```

Add the profile and validate connectivity:

```sh
envshare network add team --file team.toml
envshare doctor --network team --verbose
```

See the [`envshare-node` reference](../reference/envshare-node.md) for all
configuration limits and operational behavior.
