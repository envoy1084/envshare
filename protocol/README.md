# Envshare protocol

This directory contains the normative, language-independent v1 protocol:

- `code-format.md`: human capability representation.
- `protocol.md`: handshake, key schedule, messages and lifecycle.
- `messages.cddl`: authoritative CBOR data model.
- `test-vectors.json`: versioned format for deterministic golden vectors.

Normative integers are unsigned and encoded in network byte order where an outer
binary framing field is specified. Security transcripts are not CBOR; they use
the canonical length-prefixed construction defined in `protocol.md`.
