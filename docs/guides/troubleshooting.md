# Troubleshooting

Start with the built-in diagnostics:

```sh
envshare doctor --verbose
```

Use `envshare --log debug ...` only while diagnosing a problem. Envshare keeps
its tracing fields secret-safe, but surrounding tools may still capture command
arguments or terminal output.

## `private output failed`

Envshare could not safely create or modify the destination. Check that:

- the parent directory exists and is writable;
- the destination is a regular file, not a symlink or directory;
- the file is not locked by another program;
- an existing file has an explicit `--mode` in a non-interactive terminal; and
- the filesystem supports creating a temporary file and atomically renaming it.

Try a new destination:

```sh
envshare receive --output .env.received --mode create
```

Envshare writes relative paths in the current terminal directory, not the home
directory.

## `share not found or unauthorized`

The code may be mistyped, the sender may not be registered yet, or capability
authentication may have failed. Confirm that:

- the sender is still running;
- both peers use the same network profile;
- the complete `esh1-...` code was copied; and
- the sender printed the code after connecting to its discovery and relay node.

The error deliberately does not reveal whether a room exists.

## `share is unavailable`

The share expired, was consumed, or was cancelled. Start a new sender. A share
code cannot be reused after successful receipt.

## `network operation failed`

Run:

```sh
envshare doctor --verbose
envshare network show public
```

Check DNS for `node.envshare.xyz` and outbound TCP/UDP access to port `4001`.
Corporate firewalls may block QUIC; TCP relay connectivity must still be
available.

## Transfer or acknowledgement failure

If the receiver reports that the file was written but acknowledgement failed,
inspect that destination before retrying. The sender may not know the write
succeeded.

## Automation fails but the terminal works

Interactive prompts are disabled when stdin or stderr is not a terminal.
Provide every required value explicitly:

```sh
printf '%s\n' "$ENVSHARE_CODE" | \
  envshare receive --code-stdin --output .env --mode create
```

Use the stable [exit codes](../reference/envshare.md#exit-codes) rather than
parsing error text.
