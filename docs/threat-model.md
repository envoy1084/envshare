# Threat model

## Protected assets

- Environment payload bytes.
- The one-time capability and derived keys.
- Sender and receiver process environments.
- Stable public-node identity keys.

## Adversaries

Envshare considers passive observers, malicious discovery and relay nodes,
malformed Internet peers, unauthorized claim attempts, two holders racing with a
leaked capability, resource-exhaustion attacks, and accidental local disclosure
through output, arguments, permissions, or telemetry.

## Required properties

- Libp2p transport confidentiality and peer authentication.
- Application proof that both endpoints possess the capability.
- Proofs bound to protocol version, network, room, both peer identities and fresh
  nonces.
- Application AEAD for payloads and authenticated acknowledgements.
- A sender-enforced, fail-closed single claim.
- Hard bounds on frames, decoded collections, queues, connections, registrations,
  reservations and circuits.
- No capability, room identifier, payload, environment name/value, or sender path
  in ordinary logs, metrics, errors, panic context, or analytics.
- Private, atomic receiver output and direct child spawning without a shell.

## Not provided

- Endpoint security or protection after authorized disclosure.
- Anonymity or concealment of timing and traffic size.
- Availability against large denial-of-service attacks.
- Recovery when an attacker uses a leaked capability first.
- Guaranteed physical erasure from RAM, swap, terminal history, kernel buffers,
  filesystems, child environments, debuggers, or crash reporters.

The capability is a bearer credential. A checksum detects typing errors but adds
no authorization strength. Human-scale word or numeric codes are excluded until
a reviewed PAKE and online-guessing controls exist.

## Trust boundaries

Clients trust their local operating system and the intended remote endpoint after
capability authentication. Discovery and relay nodes are untrusted for content.
Node identity keys authenticate published node addresses but do not authorize a
receiver. Public infrastructure persists only its identity and configuration;
registrations, reservations and circuits are ephemeral.
