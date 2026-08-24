# Send and receive files

## Send a file

Pass the file path explicitly:

```sh
envshare send .env.local
```

The sender reads at most 1 MiB, connects to the selected network, and prints a
share code. It remains online until one receiver completes the transfer, the
share expires, or you press `Ctrl+C`.

Run `envshare send` without a path to choose a dotenv-style file from the
current directory. The picker is available only in an interactive terminal.

Read from standard input with `-`:

```sh
printf 'API_URL=https://api.example.com\nTOKEN=replace-me\n' | envshare send -
```

Send only selected keys:

```sh
envshare send .env --keys DATABASE_URL,API_TOKEN
```

Selected-key mode parses and normalizes the dotenv input. Missing keys fail the
command unless `--allow-missing-keys` is supplied.

Change the default 10-minute lifetime with a duration such as `30s`, `5m`, or
`1h`:

```sh
envshare send .env --expires 5m
```

## Receive a new file

```sh
envshare receive
```

The share code is requested through a hidden prompt. The default destination is
`.env` in the current directory.

New files are written through a private temporary file and renamed atomically.
On Unix, the mode is `0600`. Envshare refuses symlinks, directories, and special
files as destinations.

Choose another path with `--output`:

```sh
envshare receive --output .env.received
```

The parent directory must already exist.

## Handle an existing destination

In an interactive terminal, Envshare asks before claiming the share:

| Choice | Result |
| --- | --- |
| Merge | Add new keys and replace matching values |
| Add missing keys | Add new keys and retain existing values |
| Create a new file | Prompt for another destination |
| Replace file | Replace the complete destination atomically |
| Cancel | Exit without claiming the share |

For scripts, specify the behavior:

```sh
envshare receive --output .env --mode create
envshare receive --output .env --mode merge
envshare receive --output .env --mode append-missing
envshare receive --output .env --mode replace
```

Use `create` when a script must never overwrite an existing file. Merge modes
require valid dotenv input and preserve unrelated declarations and comments in
the destination.

Add `--durable` to flush file and parent-directory metadata before acknowledging
the transfer.

## Automation

Avoid putting the share code in a command argument. Read it from standard input:

```sh
printf '%s\n' "$ENVSHARE_CODE" | \
  envshare receive --code-stdin --output .env --mode create
```

Use `--code-only` on the sender when stdout must contain only the secret code.
Use `--json` for newline-delimited lifecycle events. Do not enable shell tracing
around either command.

See the complete [`envshare` reference](../reference/envshare.md).
