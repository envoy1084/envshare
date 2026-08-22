# Node load and soak testing

`node-load` is a bounded engineering harness in the non-published `testkit`
package. It starts the real node in-process on loopback and drives real libp2p
client swarms through Noise, TCP, Circuit Relay v2, and signed Rendezvous
messages. It creates no share capability or payload and reports aggregate counts
only.

## Workloads

```console
cargo run --release -p testkit --bin node-load -- \
  relay --clients 64 --capacity 32 --hold 5s --json

cargo run --release -p testkit --bin node-load -- \
  discovery --clients 64 --capacity 32 --hold 5s --json

cargo run --release -p testkit --bin node-load -- \
  soak --clients 64 --capacity 32 --duration 24h --hold 5s --json \
  | tee node-soak.json
```

`relay` holds accepted reservations concurrently and verifies acceptance never
exceeds the configured reservation capacity. `discovery` creates unique opaque
namespaces, holds accepted signed registrations, verifies the global bound, and
unregisters before client shutdown. `soak` repeats both forms of connection and
registration churn against one long-lived node.

Arguments are fail-closed: clients are limited to 256, capacity must be between
one and the client count, hold time is five seconds to one hour, and soak time is
at most seven days. One round may finish after the requested soak deadline by at
most its bounded hold/attempt interval. On Linux the report includes current and
peak process RSS and open-FD counts from `/proc`; other platforms return `null`
for those fields. The node event stream is continuously drained so the harness
does not manufacture event-queue pressure accidentally.

A denied relay reservation does not currently produce a distinct public client
event in rust-libp2p, so a bounded five-second `timed_out` result is the expected
client-side saturation signal. The node's reservation-denied counter is the
authoritative server-side signal. Discovery capacity denial produces an explicit
`rejected` result.

## Recorded smoke result

These results verify harness behavior; they are not a production capacity
claim.

| Date | Environment | Workload | Clients / capacity | Result | Elapsed |
|---|---|---|---:|---|---:|
| 2026-08-22 | macOS 26.5.2, Apple M3 Pro, 18 GiB, arm64, Rust 1.96.0 debug | relay | 64 / 32 | 32 accepted, 32 timed out, 0 failed | 5.088 s |
| 2026-08-22 | same | discovery | 64 / 32 | 32 accepted, 32 rejected, 0 failed | 5.081 s |
| 2026-08-22 | same | churn smoke | 16 / 8 | 2 relay rounds and 1 discovery round, 0 failed | 15.080 s |
| 2026-08-22 | Linux 7.0.14 OrbStack arm64 container, 2 CPU / 1536 MiB cgroup, Rust 1.96.0 release | relay | 64 / 32 | 32 accepted, 32 timed out, 0 failed; RSS 4.10 / 20.29 / 20.23 MB; FDs 10 / 12 / 10 | 5.039 s |
| 2026-08-22 | same | discovery | 64 / 32 | 32 accepted, 32 rejected, 0 failed; RSS 4.09 / 21.02 / 21.02 MB; FDs 10 / 24 / 10 | 5.044 s |

Resource triples are start / peak / end. macOS does not expose the Linux `/proc`
samples, so those RSS/FD fields were `null`. The Linux container results prove
the release harness and cleanup path under cgroup constraints, but the host is
still a development laptop. No multi-day, VPS, or public-network capacity result
is claimed in this table.

## Linux qualification procedure

Run qualification on the exact release binary, kernel, VM size, container or
systemd limits, and network path intended for production. For the initial 2-vCPU,
2-GiB node shape:

1. Build with `--locked --profile dist`; record commit, binary checksum, config,
   kernel, image digest, CPU, RAM, and provider bandwidth/PPS limits.
2. Keep the service cgroup at 1536 MiB, core dumps disabled, and FD limit 65536.
3. Observe a 30-minute idle baseline for RSS, FDs, tasks, readiness, and event
   drops.
4. Run relay and discovery cases at 32/16, 64/32, 128/64, and 256/128. Preserve
   the JSON reports and server metrics for every step.
5. Run the 24-hour soak command above. Sample `/metrics`, cgroup memory, CPU,
   FDs, network bytes/packets, restarts, and kernel OOM events every 30 seconds.
6. After the soak, allow a 30-minute cooldown and compare RSS/FD state with the
   baseline.
7. In a separate three-node staging fleet, repeat normal send/receive and
   `doctor` checks while draining one node, then two nodes. Verify remaining-node
   operations and restore the same Peer IDs from encrypted identity backups.

Qualification passes only when:

- the node never crashes, restarts, OOMs, or exceeds its cgroup/FD/task limits;
- readiness stays valid except during deliberate drain;
- each round attempts exactly the requested clients and accepts no more than the
  configured capacity;
- overload is expressed through bounded rejection/timeout metrics, not stalled
  tasks or unbounded queues;
- event drops remain zero under the intended operating point;
- open FDs return to within five of baseline after cooldown; and
- RSS reaches a stable plateau under repeated rounds and shows no sustained
  positive trend after workload and allocator warm-up.

Do not publish a numeric production capacity until this procedure passes on at
least two runs and the lower repeatable result is recorded here. Scale out across
independent nodes before raising compiled or configured bounds.
