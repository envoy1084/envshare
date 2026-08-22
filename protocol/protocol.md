# Envshare transfer protocol v1

## Constants and bounds

- Stream protocol: `/envshare/transfer/1.0.0`.
- Protocol version field: unsigned integer `1`.
- Maximum plaintext payload: 1,048,576 bytes.
- Maximum Open, Acknowledge, Completed or error body: 1,024 bytes.
- Maximum Offer body: 1,064,960 bytes.
- Maximum canonical transcript: 8,192 bytes.
- Maximum network identifier: 64 printable ASCII bytes.
- Maximum suggested name: 128 UTF-8 bytes with no separator or control character.

## Framing

Every request-response body is a four-byte unsigned big-endian length followed by
exactly that many CBOR bytes. A decoder validates a message-specific limit before
allocation, rejects zero length, reads exactly the body, limits CBOR nesting and
collection lengths, and rejects trailing bytes.

The CBOR body is a two-element array containing a numeric message discriminator
and the corresponding numeric-key map. Request discriminators are `0` for Open
and `1` for Acknowledge. Response discriminators are `0` for Offer, `1` for
Completed, and `2` for a protocol error. Definite-length arrays, maps, strings,
and byte strings are required; fields occur in numeric-key order.

## Root derivation

Let `secret` be the 20 capability bytes and `network_id` a public stable network
identifier. Length fields below are four-byte unsigned big-endian integers.

```text
root_prk = HKDF-Extract(SHA-256("envshare/root-salt/v1"), secret)
room_id = HKDF-Expand(root_prk,
  "envshare/room-id/v1" || len(network_id) || network_id, 16)
auth_key = HKDF-Expand(root_prk,
  "envshare/auth-key/v1" || len(network_id) || network_id, 32)
session_base_key = HKDF-Expand(root_prk,
  "envshare/session-base-key/v1" || len(network_id) || network_id, 32)
```

The discovery namespace is
`envshare/1/<network-id>/<un-padded-base32-room-id>`.

## Canonical transcripts

A transcript begins with a four-byte length and the ASCII domain string. Each
subsequent byte string is prefixed by its four-byte length. A `u16`, `u32` or
`u64` field is encoded at its fixed width in big-endian order without another
length. Fields occur exactly in the order shown by the relevant proof definition.
The builder rejects output exceeding 8,192 bytes.

## Exchange

The receiver establishes a libp2p connection to the discovered sender Peer ID and
sends `Open` with a fresh 32-byte receiver nonce. Its HMAC-SHA-256 proof covers
the `envshare/open/v1` transcript: version, network ID, room ID, authenticated
sender Peer ID bytes, authenticated receiver Peer ID bytes, and receiver nonce.

After proof verification, the first valid Open atomically binds the share to the
receiver Peer ID and nonce. The sender generates a 32-byte sender nonce and
16-byte claim ID. Session extraction uses SHA-256 of
`"envshare/session-salt/v1" || receiver_nonce || sender_nonce` as HKDF salt.
Separate payload and acknowledgement keys are expanded from the session PRK with
`envshare/payload-key/v1` and `envshare/ack-key/v1` transcripts binding network,
room, both peers and claim.

The sender encodes the envelope, authenticates its metadata as AEAD associated
data, and encrypts it with XChaCha20-Poly1305 under a fresh 24-byte nonce. The
Offer proof under `auth_key` binds all routing, claim, expiry, content, nonce and
ciphertext-digest fields. State becomes `Disclosed` before the Offer is handed to
the network.

After private atomic persistence or successful child spawn, the receiver sends an
HMAC-SHA-256 acknowledgement under `ack_key`, binding protocol, network, room,
both peers, claim and ciphertext digest. A duplicate valid acknowledgement is
idempotent.

## Single-claim lifecycle

`Available` accepts one valid Open. `Disclosed` never returns to `Available`; only
the same Peer ID and receiver nonce may retrieve the exact cached Offer during a
bounded resume window. A valid acknowledgement becomes `Consumed`. Timeout after
disclosure becomes `DeliveryUnknown`. Unclaimed expiry becomes `Expired`.

Before capability authentication, invalid room and proof conditions return the
same `NotFoundOrUnauthorized` error. No protocol error contains secret material or
sender file paths.
