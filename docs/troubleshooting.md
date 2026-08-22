# Troubleshooting

Start with `envshare doctor` for local safety, clock, DNS, configured Peer IDs,
discovery, relay reservations, and disposable network connectivity. It never uses
a real capability. Add `--json` for bounded automation output; use `--verbose` only
when disclosing configured public node identities is acceptable.

Never paste a capability, room identifier, `.env` contents, environment names or
values, sender path, raw trace, or crash dump into an issue. Envshare deliberately
collapses several security-sensitive failures into the same public error.

## Share is not found or is unavailable

- Confirm both sides use the same network profile or `--network` value and the
  same authenticated discovery endpoints.
- Keep the sender running. Discovery is not an offline mailbox, and registration
  expires with the sender-owned lifetime.
- Make sure the sender printed the capability after registration became ready.
  If it exited earlier, fix the node endpoint, expected Peer ID, firewall, or
  required relay reservation first.
- Check clock synchronization and expiration. Never extend a capability lifetime
  merely to conceal a clock or connectivity failure.
- A share may already be claimed. Exactly one valid receiver can win, and a
  disclosure state never returns to available.
- Invalid, wrong-network, unknown, expired, and unauthorized look deliberately
  similar. Re-enter the code through the hidden prompt instead of logging it.

## Peers cannot connect

- Explicit mode requires both `--peer` and `--address`; verify that the address
  ends at the sender route and is reachable from the receiver.
- Private, loopback, and link-local candidates are filtered by default. Use
  `--lan` only when the receiver intentionally trusts and can route the local
  network. `--mdns` must be enabled on the participating LAN peers.
- `--relay-only` disables mDNS and direct routes. Configure a matching relay on
  the sender and ensure its reservation succeeds before sharing the code.
- QUIC needs UDP and TCP fallback needs TCP through host and cloud firewalls.
  A DNS name must resolve to addresses that the advertised multiaddress can use.
- Run `envshare doctor` with the exact profile. A Peer ID mismatch is an endpoint
  authenticity failure, not a warning to bypass.

## Output file fails

- The default `.env.shared` may already exist. Choose another path or inspect it
  before using `--force`.
- The parent must be a writable directory. Symlinks, directories, and unsupported
  special files are refused at the destination boundary.
- On Windows, confirm the current user can create a private file and apply the
  expected ACL in that directory. On Unix, check owner permissions and umask.
- A delivery-unknown result after a successful local write means the sender did
  not confirm the acknowledgement. Inspect the selected file and do not retry to
  another destination.

## Child command fails

- Put the executable and its arguments after `--` when an argument resembles an
  Envshare option.
- No shell expansion occurs. Invoke a shell explicitly only if its additional
  parsing and secret exposure are intended.
- Choose the environment collision behavior explicitly. `--strict` rejects a
  collision; `--override` replaces it; `--clean-env` removes inherited values.
- Exit status `16` means the child could not start. Once started, the child's own
  status is preserved; an interrupt uses status `130` after contained cleanup.

## Node is not ready

Run configuration validation before starting the service:

```console
envshare-node config check --config /etc/envshare/node.toml
envshare-node healthcheck --url http://127.0.0.1:9100/health/live
envshare-node healthcheck --url http://127.0.0.1:9100/health/ready
```

Liveness means the event loop heartbeat is current. Readiness additionally
requires every configured listener. Inspect secret-safe JSON logs, bounded
metrics, resource caps, port conflicts, identity-file permissions, and firewall
rules using the [operations runbook](operations.md). OTLP exporter failure is not
a readiness dependency.

## Installation fails

- The Unix installer supports glibc Linux and macOS only; musl is rejected rather
  than receiving an incompatible archive. Windows releases support x86-64.
- The install directory must be absolute and writable. Existing binaries require
  `--force` or `-Force`.
- A checksum failure must stop before replacement. Retry from a trustworthy
  network, compare the release digest, and verify the GitHub attestation. Do not
  disable verification.
- If the chosen directory is not on `PATH`, invoke the full installed path or add
  the directory yourself; installers do not edit shell profiles or system PATH.

The [CLI reference](cli.md) maps error classes to stable exit statuses. Report
reproducible security issues privately as described in the [security policy](../SECURITY.md).

