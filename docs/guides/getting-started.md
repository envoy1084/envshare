# First transfer

This workflow uses the built-in public network. Both terminals need internet
access, and the sender must remain open until the receiver finishes.

## 1. Choose the file

Identify the dotenv file to send, such as `.env.production`. You can provide its
path directly or run `envshare send` without a path to select a dotenv file from
the current directory.

## 2. Start the sender

```sh
envshare send .env.production
```

Envshare connects to the public node and prints an `esh1-...` share code. Send
the complete code to the receiver.
Leave the sender running.

## 3. Receive the file

In another terminal, move to the directory where the file should be created:

```sh
cd /path/to/project
envshare receive
```

Enter the code at the hidden prompt. If `.env` does not exist, Envshare creates
it. Verify the result:

```sh
cat .env
```

The sender exits after the receiver writes the file and acknowledges the
transfer.

## Use one computer

Use two terminal windows and follow the same steps. This still exercises the
public discovery and relay path.

Read [Send and receive](send-and-receive.md) before overwriting or merging an
existing file.
