# Command-line interface

Run `envshare <command> --help` for the complete option reference. Configuration
is resolved before network activity using the precedence documented in
[configuration.md](configuration.md).

## Transfer commands

- `envshare send [FILE]` shares raw dotenv bytes. Without a file, an interactive
  terminal offers dotenv files from the current directory. Use `-` for bounded stdin,
  `--keys A,B` for deterministic normalized selection, and `--expires` for the
  sender-owned lifetime. Human output shows only the capability and transfer
  state by default; `--verbose` adds peer and route diagnostics. `--code-only`
  prints only the bearer capability;
  `--json` emits non-secret lifecycle records and sends the capability to
  stderr instead.
- `envshare receive` asks for the capability and targets `.env` interactively.
  A missing destination is created directly. For an existing file, choose merge,
  add missing keys, save elsewhere, replace, or cancel. Scripts remain
  deterministic with `--output` and `--mode create|merge|append-missing|replace`;
  without a terminal their legacy default is `.env.shared`. `--durable` flushes
  file and directory metadata before acknowledgement.
- `envshare run -- PROGRAM ARGS...` executes the program directly, without a
  shell. Received variables fill absent inherited names by default. `--override`
  replaces matches, `--clean-env` removes the inherited environment, and
  `--strict` rejects every collision. `--json` emits `child_started` and
  `child_exited` records; the child retains its normal stdout and stderr. An
  interrupt is forwarded first, then the contained Unix process group or
  Windows Job Object is terminated after a short grace period so descendants
  cannot escape.

Receivers prompt for the capability only on a terminal. Automation must use
`--code-stdin` or `--code`; command-line arguments may be observable to other
local processes, so stdin is preferred. A successful local write followed by an
unconfirmed acknowledgement is reported as delivery-unknown and must not be
retried to another destination.

## Connectivity

The optional `--network` flag defaults to the embedded, relay-only `public`
profile backed by `node.envshare.xyz`. Explicit direct mode uses `--peer` and
`--address`. Public discovery accepts repeated `--discovery-node` values. Senders
accept repeated `--relay` endpoints; profile `relays` are applied automatically.
`--mdns` enables local discovery, `--lan` admits private candidate routes, and
`--relay-only` excludes direct listeners and candidates as well as mDNS. The
sender does not reveal the capability until any required relay reservation and
a configured public registration are ready.

`envshare doctor` checks local clock and private-output safety, then uses a
random disposable namespace to test the selected profile's rendezvous nodes and
required relay reservations. It also exercises a local TCP/Noise/Yamux route,
relay circuits, platform DNS resolution, and every configured expected Peer ID.
It never derives diagnostics from a user capability. `--json` emits bounded
counts without namespaces or payload data; `--verbose` adds per-node public
identity results.

## Profiles and generated assets

`envshare network list|show|add|remove|use` manages named network profiles with
atomic private config writes. `envshare completions` writes Bash, Zsh, Fish,
PowerShell, or Elvish completions to stdout; target `man` writes the manual page.

Global `--no-color` and the `NO_COLOR` environment variable disable styling.
`--log` accepts a tracing filter, but Envshare's events are intentionally
secret-safe. Exit statuses are stable: `0` success, `2` usage, `10` invalid
code, `11` not found/unauthorized, `12` unavailable, `13` network, `14`
transfer, `15` output, `16` child start, `20` configuration, `70` internal,
and `130` interrupted.
