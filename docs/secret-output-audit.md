# Secret output audit

This audit covers the `v0.1.1` client and node source. It verifies that classified
data does not reach unintended output surfaces. The capability has one necessary
exception: a sender must reveal it to the user. Human `send` output and
`--code-only` are therefore secret-bearing, and JSON-mode send keeps the
capability on stderr while stdout contains lifecycle records. Those streams must
be handled as documented secret channels; their deliberate disclosure is not a
logging or telemetry disclosure.

## Classified data and controls

| Data class | Control and direct evidence |
| --- | --- |
| Payload, environment names, and values | `SecretEnvelope` has a redacted `Debug` implementation. `debug_redacts_payload_and_name`, direct-transfer sentinel tests, race/failure tests, and exact-payload CLI tests prove these values do not enter diagnostics. |
| Capability | `ShareCodeSecret` and `ShareCode` redact `Debug`; invalid-code and failure-path tests prove errors never echo supplied or valid codes. Emission is limited to the documented sender capability channels above. |
| Room identifier and discovery namespace | `DiscoveryNamespace` and Rendezvous request debug output redact their values; `namespace_debug_never_exposes_room_identifier` and `request_debug_redacts_namespace_and_record` exercise both boundaries. |
| Environment file path or suggested name | I/O failures map to fixed `CoreError`/`CliFailure` classifications. `SecretEnvelope` redacts the suggested name and rejects path-like names. No terminal or telemetry emission site receives the input path. |
| Client identity | Clients generate a fresh libp2p identity per invocation and have no identity persistence input. The routing Peer ID and address deliberately printed by a direct sender are per-process coordinates, not long-lived analytics identifiers. The node's stable, public operator identity is outside this client-identifier criterion. |
| Raw network errors and cryptographic material | Crate boundaries return fieldless, fixed error enums. Key and transcript types use redacted formatting or expose bytes only to narrow cryptographic operations. |

## Output surface review

| Surface | Result and evidence |
| --- | --- |
| Human stdout/stderr | Fixed messages and bounded routing coordinates only, apart from the intentional sender capability channels. Process-level sentinel tests cover success, overwrite refusal, missing discovery, interruption, and receiver races. |
| JSON and default automation | Event objects are fixed schemas. Client JSON stdout contains bounded event names, status, payload-format enums, and ephemeral routing coordinates; the sender capability is isolated to stderr. Node JSON tests prove peer IDs, addresses, and result counts are not serialized. |
| Logs and traces | Client tracing is off by default and receives no secret fields. Node emission sites record only `node_service` and a bounded event enum; optional OTLP exports the same spans. No arbitrary error chain, identifier, namespace, path, or payload is attached. |
| Metrics and health | `NodeStatus::metrics` emits aggregate numeric gauges and counters with no dynamic labels. Operations tests exercise the complete endpoint output. |
| Errors and panic context | Public errors are fixed, fieldless classifications. The workspace contains no `panic!`, `unwrap`, or `expect` call in any crate, so application code does not construct a panic message from classified input. Secret-bearing types remain redacted if dependency debug formatting is invoked. |
| Analytics and crash reporting | The product has no analytics SDK, advertising identifier, crash reporter, or configuration for one. The only exporter is the node's optional OTLP tracing feature, which uses the bounded fields above and is disabled without an explicit endpoint. |
| Installers and release automation | Installers print version, target, and destination but never receive a payload or capability. Release qualification uses synthetic secrets, captures transfer output in temporary files, emits only fixed pass/fail messages, and prints only secret-safe stderr classifications on failure. |

## Audit method and limits

The audit inventories every `println!`, `eprintln!`, and `tracing` emission site
under `crates/`, reviews every error boundary and custom `Debug` implementation,
checks the dependency graph for analytics/crash-reporting integrations, and maps
those source invariants to the sentinel and telemetry tests named above. The
checks passed for `v0.1.1` on 2026-08-22.

This is direct repository evidence, not an independent security assessment. It
does not cover an operator adding packet capture, shell tracing, external crash
collection, or another wrapper that records the intentional capability channel.
Those systems remain subject to the operator guidance in the privacy and security
documents.
