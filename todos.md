# Envshare implementation roadmap

This file is the authoritative sequential implementation plan for Envshare. Work
proceeds from top to bottom. A phase is complete only when its implementation,
tests, documentation, and relevant verification commands pass.

## Phase 0 — Repository and specification foundation

- [x] Create the Rust 2024 workspace, pin the toolchain, centralize dependencies,
      enable workspace lints, and commit `Cargo.lock`.
- [x] Add initial crates with enforced dependency direction: `code`, `crypto`,
      `protocol`, `core`, `network`, `cli`, `node`, and `testkit`.
- [x] Add formatting, Clippy, test, documentation, audit, license, and dependency
      policy configuration.
- [x] Define shared hard limits, typed error boundaries, safe exit codes, and a
      no-secret logging policy.
- [x] Publish v1 specifications for capability encoding, canonical transcripts,
      cryptographic domains, messages, CDDL, lifecycle semantics, and test-vector
      format.
- [x] Add `README.md`, contribution guidance, the Apache 2.0 license, `SECURITY.md`, threat
      model, architecture overview, and protocol compatibility policy.
- [x] Add pull-request CI for Linux, macOS, and Windows with formatting, Clippy,
      tests, docs, dependency audit, and policy checks.

## Phase 1 — Capability, cryptography, protocol, and lifecycle

- [x] Implement the versioned 160-bit Crockford Base32 capability format with a
      checksum, aliases, redacted secret types, and zeroization.
- [x] Implement domain-separated HKDF derivation for room, authentication, and
      session material with deterministic golden vectors.
- [x] Implement bounded canonical transcript construction and HMAC proofs binding
      network, room, version, peer identities, nonces, claim, and ciphertext.
- [x] Implement XChaCha20-Poly1305 envelope encryption/decryption and digest
      verification with secret-safe errors.
- [x] Implement bounded CBOR Open, Offer, Acknowledge, Completed, and protocol
      error messages with four-byte framing and rejection before allocation.
- [x] Implement the fail-closed sender actor, exact-offer retry cache, idempotent
      acknowledgement, expiration, and delivery-unknown transitions.
- [x] Implement receiver-side proof verification, key derivation, decryption, and
      envelope validation.
- [x] Add unit, property, concurrency, golden-vector, malformed-input, oversized
      frame, two-receiver race, retry, and acknowledgement-loss tests.
- [x] Add fuzz targets and seed corpora for capability, transcript, CBOR, and frame
      decoders.

## Phase 2 — Direct transfer and safe local workflows

- [x] Implement the Tokio-owned libp2p Swarm with QUIC, TCP, Noise, Yamux,
      Identify, Ping, connection limits, and the custom request-response codec.
- [x] Implement a bounded command/event API around the single-owner network loop,
      cancellation, timeouts, backpressure, and graceful shutdown.
- [x] Implement direct sender and receiver flows using explicit multiaddresses.
- [x] Implement bounded input reading, raw dotenv preservation, selected-key
      normalization, metadata validation, and secret-safe diagnostics.
- [x] Implement atomic private file output on Unix and Windows, no-clobber by
      default, explicit replacement, symlink/reparse-point defenses, and cleanup.
- [x] Implement direct child execution with received variables, overlay/override/
      clean modes, signal forwarding, and correct exit-status propagation.
- [x] Implement initial `send`, `receive`, and `run` CLI commands, hidden prompts,
      stdin automation, human output, JSON events, and stable exit codes.
- [x] Add localhost/LAN QUIC and TCP integration tests, CLI tests, cancellation
      tests, permission tests, exact payload boundary tests, and secret sentinel
      tests.

## Phase 3 — Relay connectivity

- [x] Implement stable Ed25519 node identity generation, inspection, secure
      persistence, configuration validation, and explicit startup failure.
- [x] Implement the Circuit Relay v2 server with bounded reservations, circuits,
      durations, bytes, per-peer limits, and rate limiting.
- [x] Implement client relay reservation, renewal, circuit listener addresses,
      relay dialing, route racing, and opportunistic DCUtR upgrades.
- [x] Implement node connection/memory limits and safe structured events.
- [x] Add relay-only integration, renewal, saturation, byte-limit, duration-limit,
      restart, and direct-transport-blocked tests.

## Phase 4 — Federated discovery

- [x] Implement the discovery abstraction and bounded Rendezvous client.
- [x] Implement patched, bounded Rendezvous server behavior with namespace,
      registration, cookie, address, TTL, global, per-peer, and rate limits.
- [x] Implement opaque-room registration, renewal, unregister, parallel multi-node
      discovery, validation, deduplication, malicious-result handling, and bounded
      candidate authentication.
- [x] Display a share code only after public reachability requirements are met.
- [x] Add optional mDNS LAN discovery and relay-only privacy mode.
- [x] Add multi-node outage, stale registration, malicious registration, wrong
      network, renewal, collision, and discovery overload tests.

## Phase 5 — Complete production CLI

- [x] Finish the documented `send`, `receive`, `run`, `doctor`, `network`, and
      `completions` command trees and configuration precedence.
- [x] Implement public/direct/relay diagnostics and disposable discovery tests in
      `doctor` without user-derived identifiers.
- [x] Complete TTY detection, `NO_COLOR`, code-only mode, JSON streaming, safe
      debug behavior, exact delivery wording, and stable error/exit contracts.
- [x] Complete Unix and Windows process-group behavior and receiver filesystem
      safety on every supported target.
- [x] Add shell completions and man pages.
- [x] Add end-to-end CLI suites covering success, races, interruption, missing
      nodes, overwrite refusal, child processes, and zero secret leakage.

## Phase 6 — Production node and operations

- [x] Implement validated node configuration with absolute safety ceilings.
- [x] Implement bounded per-IP/per-peer admission control and overload shedding.
- [x] Implement loopback health, readiness, graceful drain, and OpenMetrics
      endpoints with low-cardinality labels.
- [x] Add safe JSON logs and optional OTLP tracing behind a feature flag.
- [x] Add Dockerfile, Compose, native systemd service, sample configuration,
      firewall guidance, hardening, resource limits, and health checks.
- [ ] Add Prometheus alerts, a Grafana dashboard, backup/restore, rolling upgrade,
      identity rotation, overload, abuse, and incident runbooks.
- [ ] Add load and multi-day soak harnesses and document tested capacity limits.

## Phase 7 — Distribution, publishing, and public beta

- [ ] Configure `cargo-dist` for signed archives/installers across supported Linux,
      macOS, and Windows architectures.
- [ ] Add GitHub release workflows for tagged builds, checksums, SBOMs,
      provenance/attestations, artifact signing, and release notes.
- [ ] Add a versioned HTTPS `install.sh` supporting platform detection, checksum
      verification, explicit install location, and non-interactive operation.
- [ ] Add PowerShell installation for Windows with equivalent verification.
- [ ] Add Homebrew formula generation and document package-manager installation.
- [ ] Add release dry-run, installer smoke tests, clean-machine tests, and rollback
      documentation.
- [ ] Complete user, protocol, self-hosting, operations, release, troubleshooting,
      privacy, and security documentation.
- [ ] Run license, advisory, supply-chain, fuzz, coverage, cross-platform, NAT,
      overload, soak, and release-candidate gates.
- [ ] Record independent security review findings and close all high-severity
      issues before describing Envshare as production-ready for secrets.

## Final completion audit

- [ ] Demonstrate every functional, one-time, security, reliability, and operations
      acceptance criterion from the architecture research with direct evidence.
- [ ] Verify a sender-to-receiver transfer by code through direct, TCP fallback,
      and relay-only paths on clean supported systems.
- [ ] Verify two simultaneous valid receivers produce exactly one claimant and no
      disclosure state ever returns to available.
- [ ] Verify no payload, capability, room identifier, environment key/value, path,
      or long-lived client identifier appears in logs, metrics, errors, analytics,
      crash context, or default automation output.
- [ ] Verify repository status is clean, documentation matches released behavior,
      all required checks pass, release artifacts install correctly, and no TODO
      above remains incomplete.
