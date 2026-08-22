# Node identity operations

`envshare-node` uses a stable Ed25519 identity so clients can pin the Peer ID in
relay and discovery multiaddresses. The protobuf key file is written atomically
with private permissions and is never printed.

```console
envshare-node key generate --output /var/lib/envshare-node/identity.key
envshare-node key inspect --identity /var/lib/envshare-node/identity.key
envshare-node serve \
  --identity /var/lib/envshare-node/identity.key \
  --listen /ip4/0.0.0.0/tcp/4001 \
  --listen /ip4/0.0.0.0/udp/4001/quic-v1
```

Back up the identity file as secret key material. Restore it only to a trusted
replacement node and keep its file permissions private.

## Rotation

Rotation changes the Peer ID and therefore every configured relay/discovery
multiaddress. Generate a new identity on a separate node, publish both old and
new addresses, start the new node, verify readiness and reservations, then
remove the old address after the longest client configuration rollout window.
Finally drain and securely retire the old key. Never use `--force` as an
in-place rotation mechanism without first distributing the new Peer ID.
