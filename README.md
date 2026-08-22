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

## Planned commands

```console
envshare send .env
envshare receive --output .env.shared
envshare run -- cargo run
envshare doctor
```

The currently compiled binaries only expose version and help information while
the protocol is implemented phase by phase. Progress is tracked in
[`todos.md`](todos.md).

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
- [Protocol specification](protocol/protocol.md)
- [Capability code format](protocol/code-format.md)
- [Wire schema](protocol/messages.cddl)
- [Implementation research](research/envshare-production-architecture.md)

## License

Envshare is licensed under either of

- Apache License, Version 2.0, or
- MIT License

at your option.
