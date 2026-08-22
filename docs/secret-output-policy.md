# Secret output policy

The payload, capability, derived keys, full room identifier, environment names and
values, sender file paths, and raw peer error chains are classified as secret or
sensitive. They must not be recorded in normal terminal diagnostics, JSON events,
logs, metrics, traces, panic messages, snapshots, analytics, or crash context.

Secret-bearing types do not derive `Debug`, do not implement ordinary `Display`,
and expose bytes only to narrow cryptographic operations. Errors crossing crate
boundaries are typed classifications without secret fields. Binaries translate
them into fixed human messages, stable JSON codes, and stable process statuses.

The share code may be emitted only by the explicit interactive display or
secret-bearing automation mode. Operational events use counts and bounded enums,
not names, paths, room IDs, peer IDs as analytics identifiers, or arbitrary error
strings. Metric labels are drawn from fixed low-cardinality sets.

Tests use unique sentinel secrets and assert their absence from stdout, stderr,
JSON, tracing output, errors, metrics, and panic formatting. Debug builds do not
relax this policy.
