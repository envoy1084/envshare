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
operations_address = "127.0.0.1:9100"
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

[telemetry]
log_format = "json"
log_filter = "info,libp2p=warn"
otlp_endpoint = ""
otlp_sample_ratio = 0.01
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

- `GET /health/live` (alias `/healthz`): `200` while the node task has advanced
  within five seconds;
- `GET /health/ready` (alias `/readyz`): `200` only after every configured
  libp2p listener is ready and before drain;
- `GET /metrics`: `OpenMetrics` 1.0 counters and gauges with no peer, address,
  namespace, capability, or payload labels.

Requests are capped at 8 KiB, I/O has a two-second deadline, and at most 32
connections are handled concurrently. On an interrupt, readiness drops and
libp2p listeners close immediately. Existing peer connections continue until
they disconnect or `shutdown_grace_period` expires; the operations endpoint
stays live during that interval and then shuts down without detached request
tasks.

Container and service managers can check readiness without adding an HTTP tool
to the runtime image:

```console
envshare-node healthcheck --url http://127.0.0.1:9100/health/ready
```

The command accepts plain HTTP, loopback socket addresses, and the canonical
liveness/readiness paths only. It caps the response at 4 KiB and the entire
check at three seconds. Both Ctrl-C and `SIGTERM` trigger graceful drain.

## Logs and optional tracing

`log_format` accepts `text` or newline-delimited `json`; `--json` overrides it
for both local logs and the operational event stream. Structured events contain
stable event names but never peer IDs, multiaddresses, discovery namespaces,
capabilities, error chains, or arbitrary request data. The public node Peer ID
is printed only in interactive text mode and can always be obtained separately
with `envshare-node key inspect`.

OTLP tracing is compiled out by default. Build the node with
`cargo build -p node --release --features otlp` and set `otlp_endpoint` to an
explicit HTTP(S) OpenTelemetry Collector base URL to opt in. An absent or empty
endpoint disables export. URL credentials, query strings, and fragments are
rejected so credentials cannot be embedded accidentally; configure collector
authentication outside this file. The configured destination is the only trace
destination.

Trace IDs come from the SDK's random ID generator and are unrelated to discovery
namespaces. Spans carry no peer IDs or addresses. The sample ratio defaults to
one percent and cannot exceed ten percent; queues, batches, span attributes,
events, links, and export time are bounded. Export occurs on an independent
batch worker, and collector failures do not change health or readiness. A binary
built without `otlp` rejects a non-empty endpoint instead of silently ignoring
it.
