# Security boundary

## What Envshare protects

- The payload is encrypted between peers with XChaCha20-Poly1305.
- A receiver proves possession of the share code before receiving the payload.
- Protocol proofs bind the network, room, peer identities, claim, expiry, and
  ciphertext.
- The first valid receiver consumes the share.
- New output files are private and written atomically.
- Normal logs and JSON events omit codes, values, addresses, and discovery
  namespaces.

Libp2p transport encryption is used in addition to the application protocol.
The application handshake is still required because transport identity does
not prove possession of the share code.

## What Envshare does not protect

- A compromised sender or receiver.
- Plaintext after the receiver writes it or starts a child process.
- Source IP addresses, timing, duration, transport choice, or approximate byte
  counts visible to network infrastructure.
- Credentials copied into logs, screenshots, shell history, or public chat.
- Long-term credential storage, access control, rotation, or revocation.

The public node is an untrusted connectivity service. It can observe network
metadata and interfere with availability, but a correct client does not send it
the plaintext payload or share code.

## Handle the share code as a password

Anyone with the complete code can attempt to claim the share. Send it through a
private authenticated channel. Prefer the hidden prompt or `--code-stdin` over
`--code CODE`, because command arguments may be recorded by the shell or process
inspection tools.

If a code leaks:

1. Stop the sender with `Ctrl+C`.
2. If the sender completed, assume the payload was received.
3. Rotate every credential in the payload at its source.
4. Create another share only after the leaked values are invalid.

Expiration does not revoke plaintext already obtained by a receiver.

Report vulnerabilities according to [SECURITY.md](../../SECURITY.md).
