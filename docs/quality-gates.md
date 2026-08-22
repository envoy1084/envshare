# Release quality gates

Envshare keeps pull-request CI deliberately small. CI runs formatting, Clippy,
documentation, a focused security-critical test subset, dependency policy, and
Linux/Windows installer smoke tests. The exhaustive and privileged checks below
run for release candidates and are not scheduled or duplicated in CI.

## Required release-candidate gates

Run every command from a clean checkout at the candidate commit and attach the
complete output, commit ID, host details, and tool versions to the release issue.

| Gate | Command or evidence | Pass condition |
|---|---|---|
| License and sources | `cargo deny check` | advisories, bans, licenses, and sources pass; project files remain Apache-2.0-only |
| Advisories | `cargo audit` | no known vulnerable dependency |
| Supply chain | `cargo tree -i libp2p-rendezvous --locked` and workflow review | Rendezvous is at least 0.17.1, lockfiles are used, Actions use commit SHAs, release emits checksums, SBOMs, and attestations |
| Full tests | `cargo test --workspace --all-targets --all-features --locked` | all tests pass |
| Coverage | `cargo llvm-cov --workspace --all-targets --all-features --locked --fail-under-lines 80 --summary-only` | line coverage is at least 80% |
| Fuzzing | `scripts/fuzz-check.sh 60` | all four targets finish without a crash or sanitizer finding |
| Cross-platform | focused GitHub matrix plus candidate artifact tests | Linux, macOS, and Windows checks pass on the candidate commit |
| NAT/TCP fallback | build the release client, then run `sudo scripts/test-nat.sh` on isolated Linux | transfer succeeds across separate namespaces with UDP blocked and emulated latency |
| Published transfer | manually dispatch `release-qualification.yml` for the tag | installed QUIC/TCP transfers pass on Linux, macOS, and Windows; the published Linux node completes a relay-only transfer |
| Overload | commands in [load testing](load-testing.md) | bounds hold, zero harness failures, and cleanup returns resources near baseline |
| Soak | 24-hour procedure in [load testing](load-testing.md) | no crash, restart, OOM, unbounded RSS trend, or descriptor leak |
| Release | `scripts/release-check.sh` plus published installer/attestation checks in [release](release.md) | complete plan, install/rollback checks, checksums, SBOMs, and attestations pass |

The fuzz harness requires `nightly-2026-08-21` and `cargo-fuzz 0.13.2`. It copies
the reviewed seed corpus to a private temporary directory, so a qualification run
cannot silently add generated corpus files to the release commit.

## Recorded development evidence

These results are reproducibility evidence for the harnesses, not a substitute
for rerunning them on the final candidate or for the external security review.

| Date | Environment | Result |
|---|---|---|
| 2026-08-22 | macOS 26.5.2 arm64, Apple M3 Pro, Rust 1.96.0 | full all-feature workspace suite passed; 81.22% line coverage passed the 80% floor |
| 2026-08-22 | same, nightly-2026-08-21, cargo-fuzz 0.13.2 | capability, transcript, CBOR, and frame targets each ran 20 seconds with no crash |
| 2026-08-22 | same, post-release qualification commit `d4492b4` | full suite, Clippy, and rustdoc passed; 81.26% line coverage passed the 80% floor |
| 2026-08-22 | same, nightly-2026-08-21, cargo-fuzz 0.13.2 | all four targets each ran the required 60 seconds with no crash or sanitizer finding |
| 2026-08-22 | same, cargo-audit 0.22.2, cargo-deny 0.19.9 | advisory, license, source, and ban policies passed; Rendezvous resolved to 0.17.1 |
| 2026-08-22 | privileged Debian Bookworm arm64 container on Linux 7.0.14 OrbStack | namespace NAT gate passed with UDP blocked, TCP DNAT/SNAT, and 25 ms latency |
| 2026-08-22 | macOS arm64 and constrained Linux arm64 container | overload smoke results passed as recorded in [load testing](load-testing.md) |
| 2026-08-22 | macOS arm64, cargo-dist 0.32.0 | clean-tree release plan and artifact-lie validation passed for the initial release configuration |

The released commit passed cross-platform hosted checks, and clean published-asset
installation, checksums, and attestations were verified. A 24-hour qualification
soak and external security review are deliberately not claimed. Envshare must not
be described as production-ready for high-value secrets until those gates have
independent records. See the [criterion-level evidence](acceptance-evidence.md).
