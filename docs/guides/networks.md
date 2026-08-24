# Networks

A network profile defines discovery endpoints, relay endpoints, and the public
network identifier used in protocol derivation.

## Public network

The built-in `public` profile is selected when no configuration exists:

```sh
envshare send .env
envshare receive
```

It uses `node.envshare.xyz` for Rendezvous discovery and Circuit Relay. The
profile is relay-only, so normal transfers do not advertise local host routes.

Check connectivity:

```sh
envshare doctor --verbose
```

## Manage profiles

```sh
envshare network list
envshare network show public
```

Create `team.toml` for a private node:

```toml
network_id = "team-v1"
require_relay = true
relay_only = true
rendezvous = [
  "/dns4/node.example.com/udp/4001/quic-v1/p2p/12D3KooW...",
]
relays = [
  "/dns4/node.example.com/tcp/4001/p2p/12D3KooW...",
]
```

Add and select it:

```sh
envshare network add team --file team.toml
envshare network use team
envshare doctor --network team --verbose
```

Both peers must resolve their profile to the same `network_id`. The local
profile name does not need to match.

Use a profile for one command without changing the default:

```sh
envshare send .env --network team
envshare receive --network team
```

## Direct mode

Start a sender and request diagnostic output:

```sh
envshare send .env --verbose --listen /ip4/127.0.0.1/tcp/0
```

Use the printed Peer ID and address in another terminal:

```sh
envshare receive \
  --peer PEER_ID \
  --address MULTIADDR \
  --output .env.received
```

`--peer` and `--address` must be supplied together. Direct mode still uses
capability authentication and application-layer payload encryption.

## Local network discovery

Use `--mdns` on the sender and receiver to enable multicast DNS. The receiver
also needs `--lan` to admit private and link-local candidate addresses.

```sh
envshare send .env --mdns
envshare receive --mdns --lan
```

Do not use `--lan` on an untrusted local network unless accepting local address
candidates is intentional.
