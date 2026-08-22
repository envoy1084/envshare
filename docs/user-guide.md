# User guide

Envshare transfers a bounded dotenv payload from one online sender to one
receiver. The sender owns the lifetime and remains online until the first valid
receiver irreversibly claims the share. Public nodes help peers discover and
relay each other, but they do not receive the capability or plaintext payload.

This initial 0.x release has not completed independent security review and must
not yet be used for production secrets. Read
the [threat model](threat-model.md) before evaluating it with test credentials.

## Before a transfer

1. Install the same Envshare version on both supported systems.
2. Choose either explicit direct mode or at least one trusted-by-configuration
   public discovery node. A public node is not trusted with plaintext, but its
   stable Peer ID and endpoint still need an authentic distribution channel.
3. Confirm the input contains only the intended keys and no comments or values
   that should remain local. The capability is a bearer secret; share it through
   a separate authenticated channel.
4. Keep sender and receiver clocks synchronized and start with the default short
   expiration.

## Send and receive through discovery

On the sender:

```console
envshare send .env \
  --discovery-node /dns4/node.example/udp/4001/quic-v1/p2p/12D3KooW... \
  --relay /dns4/node.example/udp/4001/quic-v1/p2p/12D3KooW...
```

Envshare prints the capability only after its listener, required relay
reservation, and at least one configured registration are ready. Send that code
to the receiver through an authenticated channel. Do not put it in issue trackers,
chat rooms with broad history, screenshots, shell history, or CI logs.

On the receiver, enter the code at the hidden prompt:

```console
envshare receive \
  --discovery-node /dns4/node.example/udp/4001/quic-v1/p2p/12D3KooW... \
  --output .env.shared
```

The destination is created privately and atomically. Existing files are refused
unless `--force` is intentional. Use `--durable` when the acknowledgement must
wait for file and parent-directory metadata to reach durable storage.

Named profiles avoid repeatedly typing node endpoints. Configure them as shown
in [client configuration](configuration.md), then select one with
`envshare network use NAME` or `--network NAME`.

## Explicit direct mode

For a known reachable route, send with a listener and give the receiver the
printed Peer ID, address, and capability through the authenticated channel:

```console
envshare send .env --listen /ip4/127.0.0.1/tcp/0
envshare receive --peer PEER_ID --address MULTIADDR --output .env.shared
```

Loopback is useful for local testing. A real remote direct transfer needs a
routable address and firewall/NAT path. TCP fallback is part of the same native
transport stack; `--relay-only` deliberately excludes direct candidates and mDNS.

## Select keys or run a command

Send only selected normalized dotenv keys:

```console
envshare send .env --keys DATABASE_URL,API_TOKEN
```

Missing selected keys are rejected unless `--allow-missing-keys` is explicit.
Human output explicitly reports `Payload format: normalized selected keys` before
waiting for a receiver. JSON mode reports the non-secret
`"payload_format":"dotenv_normalized"` field on its ready event; code-only mode
continues to print exactly the capability and nothing else.
To avoid persisting the received payload, execute a child directly:

```console
envshare run -- cargo run
```

No shell parses the child arguments. Received values fill absent inherited
variables by default. Use exactly one intended collision policy: `--override`,
`--clean-env`, or `--strict`. Child processes can read and copy their environment;
Envshare cannot revoke a value after delivery.

## Automation

Prefer a pipe over a command-line capability argument:

```console
printf '%s\n' "$ENVSHARE_CODE" | envshare receive --code-stdin --output .env.shared
```

`--code-only` is the explicit secret-bearing sender mode. `--json` emits bounded,
non-secret lifecycle records, but it does not make a terminal, workflow runner, or
child process trusted. Never enable shell command tracing around a capability.
Stable process statuses and output behavior are listed in the [CLI reference](cli.md).

## Completion and uncertainty

One successful authenticated claim permanently moves the sender out of the
available state, even if the receiver disconnects before its final acknowledgement.
If a receiver reports delivery-unknown after writing locally, inspect that chosen
destination; do not retry the same capability to a second destination. An expired,
claimed, malformed, wrong-network, or unauthorized capability intentionally yields
little diagnostic detail.
