# Changelog

All notable changes to Envshare are documented here. Releases follow Semantic
Versioning while the public interfaces remain experimental during `0.x`.

## [Unreleased]

## [0.1.1] - 2026-08-22

### Fixed

- Securely persist received output on Windows by staging it under a protected,
  current-user-only DACL before writing secret bytes. This fixes `receive` and
  `run` failing with `private output failed` in `v0.1.0`.

### Added

- Manual release qualification of the actual published installers across Linux,
  macOS, and Windows, including direct QUIC, direct TCP, and relay-only transfer.
- Signed multi-architecture node images with SBOMs and build provenance.

### Changed

- Report the payload format derived from selected environment keys and expand
  registration-renewal, acknowledgement-loss, and node-outage coverage.

## [0.1.0] - 2026-08-22

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

### Security status

- This initial 0.x release has extensive automated and local qualification but
  has not completed independent security review or the documented 24-hour
  production-node soak. It is not described as production-ready for high-value
  secrets.

[Unreleased]: https://github.com/envoy1084/envshare/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/envoy1084/envshare/releases/tag/v0.1.1
[0.1.0]: https://github.com/envoy1084/envshare/releases/tag/v0.1.0
