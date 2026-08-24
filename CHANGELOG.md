# Changelog

All notable changes to Envshare are documented here. Releases follow Semantic
Versioning.

## [Unreleased]

## [0.1.4] - 2026-08-23

### Fixed

- Resolve bare relative output paths such as `.env` against the current
  directory, preventing interactive receive from failing with
  `private output failed` after file selection.

### Changed

- Keep CI, CLI releases, and container releases as three separate manual-only
  workflows, with only focused smoke tests in hosted CI.
- Publish CLI archives and installers independently from the node container.

## [0.1.3] - 2026-08-23

### Added

- Add interactive dotenv file selection, a focused share-code view, hidden
  receive prompts, and safe existing-file choices for the human CLI.
- Add atomic dotenv merge and add-missing modes that preserve unrelated local
  declarations and comments.

### Changed

- Hide sender peer and route diagnostics from normal output; `--verbose` retains
  them for direct-mode troubleshooting.
- Keep scripted receive behavior deterministic through explicit `--output` and
  `--mode` options.

## [0.1.2] - 2026-08-23

### Added

- Embed the authenticated, relay-only `public` network so new installations can
  send, receive, run, and diagnose through the Envshare node without creating a
  profile or passing network flags.

### Fixed

- Publish canonical DNS relay circuit routes in signed Rendezvous registrations
  so the public node accepts them and receivers can dial them.
- Keep confirmed relay streams alive briefly enough to deliver the final
  completion response before sender shutdown.
- Bound installer release downloads with connection and total timeouts plus
  retries, and document an attested GitHub CLI fallback.

### Changed

- Operate one conservatively limited public node for the initial launch and
  document its single-failure-domain trade-off.

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

[Unreleased]: https://github.com/envoy1084/envshare/compare/v0.1.4...HEAD
[0.1.4]: https://github.com/envoy1084/envshare/releases/tag/v0.1.4
[0.1.3]: https://github.com/envoy1084/envshare/releases/tag/v0.1.3
[0.1.2]: https://github.com/envoy1084/envshare/releases/tag/v0.1.2
[0.1.1]: https://github.com/envoy1084/envshare/releases/tag/v0.1.1
[0.1.0]: https://github.com/envoy1084/envshare/releases/tag/v0.1.0
