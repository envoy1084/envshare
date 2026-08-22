# Changelog

All notable changes to Envshare are documented here. Releases follow Semantic
Versioning while the public interfaces remain experimental during `0.x`.

## [Unreleased]

## [0.1.0-alpha.1] - 2026-08-22

### Added

- Accountless one-time environment sharing through direct libp2p connections or
  Circuit Relay v2 fallback.
- Federated opaque Rendezvous discovery and optional local mDNS discovery.
- Capability authentication, transcript-bound key derivation, and
  XChaCha20-Poly1305 payload encryption.
- Cross-platform `envshare` client plus a hardened, self-hostable
  `envshare-node` discovery and relay service.
- Bounded health, metrics, logging, tracing, load-testing, deployment, and
  incident-response tooling.

[Unreleased]: https://github.com/envshare/envshare/compare/v0.1.0-alpha.1...HEAD
[0.1.0-alpha.1]: https://github.com/envshare/envshare/releases/tag/v0.1.0-alpha.1
