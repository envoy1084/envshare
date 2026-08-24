# `envshare-node` reference

`envshare-node` is an untrusted discovery and Circuit Relay service. It stores
its stable identity on disk but does not store Envshare payloads or share codes.

```text
envshare-node <COMMAND>
```

Commands: `key`, `config`, `serve`, and `healthcheck`.

## `key generate`

```text
envshare-node key generate --output PATH [--force]
```

Generates an Ed25519 identity with operating-system randomness, writes it as a
private regular file, and prints the public libp2p Peer ID. `--force` atomically
replaces an existing regular file; symlinks and special files are refused.

## `key inspect`

```text
envshare-node key inspect --identity PATH
```

Decodes an identity file and prints only its public Peer ID.

The identity must persist across restarts. Replacing it changes the node Peer ID
and invalidates client multiaddresses that refer to the old identity.

## `config check`

```text
envshare-node config check --config PATH
```

Validates a regular UTF-8 TOML file no larger than 256 KiB. It checks unknown
fields, multiaddresses, durations, relationships between limits, and absolute
safety ceilings without loading an identity or binding sockets.

## `serve`

```text
envshare-node serve --identity PATH [--config PATH] [--listen MULTIADDR]... [--json]
```

| Option | Description |
| --- | --- |
| `--identity PATH` | Required stable Ed25519 identity |
| `--config PATH` | Strict node configuration; built-in defaults if omitted |
| `--listen MULTIADDR` | Replace configured listeners; repeatable |
| `--json` | Emit newline-delimited operational events and JSON logs |

`SIGTERM` and `Ctrl+C` begin graceful shutdown. Readiness becomes false while
liveness remains true, and active work drains for up to
`shutdown_grace_period`.

## `healthcheck`

```text
envshare-node healthcheck [--url URL]
```

The default URL is `http://127.0.0.1:9100/health/ready`. Only loopback HTTP URLs
ending in `/health/live` or `/health/ready` are accepted. The request has a
three-second bound, and only HTTP `200` succeeds.

## Configuration

The maintained complete example is
[`deploy/config/node.example.toml`](../../deploy/config/node.example.toml).
Missing fields use the defaults below; unknown fields are rejected.

### Listeners and process limits

| Field | Default | Accepted value |
| --- | ---: | --- |
| `listen_addresses` | TCP and QUIC on `0.0.0.0:4001` | 1–16 multiaddresses |
| `operations_address` | `127.0.0.1:9100` | Loopback socket address or omitted |
| `shutdown_grace_period` | `30s` | `0s`–`5m` |
| `event_capacity` | `256` | 1–8192 |
| `max_process_memory_bytes` | `1073741824` | 64 MiB–64 GiB |

### Relay limits

| Field | Default | Hard bound |
| --- | ---: | ---: |
| `max_reservations` | `128` | 1–4096 |
| `max_reservations_per_peer` | `2` | 1–64 and not above total |
| `reservation_duration` | `1h` | Greater than zero, at most 24h |
| `max_circuits` | `64` | 1–4096 |
| `max_circuits_per_peer` | `4` | 1–64 and not above total |
| `max_circuit_duration` | `2m` | Greater than zero, at most 1h |
| `max_circuit_bytes` | `2097152` | 1 KiB–1 GiB |

A circuit closes when either its duration or byte limit is reached.

### Connection admission

| Field | Default | Hard bound |
| --- | ---: | ---: |
| `max_connections` | `512` | 1–8192 |
| `max_connections_per_peer` | `8` | 1–128 and not above total |
| `max_connections_per_ip` | `32` | 1–256 |
| `connection_attempts_per_ip_per_minute` | `120` | 1–1200 |
| `connection_rate_limit_ips` | `4096` | 1–16384 |

### Discovery limits

| Field | Default | Hard bound |
| --- | ---: | ---: |
| `discovery_min_ttl_seconds` | `30` | Greater than zero |
| `discovery_max_ttl_seconds` | `300` | At least min, at most 86400 |
| `discovery_registrations_per_peer` | `8` | 1–total |
| `discovery_registrations_total` | `256` | 1–4096 |
| `discovery_registrations_per_namespace` | `32` | 1–total |
| `discovery_cookies` | `512` | 1–8192 |
| `discovery_addresses_per_registration` | `8` | 1–16 |
| `discovery_record_bytes` | `16384` | 512 bytes–16 KiB |
| `discovery_results` | `32` | 1–64 and not above total |
| `discovery_register_requests_per_minute` | `12` | 1–120 |
| `discovery_discover_requests_per_minute` | `30` | 1–240 |
| `discovery_rate_limit_peers` | `1024` | 1–4096 |
| `discovery_allow_private_addresses` | `false` | Boolean |

`discovery_results × discovery_record_bytes` cannot exceed 900 KiB. Public
nodes should not admit private or link-local registrations.

### Telemetry

```toml
[telemetry]
log_format = "json"
log_filter = "info,libp2p=warn"
otlp_endpoint = ""
otlp_sample_ratio = 0.01
```

| Field | Default | Validation |
| --- | --- | --- |
| `log_format` | `text` | `text` or `json` |
| `log_filter` | `info,libp2p=warn` | Valid filter, 1–256 characters |
| `otlp_endpoint` | Disabled | HTTP(S) base URL without credentials, query, or fragment |
| `otlp_sample_ratio` | `0.01` | Finite value from 0.0 through 0.1 |

OTLP export requires a binary built with the `otlp` feature. An empty endpoint
disables export.

## Operations endpoints

When `operations_address` is configured, the node exposes:

| Path | Purpose |
| --- | --- |
| `/health/live` | Process liveness |
| `/health/ready` | Acceptance of new work |
| `/metrics` | OpenMetrics metrics |

Keep this server on loopback and expose metrics through a controlled monitoring
path if needed.
