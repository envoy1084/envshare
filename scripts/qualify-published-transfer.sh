#!/bin/sh
# SPDX-License-Identifier: Apache-2.0

set -eu

tag=${1:-}
mode=${2:-direct}

case "$tag" in
    v[0-9]*.[0-9]*.[0-9]*) ;;
    *) printf '%s\n' "usage: $0 vMAJOR.MINOR.PATCH [direct|relay]" >&2; exit 2 ;;
esac
case "$mode" in
    direct|relay) ;;
    *) printf '%s\n' "qualification mode must be direct or relay" >&2; exit 2 ;;
esac

version=${tag#v}
work_root=$(mktemp -d "${TMPDIR:-/tmp}/envshare-release-transfer.XXXXXX")
sender_pid=
node_pid=

cleanup() {
    if [ -n "$sender_pid" ]; then
        kill "$sender_pid" 2>/dev/null || true
        wait "$sender_pid" 2>/dev/null || true
    fi
    if [ -n "$node_pid" ]; then
        kill -INT "$node_pid" 2>/dev/null || true
        wait "$node_pid" 2>/dev/null || true
    fi
    find "$work_root" -type f -delete 2>/dev/null || true
    find "$work_root" -depth -type d -exec rmdir {} \; 2>/dev/null || true
}
trap cleanup EXIT HUP INT TERM

wait_for_text() {
    file=$1
    text=$2
    process=$3
    attempts=0
    while ! grep -F "$text" "$file" >/dev/null 2>&1; do
        if ! kill -0 "$process" 2>/dev/null; then
            printf '%s\n' "process exited before reporting $text" >&2
            return 1
        fi
        attempts=$((attempts + 1))
        if [ "$attempts" -ge 30 ]; then
            printf '%s\n' "timed out waiting for $text" >&2
            return 1
        fi
        sleep 1
    done
}

value_after() {
    sed -n "s/^$2//p" "$1" | head -n 1
}

install_client() {
    if [ -n "${ENVSHARE_BIN:-}" ]; then
        client_bin=$ENVSHARE_BIN
        return
    fi
    installer="$work_root/install.sh"
    curl --proto '=https' --tlsv1.2 -fsSL \
        "https://github.com/envoy1084/envshare/releases/download/$tag/install.sh" \
        -o "$installer"
    sh "$installer" --version "$version" --install-dir "$work_root/bin"
    client_bin="$work_root/bin/envshare"
}

run_direct_case() {
    transport=$1
    listen=$2
    input="$work_root/$transport.env"
    output="$work_root/$transport.received.env"
    sender_log="$work_root/$transport.sender.log"
    sender_error="$work_root/$transport.sender.err"
    printf '%s\n' "QUALIFICATION_${transport}=exact-private-value" > "$input"

    "$client_bin" send "$input" --expires 30s --listen "$listen" \
        > "$sender_log" 2> "$sender_error" &
    sender_pid=$!
    wait_for_text "$sender_log" "Direct address: " "$sender_pid"
    code=$(value_after "$sender_log" "Share code: ")
    peer=$(value_after "$sender_log" "Sender peer: ")
    address=$(value_after "$sender_log" "Direct address: ")
    test -n "$code" && test -n "$peer" && test -n "$address"

    "$client_bin" receive --code "$code" --peer "$peer" --address "$address" \
        --output "$output" > "$work_root/$transport.receiver.log" \
        2> "$work_root/$transport.receiver.err"
    cmp "$input" "$output"
    wait "$sender_pid"
    sender_pid=
}

install_node() {
    if [ -n "${ENVSHARE_NODE_BIN:-}" ]; then
        node_bin=$ENVSHARE_NODE_BIN
        return
    fi
    test "$(uname -s)" = Linux
    case $(uname -m) in
        x86_64|amd64) target=x86_64-unknown-linux-gnu ;;
        arm64|aarch64) target=aarch64-unknown-linux-gnu ;;
        *) printf '%s\n' "unsupported Linux node architecture" >&2; exit 2 ;;
    esac
    archive="node-$target.tar.xz"
    base="https://github.com/envoy1084/envshare/releases/download/$tag"
    curl --proto '=https' --tlsv1.2 -fsSL "$base/$archive" -o "$work_root/$archive"
    curl --proto '=https' --tlsv1.2 -fsSL "$base/$archive.sha256" \
        -o "$work_root/$archive.sha256"
    expected=$(awk '{ print $1; exit }' "$work_root/$archive.sha256")
    actual=$(sha256sum "$work_root/$archive" | awk '{ print $1 }')
    test -n "$expected" && test "$expected" = "$actual"
    mkdir "$work_root/node"
    tar -xJf "$work_root/$archive" -C "$work_root/node"
    node_bin=$(find "$work_root/node" -type f -name envshare-node -perm -u+x | head -n 1)
    test -n "$node_bin"
}

run_relay_case() {
    install_node
    identity="$work_root/node.identity"
    node_config="$work_root/node.toml"
    node_log="$work_root/node.log"
    "$node_bin" key generate --output "$identity" >/dev/null
    printf '%s\n' \
        'listen_addresses = ["/ip4/127.0.0.1/tcp/0"]' \
        'operations_address = "127.0.0.1:0"' \
        'discovery_allow_private_addresses = true' > "$node_config"
    "$node_bin" serve --identity "$identity" --config "$node_config" \
        > "$node_log" 2> "$work_root/node.err" &
    node_pid=$!
    wait_for_text "$node_log" "Listening: " "$node_pid"
    node_peer=$(value_after "$node_log" "Node peer: ")
    node_address=$(value_after "$node_log" "Listening: ")
    test -n "$node_peer" && test -n "$node_address"
    endpoint="$node_address/p2p/$node_peer"

    client_config="$work_root/client.toml"
    printf '%s\n' \
        'version = 1' \
        'default_network = "qualification"' \
        '' \
        '[networks.qualification]' \
        'network_id = "qualification"' \
        'require_relay = true' \
        "rendezvous = [\"$endpoint\"]" \
        "relays = [\"$endpoint\"]" > "$client_config"
    input="$work_root/relay.env"
    output="$work_root/relay.received.env"
    sender_log="$work_root/relay.sender.log"
    printf '%s\n' 'QUALIFICATION_RELAY=exact-private-value' > "$input"

    "$client_bin" --config "$client_config" send "$input" --expires 30s --relay-only \
        > "$sender_log" 2> "$work_root/relay.sender.err" &
    sender_pid=$!
    wait_for_text "$sender_log" "Relay address: " "$sender_pid"
    code=$(value_after "$sender_log" "Share code: ")
    test -n "$code"
    "$client_bin" --config "$client_config" receive --code "$code" --relay-only \
        --output "$output" > "$work_root/relay.receiver.log" \
        2> "$work_root/relay.receiver.err"
    cmp "$input" "$output"
    wait "$sender_pid"
    sender_pid=
    kill -INT "$node_pid"
    wait "$node_pid"
    node_pid=
}

install_client
test "$("$client_bin" --version)" = "envshare $version"

if [ "$mode" = direct ]; then
    run_direct_case quic /ip4/127.0.0.1/udp/0/quic-v1
    run_direct_case tcp /ip4/127.0.0.1/tcp/0
else
    run_relay_case
fi

printf '%s\n' "published $tag $mode transfer qualification passed"
