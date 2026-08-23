# syntax=docker/dockerfile:1.7@sha256:a57df69d0ea827fb7266491f2813635de6f17269be881f696fbfdf2d83dda33e

FROM rust:1.96.0-bookworm@sha256:5e2214abe154fe26e39f64488952e5c991eeed1d6d6da7cc8381ae83927f0cfc AS builder

WORKDIR /src
COPY . .

ARG NODE_FEATURES=""
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/src/target,sharing=locked \
    if [ -n "${NODE_FEATURES}" ]; then \
      cargo build --locked --profile dist --package node --features "${NODE_FEATURES}"; \
    else \
      cargo build --locked --profile dist --package node; \
    fi \
    && cp /src/target/dist/envshare-node /tmp/envshare-node

FROM debian:bookworm-slim@sha256:abd67ffcfa541b485a3dff59865ab629aa048a6c613e639d36e7456b0b229241 AS runtime

ARG VERSION="0.1.2"
LABEL org.opencontainers.image.title="Envshare node" \
      org.opencontainers.image.description="Bounded libp2p discovery and relay node" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.source="https://github.com/envoy1084/envshare" \
      org.opencontainers.image.version="${VERSION}"

COPY --from=builder /tmp/envshare-node /usr/local/bin/envshare-node
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

RUN install -d -o 10001 -g 10001 -m 0750 /var/lib/envshare-node \
    && install -d -o 10001 -g 10001 -m 0750 /etc/envshare-node

USER 10001:10001
WORKDIR /var/lib/envshare-node

EXPOSE 4001/tcp 4001/udp 9100/tcp

ENTRYPOINT ["/usr/local/bin/envshare-node"]
CMD ["serve", "--identity", "/var/lib/envshare-node/identity.key", "--config", "/etc/envshare-node/node.toml"]
