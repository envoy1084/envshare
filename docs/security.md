# Security guide

Envshare is designed to reduce the exposure of a short-lived, one-time dotenv
handoff. This initial 0.x release has not completed independent security review and is not
approved for production secrets. The [threat model](threat-model.md) is the
authoritative statement of protected assets, adversaries, and exclusions.

## Capability safety

The share code is a bearer capability: possession is authorization to attempt the
single claim. Transfer it through a separate authenticated channel, use the hidden
prompt or `--code-stdin`, keep the default short lifetime, and cancel the sender if
the code may have leaked. Do not pass it as a command-line argument on shared
systems or place it in URLs, chat history, screenshots, tickets, CI variables with
echo enabled, or shell traces.

Node addresses and expected stable Peer IDs also need an authentic configuration
channel. Encryption does not help if a user accepts an attacker-selected endpoint
or gives the capability to the attacker.

## What the protocol enforces

- A versioned, bounded framing and CBOR protocol rejects oversized or unsupported
  messages before unbounded allocation.
- The capability authenticates canonical transcripts; HKDF derives direction and
  context-separated keys; XChaCha20-Poly1305 protects the payload with associated
  transcript data.
- The sender commits to one claimant before disclosure and never returns a share
  to available, including after an ambiguous disconnect.
- Direct QUIC or TCP connections use libp2p security and multiplexing. Circuit
  relay and Rendezvous nodes remain outside the plaintext trust boundary.
- Receiver writes are private, no-clobber, and atomic by default. `envshare run`
  executes without an implicit shell and contains descendants for interruption.

These properties do not protect a compromised endpoint, authorized receiver,
leaked capability, malicious child process, observable traffic metadata, terminal
history, swap, backups, or secrets copied after delivery.

## Safe client operation

Inspect selected keys before sending, prefer a clean dedicated working directory,
and do not elevate the CLI. Treat delivery-unknown as potentially delivered.
Avoid `--force`, `--override`, `--clean-env`, `--lan`, public mDNS, long expiration,
or relay-only operation unless their specific tradeoff is intended. Keep host
patches, clocks, DNS, and endpoint protection current.

Use release artifacts only after SHA-256 and GitHub attestation verification.
Installers do not bypass platform signing warnings, change PATH, or weaken policy.
Pin both version and signer workflow in automated acquisition.

## Safe node operation

Nodes must run with the supplied absolute safety ceilings, admission controls,
resource limits, persistent private identity, loopback-only operations endpoint,
and explicit firewall rules. Back up the identity offline, restrict configuration
and key permissions, patch dependencies, alert on saturation, and rotate only
through the documented identity procedure. Do not add payload logging, peer-ID
metric labels, debug packet capture, public metrics, or unbounded tracing.

The node is intentionally not an account authority or secret store. A public node
operator can still observe network metadata and deny service, so clients should
federate independently operated nodes where practical.

## Disclosure and incident response

Suspected vulnerabilities must be reported privately under the repository
[security policy](../SECURITY.md) without real credentials. For a leaked share,
cancel the sender and rotate the underlying secret based on the destination
system's policy; merely expiring a capability cannot revoke plaintext already
received. Node and release incidents follow the [operations](operations.md) and
[release withdrawal](release.md#rollback-and-withdrawal) runbooks.

Production-ready claims require the final acceptance gates, independent security
review, closure of all high-severity findings, and evidence on every supported
release target. Passing unit tests or using standard cryptographic primitives is
not a substitute for that review.
