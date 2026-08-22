# Architecture

Envshare v1 is a sender-online, capability-authenticated, single-claim transfer.
The sender keeps the payload in memory and advertises reachable addresses under
an opaque, high-entropy Rendezvous namespace. The receiver derives that namespace
from the capability, discovers candidates, authenticates the actual sender, and
receives an application-encrypted envelope.

## Planes

The data plane is a direct libp2p QUIC/TCP connection or a Circuit Relay v2
circuit. Payload confidentiality does not rely on a relay: Envshare applies AEAD
to the envelope in addition to libp2p transport security.

The discovery plane is a small federated set of Rendezvous nodes. A node can
observe metadata, censor, return false candidates, or disappear. It cannot derive
the capability or authenticate and decrypt the payload. Multiple nodes improve
availability; they do not make the system anonymous.

Clients retain the standard libp2p Rendezvous v1 wire protocol. Public nodes use
an Envshare-specific inbound implementation because the generic server cannot
validate application namespaces and signed address records before storage. The
node accepts only the fixed `envshare-v1-` opaque namespace shape and bounds TTL,
record bytes, addresses, registrations per peer/namespace/globally, response
results, cookies, per-peer request rates, and the rate-bucket map. Public defaults
reject loopback, unspecified, link-local, multicast, private, and mismatched-peer
routes; private self-hosted nodes must opt in to local addresses explicitly.

## Workspace boundaries

- `code`: capability generation, canonical encoding, parsing and secret
  ownership.
- `crypto`: transcripts, derivation, proofs, digest and AEAD operations.
- `protocol`: bounded envelope, messages, framing, versions and limits.
- `network`: client Swarm, transports, discovery, relay and route logic.
- `core`: sender actor, receiver service, files and child execution.
- `cli`: commands, configuration, terminal presentation and exit codes.
- `node`: stable untrusted discovery/relay service and operations.
- `testkit`: deterministic clocks and integration network fixtures.

The Swarm and sender lifecycle each have a single owner. Bounded channels connect
the network event loop to services. A libp2p Swarm is never shared behind a mutex.

DNS multiaddresses are resolved in the bounded route layer using a current
resolver rather than rust-libp2p's optional DNS transport while that transport is
coupled to an affected Hickory release line. Resolved addresses remain bound to
the expected `/p2p/<peer-id>`, so redirected DNS cannot authenticate the wrong
node.

## One-time invariant

The first valid `Open` binds the share to the authenticated receiver peer and its
nonce. Before ciphertext can be handed to the network, state becomes
`Disclosed`. It never returns to `Available`. The same claim may briefly retrieve
the exact cached ciphertext; another claim cannot. Missing acknowledgement ends
as `DeliveryUnknown`, not as an available share.

The complete implementation architecture is retained in
[`research/envshare-production-architecture.md`](../research/envshare-production-architecture.md).
