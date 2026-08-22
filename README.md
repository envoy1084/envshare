# Envshare

Envshare is an accountless, one-time handoff for `.env` files and selected
environment variables. The sender stays online while one receiver claims an
end-to-end encrypted payload over libp2p.

> **Status:** early implementation. The protocol and command-line interface are
> not yet stable and this repository must not yet be used for production secrets.

## Product boundary

- One sender, one receiver claim, and a short expiration.
- Direct QUIC or TCP where possible, with Circuit Relay v2 fallback.
- Federated Rendezvous discovery using an opaque room identifier.
- Application-layer capability authentication and XChaCha20-Poly1305 encryption.
- No accounts, database, hosted payload, offline mailbox, or persistent client
  identity.

The capability is a bearer secret. Anyone who obtains it can claim the share.
Envshare does not protect a secret after an authorized receiver obtains it and
does not provide anonymity.

## Commands

Start a node with a persistent identity, then use its printed endpoint for
federated discovery:

```console
envshare-node key generate --output node.key
envshare-node serve --identity node.key --config node.toml

envshare send .env --discovery-node <NODE_MULTIADDR_WITH_PEER_ID>
envshare receive --discovery-node <NODE_MULTIADDR_WITH_PEER_ID> --output .env.shared
envshare run --discovery-node <NODE_MULTIADDR_WITH_PEER_ID> -- cargo run
```

The sender reveals the capability only after at least one configured discovery
node accepts its signed registration. Configure multiple `--discovery-node`
values for federation. The receiver queries them concurrently, bounds and
deduplicates their untrusted results, then capability-authenticates candidates.
Use `--lan` to admit private addresses and `--mdns` for optional LAN discovery;
`--relay-only` disables direct candidates and mDNS.

Explicit direct mode remains available:

```console
envshare send .env --listen /ip4/127.0.0.1/tcp/0
envshare receive --peer <PEER_ID> --address <MULTIADDR> --output .env.shared
envshare run --peer <PEER_ID> --address <MULTIADDR> -- cargo run
```

The sender prints a capability code, Peer ID, and direct multiaddress after its
listener is ready. The receiver reads the code through a hidden prompt by
default; `--code-stdin` is available for automation.

## Development

Envshare uses the Rust version pinned in `rust-toolchain.toml`.

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --workspace --all-targets --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo deny check
```

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for repository conventions and
[`SECURITY.md`](SECURITY.md) for vulnerability reporting.

## Architecture and protocol

- [Architecture overview](docs/architecture.md)
- [Threat model](docs/threat-model.md)
- [Secret output policy](docs/secret-output-policy.md)
- [Node identity operations](docs/node-identity.md)
- [Node configuration](docs/node-configuration.md)
- [Node deployment](docs/deployment.md)
- [Node operations](docs/operations.md)
- [Node load and soak testing](docs/load-testing.md)
- [Command-line interface](docs/cli.md)
- [Client configuration](docs/configuration.md)
- [Protocol specification](protocol/protocol.md)
- [Capability code format](protocol/code-format.md)
- [Wire schema](protocol/messages.cddl)
- [Implementation research](research/envshare-production-architecture.md)

## License

Envshare is licensed under the [Apache License, Version 2.0](LICENSE).
