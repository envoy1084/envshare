# Node deployment

The initial public launch intentionally runs one node. Do not add more public
nodes until usage and failure data justify federation. Start with 2 vCPU, 2 GiB
RAM, 20 GiB disk, public IPv4, and preferably IPv6. This is one failure domain:
maintenance, host failure, or a network outage temporarily stops new public
transfers. Relay bandwidth and concurrent connections are the main capacity
drivers; the sample limits are conservative starting points, not measured
guarantees.

The checked-in node configuration caps the initial service at 128 reservations,
64 circuits, 512 connections, 32 connections per source IP, 2 MiB per circuit,
and a 1 GiB process-memory admission threshold. Compose adds a 1.5 GiB cgroup
limit, 256-process limit, and 65,536 file-descriptor limit. Raise one limit at a
time only after observing saturation, memory, descriptors, and bandwidth.

## Network and firewall

The node exposes raw libp2p transports directly. It does not need an HTTP reverse
proxy.

| Port | Protocol | Exposure | Purpose |
|---:|---|---|---|
| 22 | TCP | restricted administration networks | SSH |
| 4001 | TCP | public | TCP, Noise, and Yamux |
| 4001 | UDP | public | QUIC v1 |
| 9100 | TCP | loopback only | health and OpenMetrics |

On an Ubuntu or Debian host using UFW:

```console
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow from 192.0.2.0/24 to any port 22 proto tcp
sudo ufw allow 4001/tcp
sudo ufw allow 4001/udp
sudo ufw enable
sudo ufw status verbose
```

Replace the documentation-only SSH source network before applying the rules.
Mirror TCP/4001 and UDP/4001 in the provider firewall. Do not add a public rule
for 9100; scrape it on loopback or through a private monitoring path. Verify TCP
and UDP independently from another network. Docker port publishing can bypass
some host firewall rule paths; the supplied Linux Compose deployment uses host
networking so the documented host rules remain authoritative.

## Container deployment

The [Dockerfile](../deploy/docker/Dockerfile) pins multi-architecture builder and
runtime image indexes, builds with the locked dependency graph, and runs as UID
and GID 10001. The runtime has no package-install step, the identity is never
copied into the image, and Compose supplies a read-only root filesystem, dropped
capabilities, no-new-privileges, bounded memory/PIDs/file descriptors, disabled
core dumps, and a native readiness check.

Tagged releases publish a signed multi-platform node image for `linux/amd64` and
`linux/arm64`, including an SBOM and build provenance:

```console
docker pull ghcr.io/envoy1084/envshare-node:0.1.2
cosign verify ghcr.io/envoy1084/envshare-node:0.1.2 \
  --certificate-identity-regexp \
  'https://github.com/envoy1084/envshare/.github/workflows/container-release.yml@refs/tags/v0.1.2' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

The verified `v0.1.2` multi-platform index digest is
`sha256:6123c0a804908647ccac859b0d665311afa9fec36390700e8e29efcd539b291b`.
Pin `ghcr.io/envoy1084/envshare-node@sha256:6123c0a804908647ccac859b0d665311afa9fec36390700e8e29efcd539b291b`
in the deployment instead of relying on the mutable tag.

From the repository root:

```console
docker compose -f deploy/docker/compose.yaml build
docker compose -f deploy/docker/compose.yaml run --rm --no-deps node \
  key generate --output /var/lib/envshare-node/identity.key
docker compose -f deploy/docker/compose.yaml run --rm --no-deps node \
  key inspect --identity /var/lib/envshare-node/identity.key
docker compose -f deploy/docker/compose.yaml up -d
docker compose -f deploy/docker/compose.yaml ps
docker compose -f deploy/docker/compose.yaml logs --tail=100 node
```

The named volume is the only writable persistent location. Back up its
`identity.key` offline before relying on the node. To include OTLP support in a
locally built image, set `ENVSHARE_NODE_FEATURES=otlp`; export still remains off
until `otlp_endpoint` is configured.

Host networking is supported by Docker Engine on Linux. Where it is unavailable,
remove `network_mode: host`, publish `4001:4001/tcp`, `4001:4001/udp`, and
`127.0.0.1:9100:9100/tcp`, then verify advertised addresses refer to the public
host rather than a container-private address.

Verify the running node:

```console
docker compose -f deploy/docker/compose.yaml exec node \
  envshare-node healthcheck --url http://127.0.0.1:9100/health/ready
curl --fail http://127.0.0.1:9100/metrics
envshare doctor --verbose
```

## Native systemd deployment

Create a dedicated account and protected directories:

```console
cargo build --locked --profile dist --package node
sudo useradd --system --home /var/lib/envshare-node --create-home \
  --shell /usr/sbin/nologin envshare
sudo install -d -o envshare -g envshare -m 0750 /var/lib/envshare-node
sudo install -d -o root -g envshare -m 0750 /etc/envshare-node
sudo install -o root -g root -m 0755 target/dist/envshare-node \
  /usr/local/bin/envshare-node
sudo install -o root -g envshare -m 0640 deploy/config/node.example.toml \
  /etc/envshare-node/node.toml
sudo install -o root -g root -m 0644 deploy/systemd/envshare-node.service \
  /etc/systemd/system/envshare-node.service
```

Generate the stable identity once, validate configuration, and start:

```console
sudo -u envshare envshare-node key generate \
  --output /var/lib/envshare-node/identity.key
sudo -u envshare envshare-node key inspect \
  --identity /var/lib/envshare-node/identity.key
sudo -u envshare envshare-node config check \
  --config /etc/envshare-node/node.toml
sudo systemctl daemon-reload
sudo systemctl enable --now envshare-node
sudo systemctl status envshare-node
journalctl -u envshare-node -n 100 --no-pager
```

The unit allows only Internet/Unix address families, removes capabilities,
protects the host and kernel, denies executable writable memory, disables core
dumps, and limits memory, tasks, and file descriptors. Test the unit on the exact
Linux distribution before rollout; use `systemd-analyze security envshare-node`
to review effective hardening. If a setting conflicts with a platform crypto or
network implementation, narrow that setting explicitly and record why.

Readiness and drain checks:

```console
envshare-node healthcheck --url http://127.0.0.1:9100/health/live
envshare-node healthcheck --url http://127.0.0.1:9100/health/ready
sudo systemctl stop envshare-node
```

On stop, readiness fails immediately, listeners close, existing connections get
up to 30 seconds to drain, and systemd permits 45 seconds before forced cleanup.
Avoid speculative sysctl tuning. Change file-descriptor, socket-buffer, queue,
or conntrack limits only after load measurements and keep the values in managed
infrastructure configuration.
