# Acceptance evidence

This document maps the production acceptance criteria in
[`research/envshare-production-architecture.md`](../research/envshare-production-architecture.md)
to evidence. `Verified` means a test, source invariant, or published artifact
directly demonstrates the criterion. `External` means repository work is present,
but the criterion requires an independent reviewer or deployed infrastructure and
is not claimed complete.

The current qualification code and released `v0.1.2` commit is
`8fceb8ab41f33186910672f9a40d3c305816d009`.

## Functional

| Criterion | Status | Direct evidence |
|---|---|---|
| Sender creates a reachable share with one command | Verified | `direct_send_and_receive_preserve_exact_private_payload` in [`crates/cli/tests/commands.rs`](../crates/cli/tests/commands.rs) starts `envshare send` and completes the transfer. |
| Receiver needs only the code and selected network | Verified | `federated_transfer_survives_two_of_three_nodes_unavailable` receives without peer or route arguments. |
| Direct QUIC works | Verified | `quic_transfers_a_bounded_message` in [`crates/network/tests/direct.rs`](../crates/network/tests/direct.rs), plus the published-artifact [release qualification](https://github.com/envoy1084/envshare/actions/runs/32574706429). |
| TCP fallback works | Verified | `tcp_noise_yamux_transfers_a_bounded_message`, the isolated UDP-blocked NAT gate in [`scripts/test-nat.sh`](../scripts/test-nat.sh), and the published-artifact release qualification. |
| Relay-only transfer works | Verified | `relay_only_profile_transfers_without_a_direct_listener`, `relay_reservation_carries_a_transfer_request`, and the published Linux client/node release qualification. |
| Three-node discovery works with two nodes unavailable | Verified | `federated_transfer_survives_two_of_three_nodes_unavailable` configures one live and two unavailable discovery nodes. |
| `run` injects variables without writing a file | Verified | `direct_run_overrides_environment_and_propagates_exit_status`. |
| Selected-key sending works and reports normalization | Verified | `selected_key_send_reports_and_delivers_normalized_payload`; human and JSON contracts are documented in the [user guide](user-guide.md). |

## One-time guarantee

| Criterion | Status | Direct evidence |
|---|---|---|
| Two simultaneous valid receivers yield one claimant | Verified | `two_simultaneous_receivers_create_exactly_one_output` and `concurrent_receivers_are_serialized_to_exactly_one_winner`. |
| No second receiver succeeds after response or acknowledgement loss | Verified | `acknowledgement_loss_closes_real_swarm_share_to_another_receiver` in [`crates/core/tests/direct.rs`](../crates/core/tests/direct.rs). |
| The winning in-process claim resumes with exact cached ciphertext | Verified | `first_valid_receiver_wins_and_exact_retry_reuses_offer` compares the complete cached offer. |
| Sender reports `DeliveryUnknown` rather than reopening | Verified | `disclosed_share_never_reopens_after_acknowledgement_timeout` and the real-Swarm acknowledgement-loss test. |
| Sender restart loses the share | Verified | [`crates/cli/src/commands/send.rs`](../crates/cli/src/commands/send.rs) generates a fresh capability and in-memory actor per invocation; neither client crate has share persistence or a capability-restoration input. |

## Security

| Criterion | Status | Direct evidence |
|---|---|---|
| Capability contains 160 random bits before checksum | Verified | `SECRET_BYTES == 20`, OS generation, fixed vectors, property tests, and the [code format](../protocol/code-format.md). |
| Protocol binds peer IDs, room, network, nonces, and version | Verified | Independent proof vectors and tamper rejection in [`crates/crypto/src/proof.rs`](../crates/crypto/src/proof.rs) and [`crates/core/tests/lifecycle.rs`](../crates/core/tests/lifecycle.rs). |
| Payload has application AEAD in addition to transport encryption | Verified | XChaCha20-Poly1305 round trips and tamper rejection in [`crates/crypto/src/aead.rs`](../crates/crypto/src/aead.rs), with Noise/TLS transports in the network integration tests. |
| Normal output, logs, metrics, errors, analytics, and crash context exclude secrets | Verified | Sentinel CLI failure tests, redacted `Debug` tests, identifier-free node JSON tests, aggregate-only metrics, the [secret-output policy](secret-output-policy.md), and the completed [secret output audit](secret-output-audit.md). The sender's documented capability channel is the necessary intentional exception. This does not substitute for independent review. |
| Every network decoder and collection is bounded | Verified | Protocol header-only oversize rejection, all four fuzz targets, node-store/admission bound tests, and limits in [`crates/protocol/src/limits.rs`](../crates/protocol/src/limits.rs). |
| Receiver output is private and atomic | Verified | Cross-platform implementation in [`crates/core/src/local/output.rs`](../crates/core/src/local/output.rs), permission/no-clobber tests, and Windows hosted compilation/tests. |
| `cargo audit` and `cargo deny` pass | Verified | Both passed on 2026-08-22 at the current qualification commit. |
| Rendezvous includes the CVE fix | Verified | `cargo tree -i libp2p-rendezvous --locked` resolves `0.17.1`. |
| Independent review has no unresolved high-severity issue | External | No independent review has been commissioned or recorded. Envshare is therefore not described as production-ready for high-value secrets. |

## Reliability

| Criterion | Status | Direct evidence |
|---|---|---|
| Relay reservation renewal works | Verified | `client_renews_short_relay_reservation` in [`crates/node/tests/relay.rs`](../crates/node/tests/relay.rs). |
| Sender registration renewal works | Verified | `registration_maintenance_renews_and_unregisters_on_cancellation` proves renewal and cleanup. |
| Node rolling restart is tested | External | Drain, stopped-relay failure, stable identity, and restart procedures exist, but the documented three-node staging exercise has not been recorded. |
| Node overload rejects instead of crashing | Verified | Admission/store saturation tests and the bounded real-node load results in [load testing](load-testing.md). |
| Multi-day soak has no unbounded resource growth | External | The bounded harness and procedure exist; the required 24-hour qualification run has not been recorded. |
| Cancellation leaves no partial output | Verified | Atomic-output tests, unavailable-node output test, and process interruption suites. |

## Operations

| Criterion | Status | Direct evidence |
|---|---|---|
| Health and metrics are useful and non-sensitive | Verified | [`crates/node/tests/operations.rs`](../crates/node/tests/operations.rs), telemetry tests, and identifier-free JSON tests. |
| Capacity and availability alerts are active | External | Versioned Prometheus rules and a dashboard exist under [`deploy/monitoring`](../deploy/monitoring), but alert routing must be activated and exercised by an operator in staging. |
| Identity backup and restore is tested | External | Stable identity round-trip tests and a restore procedure exist; an isolated deployed-host restore record is still required. |
| Docker and systemd deployment docs are tested from a clean VPS | External | Hardened assets and validation commands exist, but no clean public VPS qualification record is attached. |
| Release binaries and images are signed | Verified | All 31 `v0.1.2` release assets have GitHub attestations constrained to the release workflow. The public multi-platform image has GitHub provenance and verifies at `ghcr.io/envoy1084/envshare-node@sha256:6123c0a804908647ccac859b0d665311afa9fec36390700e8e29efcd539b291b`. |
| Incident owner and vulnerability path are published | Verified | [`SECURITY.md`](../SECURITY.md) and the [incident runbook](operations.md). |

## Qualification record

| Gate | Result and evidence |
|---|---|
| Format, Clippy, docs, full tests | Passed on macOS arm64 with Rust 1.96.0 using all targets/features and the locked graph. |
| License, advisories, sources | `cargo deny check` and `cargo audit` passed; duplicate-version notices are warnings accepted by policy. Project source remains Apache-2.0-only. |
| Coverage | 81.26% lines with `cargo-llvm-cov 0.8.7`; required floor is 80%. |
| Fuzz | Capability, transcript, CBOR, and frame targets each ran 60 seconds with `cargo-fuzz 0.13.2` and no crash or sanitizer finding. |
| NAT/TCP | Privileged isolated Linux namespace gate passed with UDP blocked, TCP DNAT/SNAT, and 25 ms latency, as recorded in [quality gates](quality-gates.md). |
| Overload | macOS and constrained Linux relay/discovery smoke runs passed with bounded acceptance and cleanup, as recorded in [load testing](load-testing.md). |
| Cross-platform | The focused Linux/macOS/Windows [CI run](https://github.com/envoy1084/envshare/actions/runs/32635184286) passed, including the hosted Windows private-output ACL regression and clean installer smokes. |
| Published transfer | The [release qualification](https://github.com/envoy1084/envshare/actions/runs/32635186201) installed `v0.1.2` from GitHub and passed its published-artifact transfer gates. A clean local profile also completed an exact-payload, relay-only transfer through the deployed public node using no network flag; the receiver wrote mode `0600` and the sender confirmed consumption. |
| Distribution | The [release workflow](https://github.com/envoy1084/envshare/actions/runs/32635186201) published the [`v0.1.2` release](https://github.com/envoy1084/envshare/releases/tag/v0.1.2) with 31 assets and verified their attestations. The [container workflow](https://github.com/envoy1084/envshare/actions/runs/32635186200) published the signed `linux/amd64` and `linux/arm64` image index at digest `sha256:6123c0a804908647ccac859b0d665311afa9fec36390700e8e29efcd539b291b`; the deployed node reports healthy and ready with its stable peer identity. |
| Release dry-run | `scripts/release-check.sh` passed from a clean tree at the current qualification commit and validates both published-transfer harnesses. |
| External maturity | Deployed soak operation and independent review are not repository implementation tasks. They remain explicit prerequisites for a future production-secret approval. |

## Release classification

`v0.1.2` is the recommended public initial release. Its implemented behavior,
published installers, transfer paths, and signed artifacts are qualified. It is
not a production-secret approval; that label still requires the external criteria
above, especially independent review and a deployed soak.
