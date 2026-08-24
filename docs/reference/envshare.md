# `envshare` reference

```text
envshare [GLOBAL OPTIONS] <COMMAND>
```

Commands: `send`, `receive`, `run`, `doctor`, `network`, and `completions`.

## Global options

| Option | Description |
| --- | --- |
| `--config PATH` | Override the platform configuration path |
| `--no-color` | Disable terminal styling; also implied by `NO_COLOR` |
| `--log FILTER` | Set the secret-safe tracing filter |
| `-V`, `--version` | Print the version |
| `-h`, `--help` | Print help |

## `send`

```text
envshare send [OPTIONS] [INPUT]
```

`INPUT` is a file path or `-` for standard input. When omitted in an
interactive terminal, Envshare opens a file picker. Input is limited to 1 MiB.

| Option | Description |
| --- | --- |
| `--expires DURATION` | Share lifetime; built-in default is `10m` |
| `--keys KEY,...` | Parse dotenv input and include only these keys |
| `--allow-missing-keys` | Skip requested keys that are absent; requires `--keys` |
| `--network NAME` | Use a configured network profile for this command |
| `--listen MULTIADDR` | Direct listener; default `/ip4/127.0.0.1/tcp/0` |
| `--discovery-node MULTIADDR` | Add a Rendezvous endpoint; repeatable |
| `--relay MULTIADDR` | Add a Circuit Relay endpoint; repeatable |
| `--mdns` | Enable multicast DNS discovery |
| `--relay-only` | Advertise and accept only relay routes |
| `--code-only` | Write only the share code to stdout |
| `--json` | Emit non-secret newline-delimited lifecycle events |
| `--verbose` | Include the sender Peer ID and route in human output |

`--code-only` conflicts with `--json`. `--verbose` conflicts with both.

## `receive`

```text
envshare receive [OPTIONS]
```

| Option | Description |
| --- | --- |
| `--code CODE` | Supply the capability as an argument; avoid when possible |
| `--code-stdin` | Read the capability from standard input |
| `-o`, `--output PATH` | Destination; interactive default is `.env` |
| `--mode create` | Refuse an existing destination |
| `--mode replace` | Atomically replace the complete regular file |
| `--mode merge` | Add new keys and update matching keys |
| `--mode append-missing` | Add missing keys without changing existing values |
| `--durable` | Flush file and directory metadata before acknowledgement |
| `--peer PEER_ID` | Explicit sender identity; requires `--address` |
| `--address MULTIADDR` | Explicit sender address; requires `--peer` |
| `--discovery-node MULTIADDR` | Add a Rendezvous endpoint; repeatable |
| `--mdns` | Enable multicast DNS candidate discovery |
| `--lan` | Admit private and link-local candidate addresses |
| `--relay-only` | Dial only relay routes and disable mDNS |
| `--network NAME` | Use a configured network profile |
| `--json` | Emit non-secret newline-delimited events |

Non-interactive use must supply the code, output path, and an existing-file
mode where required.

## `run`

```text
envshare run [OPTIONS] -- <PROGRAM> [ARGUMENTS...]
```

Connection and code-input options are the same as `receive`.

| Option | Description |
| --- | --- |
| `--override` | Received values replace matching inherited values |
| `--clean-env` | Clear inherited values before applying received values |
| `--strict` | Fail if any received key exists in the inherited environment |
| `--json` | Emit non-secret lifecycle events |

The program is executed directly without a shell. Envshare returns its exit
status after a successful transfer.

## `doctor`

```text
envshare doctor [--network NAME] [--json] [--verbose]
```

Checks the local clock, private file output, DNS, discovery, and relay
connectivity. `--verbose` adds per-node public identity results.

## `network`

```text
envshare network list
envshare network show NAME
envshare network add NAME --file PROFILE.toml
envshare network remove NAME
envshare network use NAME
```

`add` replaces a profile with the same local name. The active default profile
cannot be removed. Configuration writes are private and atomic.

Profile schema:

```toml
network_id = "team-v1"
require_relay = true
relay_only = true
rendezvous = ["/dns4/node.example.com/udp/4001/quic-v1/p2p/PEER_ID"]
relays = ["/dns4/node.example.com/tcp/4001/p2p/PEER_ID"]
```

Profiles accept at most eight Rendezvous endpoints and eight relays. Names and
network IDs contain 1–64 ASCII letters, digits, `.`, `_`, or `-`. Endpoints must
end in `/p2p/<peer-id>`, with no repeated Peer ID in one list.

## `completions`

```text
envshare completions <TARGET>
```

Targets: `bash`, `zsh`, `fish`, `power-shell`, `elvish`, and `man`. Generated
content is written to stdout.

## Client configuration

Envshare works without a configuration file. Default paths:

| Platform | Path |
| --- | --- |
| macOS | `~/Library/Application Support/envshare/config.toml` |
| Linux and other Unix | `$XDG_CONFIG_HOME/envshare/config.toml` or `~/.config/envshare/config.toml` |
| Windows | `%APPDATA%\envshare\config.toml` |

`--config` takes precedence over `ENVSHARE_CONFIG` and the platform path.

```toml
version = 1
default_network = "public"

[defaults]
share_ttl = "10m"
relay_only = false
mdns = false
```

Environment overrides:

| Variable | Effect |
| --- | --- |
| `ENVSHARE_CONFIG` | Select the configuration file |
| `ENVSHARE_NETWORK` | Override `default_network` |
| `ENVSHARE_SHARE_TTL` | Override the sender lifetime |
| `ENVSHARE_MDNS` | Set the mDNS default |
| `ENVSHARE_RELAY_ONLY` | Set the relay-only default |
| `NO_COLOR` | Disable terminal styling |

Boolean values are lowercase `1`, `true`, `yes`, `0`, `false`, or `no`.
Precedence is command options, environment, configuration file, then built-in
defaults.

## Exit codes

| Code | Meaning |
| ---: | --- |
| `0` | Success |
| `2` | Invalid command or arguments |
| `10` | Invalid share code |
| `11` | Share not found or unauthorized |
| `12` | Share expired, consumed, or unavailable |
| `13` | Network failure |
| `14` | Secure transfer or acknowledgement failure |
| `15` | Private output or merge failure |
| `16` | Child process could not start or be managed |
| `20` | Invalid configuration, profile, duration, or log filter |
| `70` | Internal failure |
| `130` | Interrupted |

`envshare run` returns the child status after a successful transfer, so it can
overlap an Envshare-defined code. Use JSON events when automation must
distinguish those cases.
