# Envshare documentation

Envshare transfers one dotenv file from a sender to one receiver. The sender
stays online until the transfer succeeds, expires, or is cancelled. The public
node provides discovery and relay connectivity; it does not store the payload.

## Guides

- [Install Envshare](guides/installation.md)
- [Complete the first transfer](guides/getting-started.md)
- [Send and receive files](guides/send-and-receive.md)
- [Run a command with received values](guides/run-a-command.md)
- [Use public, private, and direct networks](guides/networks.md)
- [Understand the security boundary](guides/security.md)
- [Troubleshoot failures](guides/troubleshooting.md)
- [Run an Envshare node](guides/self-hosting.md)

## Reference

- [`envshare` command reference](reference/envshare.md)
- [`envshare-node` command reference](reference/envshare-node.md)

Protocol details for implementers are maintained separately:

- [Protocol specification](../protocol/protocol.md)
- [Share-code format](../protocol/code-format.md)
- [Wire schema](../protocol/messages.cddl)
