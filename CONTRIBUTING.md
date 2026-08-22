# Contributing to Envshare

Envshare handles developer secrets and accepts a smaller margin for ambiguity
than an ordinary file-transfer tool. Changes should preserve bounded resource
use, fail-closed one-time semantics, and secret-safe output.

## Workflow

1. Work through a focused change from `todos.md` or an agreed issue.
2. Add tests for meaningful behavior and failure cases.
3. Run formatting, Clippy, tests, documentation, and dependency policy checks.
4. Use a Conventional Commit describing one self-contained change.
5. Update protocol or operator documentation when behavior changes.

Do not include real credentials in fixtures, examples, logs, snapshots, issues,
or pull requests. Test secrets must be conspicuous sentinel values generated for
the test and assertions must prove they do not escape intended buffers.

## Protocol changes

Wire or cryptographic changes require:

- an explicit compatibility decision;
- updated CDDL and normative prose;
- deterministic cross-language test vectors;
- negative and malformed-input tests; and
- threat-model review.

Do not reuse an existing protocol identifier or cryptographic domain string for
an incompatible meaning.

## Checks

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo nextest run --workspace --all-targets --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo deny check
```
