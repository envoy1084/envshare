# Protocol compatibility policy

The v1 transfer stream identifier is `/envshare/transfer/1.0.0`. Its version is a
wire-compatibility version, not the CLI package version.

- An incompatible message or framing change receives a new major stream ID.
- Optional compatible fields require explicit bounds and must be safely ignored by
  older peers before they are admitted to the same protocol ID.
- Unknown enum variants and unsupported major versions are rejected.
- Cryptographic domain strings are independently versioned and are never reused
  for a changed transcript or purpose.
- Implementations support only explicitly tested adjacent versions.
- A second implementation is not released until normative test vectors are
  published and exercised across both implementations.
