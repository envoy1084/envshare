# Node configuration

Start the service with a stable identity and a strict TOML file:

```console
envshare-node serve \
  --identity /var/lib/envshare-node/identity.key \
  --config /etc/envshare-node/node.toml \
  --json
```

Every field is optional and inherits the compiled safety default. Unknown
fields, invalid durations or multiaddresses, non-regular files, files larger
than 256 KiB, and values outside absolute ceilings fail startup before sockets
are bound.

```toml
listen_addresses = [
  "/ip4/0.0.0.0/tcp/4001",
  "/ip4/0.0.0.0/udp/4001/quic-v1",
]

max_reservations = 128
max_reservations_per_peer = 2
reservation_duration = "1h"
max_circuits = 64
max_circuits_per_peer = 4
max_circuit_duration = "2m"
max_circuit_bytes = 2097152

max_connections = 512
max_connections_per_peer = 8
max_connections_per_ip = 32
connection_attempts_per_ip_per_minute = 120
connection_rate_limit_ips = 4096
max_process_memory_bytes = 1073741824
event_capacity = 256
operations_address = "127.0.0.1:9090"
shutdown_grace_period = "30s"

discovery_min_ttl_seconds = 30
discovery_max_ttl_seconds = 300
discovery_registrations_per_peer = 8
discovery_registrations_total = 256
discovery_registrations_per_namespace = 32
discovery_cookies = 512
discovery_addresses_per_registration = 8
discovery_record_bytes = 16384
discovery_results = 32
discovery_register_requests_per_minute = 12
discovery_discover_requests_per_minute = 30
discovery_rate_limit_peers = 1024
discovery_allow_private_addresses = false
```

Absolute ceilings cap listeners, connections, reservations, circuits, circuit
duration and bytes, memory thresholds, event queues, registration storage,
response size, and request-rate state. They cannot be raised through TOML or
command-line overrides. Private, loopback, and link-local discovery records are
rejected unless `discovery_allow_private_addresses` is explicitly enabled for a
private deployment.

Source-IP admission runs before Noise negotiation. It bounds simultaneous
connections, attempts per fixed one-minute window, and the number of retained IP
buckets. Authenticated peers are independently bounded by transport connections,
relay reservations/circuits, discovery storage, and discovery request rates.
When any bound or rate-map capacity is exhausted, the node rejects new work
without allocating unbounded state.

## Health, readiness, metrics, and drain

The operations listener accepts loopback addresses only. It serves:

- `GET /healthz`: `200` while the node task is alive;
- `GET /readyz`: `200` only after a libp2p listener is ready and before drain;
- `GET /metrics`: `OpenMetrics` 1.0 counters and gauges with no peer, address,
  namespace, capability, or payload labels.

Requests are capped at 8 KiB, I/O has a two-second deadline, and at most 32
connections are handled concurrently. On an interrupt, readiness drops and
libp2p listeners close immediately. Existing peer connections continue until
they disconnect or `shutdown_grace_period` expires; the operations endpoint
stays live during that interval and then shuts down without detached request
tasks.
