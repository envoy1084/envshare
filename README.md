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

## Installation

Install a versioned release on Linux or macOS:

```console
curl --proto '=https' --tlsv1.2 --connect-timeout 10 --max-time 120 -LsSf https://github.com/envoy1084/envshare/releases/download/v0.1.2/install.sh | sh
```

Windows PowerShell and explicit-location commands are documented in the
[installation guide](docs/installation.md). Release installers verify the
archive SHA-256 before installing it and never prompt or edit shell profiles.

## Commands

The built-in `public` network uses the Envshare-operated discovery and relay
node, so a normal transfer needs no network flags:

```console
envshare send .env
envshare receive --output .env.shared
envshare run -- cargo run
```

The sender reveals the capability only after the public node accepts its relay
reservation and signed registration. The receiver discovers the sender and
capability-authenticates it before accepting the encrypted payload. Network
flags remain available for self-hosted profiles, explicit endpoints, and LAN
use; see the [configuration guide](docs/configuration.md).

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

- [Documentation index](docs/README.md)
- [User guide](docs/user-guide.md)
- [Architecture overview](docs/architecture.md)
- [Threat model](docs/threat-model.md)
- [Secret output policy](docs/secret-output-policy.md)
- [Security guide](docs/security.md)
- [Privacy and data handling](docs/privacy.md)
- [Troubleshooting](docs/troubleshooting.md)
- [Node identity operations](docs/node-identity.md)
- [Node configuration](docs/node-configuration.md)
- [Node deployment](docs/deployment.md)
- [Node operations](docs/operations.md)
- [Node load and soak testing](docs/load-testing.md)
- [Installation](docs/installation.md)
- [Release and rollback](docs/release.md)
- [Command-line interface](docs/cli.md)
- [Client configuration](docs/configuration.md)
- [Protocol specification](protocol/protocol.md)
- [Capability code format](protocol/code-format.md)
- [Wire schema](protocol/messages.cddl)
- [Implementation research](research/envshare-production-architecture.md)

## License

Envshare is licensed under the [Apache License, Version 2.0](LICENSE).
