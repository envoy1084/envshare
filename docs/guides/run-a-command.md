# Run a command with received values

`envshare run` receives dotenv values and starts one child process without
writing a file:

```sh
envshare run -- npm run dev
```

Arguments after `--` are passed directly to the program. No shell is invoked.

## Environment precedence

By default, inherited environment variables win. Received values are added only
when the key is absent locally.

Use received values for matching keys:

```sh
envshare run --override -- npm run dev
```

Clear the inherited environment first:

```sh
envshare run --clean-env -- /absolute/path/to/server
```

Use an absolute executable path if clearing `PATH` would make the program
unreachable.

Reject any collision between received and inherited values:

```sh
envshare run --strict -- cargo run
```

`--override`, `--clean-env`, and `--strict` are mutually exclusive where their
behavior conflicts.

## Process behavior

- Envshare returns the child process exit status.
- On Unix, signal termination maps to `128 + signal`.
- Interrupts are forwarded to the contained child process group.
- The sender is acknowledged after the child starts, not when it exits.

The child can print, persist, or transmit every value it receives. Envshare
cannot revoke a value after the process starts.
