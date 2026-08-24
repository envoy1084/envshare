# Envshare

Envshare is an accountless, one-time handoff for `.env` files and selected
environment variables. The sender stays online while one receiver claims an
end-to-end encrypted payload over libp2p.

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

Install Envshare with Homebrew on macOS or Linux:

```console
brew install envoy1084/tap/envshare
```

Alternatively, use the release installer:

```console
curl -fsSL https://github.com/envoy1084/envshare/releases/latest/download/install.sh | sh
```

Windows PowerShell, updates, and explicit-location commands are documented in
the [installation guide](docs/guides/installation.md). Release installers verify
the archive SHA-256 before installing it and never prompt or edit shell profiles.

## Commands

The built-in `public` network uses the Envshare-operated discovery and relay
node, so a normal transfer needs no network flags:

```console
envshare send
envshare receive
envshare run -- cargo run
```

In an interactive terminal, `send` offers the dotenv files in the current
directory and `receive` asks for the share code. A new payload is saved to
`.env`; when that file exists, Envshare offers to merge values, add only missing
keys, save elsewhere, replace it, or cancel before claiming the share.

The sender reveals the capability only after the public node accepts its relay
reservation and signed registration. The receiver discovers the sender and
capability-authenticates it before accepting the encrypted payload. Network
flags remain available for self-hosted profiles, explicit endpoints, and LAN
use; see the [network guide](docs/guides/networks.md).

Explicit direct mode remains available:

```console
envshare send .env --listen /ip4/127.0.0.1/tcp/0
envshare receive --peer <PEER_ID> --address <MULTIADDR> --output .env.shared
envshare run --peer <PEER_ID> --address <MULTIADDR> -- cargo run
```

The sender shows only the capability and transfer state by default. Add
`--verbose` when explicit direct mode requires the Peer ID and multiaddress. The
receiver reads the code through a hidden prompt by default; `--code-stdin` is
available for automation.

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

## Documentation

Documentation is plain Markdown under [`docs/`](docs/):

- [Guides](docs/guides/README.md)
- [`envshare` reference](docs/reference/envshare.md)
- [`envshare-node` reference](docs/reference/envshare-node.md)

Protocol maintainers can also refer to the [protocol specification](protocol/protocol.md),
[capability format](protocol/code-format.md), and [wire schema](protocol/messages.cddl).
The proposed organization workspace is specified in the
[Envshare v1 product and architecture document](research/envshare-v1.md).

## License

Envshare is licensed under the [Apache License, Version 2.0](LICENSE).
