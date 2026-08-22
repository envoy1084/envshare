# Node operations runbook

This runbook covers the stateful and failure-sensitive parts of an Envshare
node. The node never persists shared payloads or discovery registrations. Its
only irreplaceable local state is the Ed25519 identity key.

## Routine checks

For every region, continuously verify:

```console
envshare-node healthcheck --url http://127.0.0.1:9100/health/live
envshare-node healthcheck --url http://127.0.0.1:9100/health/ready
curl --fail http://127.0.0.1:9100/metrics
envshare doctor --network public-v1 --verbose
```

The local checks prove process progress and listeners. `doctor` must run from an
independent network and prove TCP, QUIC, relay, and discovery paths. Review the
[Prometheus rules and Grafana dashboard](../deploy/monitoring/) for capacity,
rejection, restart, listener, queue, and regional-probe signals.

## Backup and restore

Back up only:

- `/var/lib/envshare-node/identity.key`;
- `/etc/envshare-node/node.toml`;
- deployment and monitoring configuration; and
- the expected public Peer ID recorded in infrastructure configuration.

Stop or snapshot one node at a time. Copy the identity without changing its
bytes, encrypt it using the organization's approved offline backup system, and
store it separately from host and cloud credentials. Never put an identity in a
container image, repository, ticket, or log. Registration memory and relay state
must not be backed up.

Test a backup in an isolated temporary host or container:

```console
install -o envshare -g envshare -m 0600 restored-identity.key \
  /var/lib/envshare-node/identity.key
sudo -u envshare envshare-node key inspect \
  --identity /var/lib/envshare-node/identity.key
sudo -u envshare envshare-node config check \
  --config /etc/envshare-node/node.toml
```

The inspected Peer ID must exactly match the recorded value before the restored
node is allowed onto the network. A restore test is incomplete until the node is
ready and a remote `doctor` passes both transports, relay, and discovery.

## Rolling upgrade and rollback

Upgrade one failure domain at a time:

1. Confirm at least two other independent nodes are ready and externally usable.
2. Record current image digest or binary checksum, version, Peer ID, config, and
   dashboard baseline.
3. Stop the selected node. `SIGTERM` removes readiness and listeners immediately
   and permits the configured connection drain before exit.
4. Wait until the process exits; do not force-kill unless the systemd/Compose
   stop deadline expires.
5. Install the verified artifact without replacing the identity or configuration.
6. Start it and require liveness, readiness, both public transports, relay, and
   discovery checks to pass.
7. Watch restart, memory, rejection, and saturation metrics for at least one
   normal traffic interval before proceeding to the next region.

Rollback on repeated restarts, readiness failure, protocol regression, or an
unexpected resource step. Restore the previous signed image digest or binary,
leave the identity unchanged, revalidate configuration, and repeat the full
remote verification. Never roll every region simultaneously.

## Identity rotation

A new key means a new Peer ID and new multiaddresses. Rotation is a migration,
not an in-place file replacement:

1. Provision a second node with a newly generated identity.
2. Publish both old and new node records in the client network profile or signed
   manifest.
3. Run both throughout a client adoption window and monitor use.
4. Remove the old record in a later client/profile release.
5. Retain the old encrypted key only through the rollback window, then destroy
   every copy according to the key-destruction policy.

Never silently replace the key behind an existing hostname. If a key may be
compromised, skip the normal overlap assumption: remove its node record, publish
a security notice, deploy a new key on clean infrastructure, and investigate
metadata visibility and traffic manipulation. Compromise of a node identity
does not by itself reveal share codes or decrypt correctly implemented payloads.

## Capacity and overload

Use the configured-capacity metrics rather than host utilization alone:

- `envshare_node_connections` and `_connection_capacity`;
- `envshare_node_reservations` and `_reservation_capacity`;
- `envshare_node_circuits` and `_circuit_capacity`;
- `envshare_node_discovery_registrations` and
  `_discovery_registration_capacity`; and
- admission/protocol rejection and dropped-event counters.

When utilization exceeds 85 percent or rejections rise:

1. Confirm whether the increase is legitimate traffic, a single-region outage,
   abusive sources, or a monitoring artifact.
2. Preserve established circuits; do not restart merely to clear counters.
3. Add independently bounded capacity or shift client profiles away from the
   affected failure domain.
4. Apply short source-network controls at provider and host firewalls when
   evidence supports them. Account for shared NATs and avoid permanent automatic
   bans from one malformed packet.
5. Change application limits only after memory, FD, bandwidth, and load-test
   evidence confirms the new bound is safe. Absolute compiled ceilings still
   apply.

Do not inspect encrypted payload contents or log peer/address identifiers to
classify abuse. Provider DDoS controls, small relay duration/byte/per-peer limits,
regional transfer budgets, and the ability to withdraw an affected node from
client profiles are the preferred controls.

## Alert response

`EnvshareNodeDown`, `EnvshareNodeNotReady`, listener loss, or regional probe
failure is availability-critical. Check process status, recent safe event names,
operations endpoint binding, TCP and UDP listeners, firewall changes, provider
incidents, DNS, and a remote `doctor`. Keep at least two other regions serving
while repairing the node.

For restart loops, stop automatic restart before logs rotate, preserve the exact
artifact/config hashes, run `config check`, inspect cgroup termination reason,
and roll back if the new artifact introduced the loop. Never enable secret-bearing
backtraces or dump process memory in production.

For event drops or rejection spikes, first verify capacity metrics and external
traffic volume. Individual client failures are not alert-worthy. Escalate only
on sustained fleet or regional impact.

## Security incident handling

For suspected host or identity compromise:

1. Remove the node from published network profiles and provider routing.
2. Preserve host/cloud audit evidence without copying process memory or logging
   payloads, codes, namespaces, peer IDs, or source addresses into public tools.
3. Revoke host, monitoring, registry, and deployment credentials.
4. Treat the identity as compromised and follow emergency rotation on clean
   infrastructure.
5. Determine the interval and regions of possible metadata observation or
   traffic manipulation, notify maintainers through `SECURITY.md`, and publish
   appropriate user guidance.
6. Restore service from verified artifacts and configuration; never reuse a
   suspect machine image.

For an accidental telemetry exposure, disable the exporter, preserve access
controls and destination audit logs, determine the exact field schema and time
window, rotate exporter credentials, and delete data according to policy. The
normal node schema intentionally excludes secret and identifier fields; any
deviation is a security bug.
