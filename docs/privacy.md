# Privacy and data handling

Envshare has no accounts, hosted payload database, offline mailbox, analytics
SDK, advertising identifier, or default crash reporter. That narrow product
surface reduces retained data; it does not provide anonymity.

## Data handled by clients

The sender reads the selected dotenv bytes into bounded memory and remains online
until claim, expiration, cancellation, or failure. The receiver obtains plaintext
only after capability authentication and encrypted transport, then either writes
the chosen private file or adds values to a child-process environment. Files,
process environments, terminal output, pipes, swap, backups, endpoint security
software, and authorized applications are outside protocol erasure guarantees.

Client Peer IDs and discovery namespaces are derived for a share and are not
intended as long-lived analytics identifiers. Configuration stores public network
names, node addresses, expected Peer IDs, and preferences locally; capability
codes and payload values are invalid configuration fields.

## Data visible to nodes and networks

Discovery and relay nodes are untrusted infrastructure. They can observe source
IP addresses, ephemeral Peer IDs, connection timing, transport addresses, bounded
registration metadata, circuit duration, and approximate traffic volume. A
Rendezvous node learns the opaque namespace used for matching; a relay carries
encrypted traffic. Neither should receive the capability, encryption keys,
environment names, values, or plaintext payload.

Network operators, DNS resolvers, hosting providers, ISPs, local routers, and a
shared-NAT peer can correlate timing and addresses. mDNS additionally announces a
disposable service on the local network. Envshare does not hide that two endpoints
communicated and makes no anonymity or traffic-analysis claim.

## Retention

- A sender holds bounded transfer state only for its lifetime and irreversibly
  closes availability on the first valid claim attempt.
- A receiver retains whatever file or child-process copies the user chooses.
- Nodes retain their stable identity key and configuration. Discovery records and
  relay reservations expire; safe operational metrics are aggregate and logs are
  subject to the operator's configured retention.
- Cryptographic buffers use best-effort zeroization where practical. Compilers,
  operating systems, terminals, child processes, swap, hibernation, and hardware
  may retain copies, so zeroization is not guaranteed erasure.

## Logs, metrics, and diagnostics

Normal errors, JSON events, logs, metrics, traces, panic formatting, and default
automation output must not include payload bytes, capabilities, full room IDs,
environment names or values, sender paths, or raw peer error chains. Node labels
come from bounded enums rather than peer or address identifiers. See the
[secret-output policy](secret-output-policy.md) and its
[completed output-surface audit](secret-output-audit.md). The intentional sender
capability channel is the necessary exception and must be treated as secret.

`envshare doctor --verbose` may show configured public node identities by explicit
request. Installer downloads disclose the requested version, IP address, and
ordinary HTTP metadata to GitHub and network intermediaries. Homebrew has its own
package-manager behavior and policies.

## Operator responsibility

Self-hosted node operators control infrastructure logs, metrics scraping, tracing,
backups, and retention. They should minimize access and duration, protect the
identity key, avoid packet capture except during approved incidents, and publish
their own privacy notice if offering a public service. Envshare itself has no
central controller able to answer account-data requests or delete receiver files.
