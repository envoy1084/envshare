#!/bin/sh
# SPDX-License-Identifier: Apache-2.0

set -eu

[ "$(id -u)" -eq 0 ] || {
    printf '%s\n' "test-nat.sh must run as root in an isolated Linux CI runner" >&2
    exit 1
}
[ "$(uname -s)" = Linux ] || {
    printf '%s\n' "test-nat.sh supports Linux only" >&2
    exit 1
}

for command in ip iptables sysctl tc; do
    command -v "$command" >/dev/null 2>&1 || {
        printf '%s\n' "$command is required" >&2
        exit 1
    }
done

repo_root=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
binary="$repo_root/target/release/envshare"
[ -x "$binary" ] || {
    printf '%s\n' "build target/release/envshare before running this test" >&2
    exit 1
}

suffix=$$
sender_ns="envshare-s-$suffix"
receiver_ns="envshare-r-$suffix"
sender_link="es${suffix}s"
receiver_link="es${suffix}r"
sender_host=10.231.1.1
sender_guest=10.231.1.2
receiver_host=10.231.2.1
receiver_guest=10.231.2.2
test_root=$(mktemp -d "${TMPDIR:-/tmp}/envshare-nat-test.XXXXXX")
sender_pid=
port=
forward_rule_added=0
return_rule_added=0
dnat_rule_added=0
snat_rule_added=0
udp_rule_added=0
original_forward=$(sysctl -n net.ipv4.ip_forward)

cleanup() {
    if [ -n "$sender_pid" ]; then
        kill "$sender_pid" 2>/dev/null || true
        wait "$sender_pid" 2>/dev/null || true
    fi
    if [ "$udp_rule_added" -eq 1 ]; then
        iptables -D FORWARD -i "$receiver_link" -o "$sender_link" -p udp -j DROP 2>/dev/null || true
    fi
    if [ "$return_rule_added" -eq 1 ]; then
        iptables -D FORWARD -i "$sender_link" -o "$receiver_link" -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT 2>/dev/null || true
    fi
    if [ "$forward_rule_added" -eq 1 ]; then
        iptables -D FORWARD -i "$receiver_link" -o "$sender_link" -p tcp --dport "$port" -j ACCEPT 2>/dev/null || true
    fi
    if [ "$snat_rule_added" -eq 1 ]; then
        iptables -t nat -D POSTROUTING -o "$sender_link" -p tcp -d "$sender_guest" --dport "$port" -j MASQUERADE 2>/dev/null || true
    fi
    if [ "$dnat_rule_added" -eq 1 ]; then
        iptables -t nat -D PREROUTING -i "$receiver_link" -p tcp -d "$receiver_host" --dport "$port" -j DNAT --to-destination "$sender_guest:$port" 2>/dev/null || true
    fi
    ip link delete "$sender_link" 2>/dev/null || true
    ip link delete "$receiver_link" 2>/dev/null || true
    ip netns delete "$sender_ns" 2>/dev/null || true
    ip netns delete "$receiver_ns" 2>/dev/null || true
    sysctl -q -w "net.ipv4.ip_forward=$original_forward" >/dev/null 2>&1 || true
    rm -rf "$test_root"
}
trap cleanup EXIT HUP INT TERM

umask 077
printf 'NETWORK_EMULATION_SENTINEL=random-test-value\n' > "$test_root/input.env"

ip netns add "$sender_ns"
ip netns add "$receiver_ns"
ip link add "$sender_link" type veth peer name eth0 netns "$sender_ns"
ip link add "$receiver_link" type veth peer name eth0 netns "$receiver_ns"
ip address add "$sender_host/24" dev "$sender_link"
ip address add "$receiver_host/24" dev "$receiver_link"
ip link set "$sender_link" up
ip link set "$receiver_link" up

ip -n "$sender_ns" address add "$sender_guest/24" dev eth0
ip -n "$sender_ns" link set lo up
ip -n "$sender_ns" link set eth0 up
ip -n "$sender_ns" route add default via "$sender_host"
ip -n "$receiver_ns" address add "$receiver_guest/24" dev eth0
ip -n "$receiver_ns" link set lo up
ip -n "$receiver_ns" link set eth0 up
ip -n "$receiver_ns" route add default via "$receiver_host"

sysctl -q -w net.ipv4.ip_forward=1
tc qdisc add dev "$receiver_link" root netem delay 25ms 5ms

ip netns exec "$sender_ns" "$binary" send "$test_root/input.env" \
    --listen "/ip4/$sender_guest/tcp/0" > "$test_root/sender.out" 2> "$test_root/sender.err" &
sender_pid=$!

ready=0
for _ in 1 2 3 4 5 6 7 8 9 10; do
    if grep -q '^Direct address: ' "$test_root/sender.out"; then
        ready=1
        break
    fi
    if ! kill -0 "$sender_pid" 2>/dev/null; then
        printf '%s\n' "sender exited before becoming ready" >&2
        exit 1
    fi
    sleep 1
done
[ "$ready" -eq 1 ] || {
    printf '%s\n' "sender did not become ready" >&2
    exit 1
}

code=$(sed -n 's/^Share code: //p' "$test_root/sender.out")
peer=$(sed -n 's/^Sender peer: //p' "$test_root/sender.out")
direct=$(sed -n 's/^Direct address: //p' "$test_root/sender.out")
port=${direct##*/tcp/}
case "$port" in
    ''|*[!0-9]*) printf '%s\n' "sender produced an invalid TCP port" >&2; exit 1 ;;
esac
[ -n "$code" ] && [ -n "$peer" ] || {
    printf '%s\n' "sender readiness output was incomplete" >&2
    exit 1
}

iptables -t nat -A PREROUTING -i "$receiver_link" -p tcp -d "$receiver_host" --dport "$port" -j DNAT --to-destination "$sender_guest:$port"
dnat_rule_added=1
iptables -t nat -A POSTROUTING -o "$sender_link" -p tcp -d "$sender_guest" --dport "$port" -j MASQUERADE
snat_rule_added=1
iptables -I FORWARD 1 -i "$receiver_link" -o "$sender_link" -p tcp --dport "$port" -j ACCEPT
forward_rule_added=1
iptables -I FORWARD 1 -i "$sender_link" -o "$receiver_link" -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
return_rule_added=1
iptables -I FORWARD 1 -i "$receiver_link" -o "$sender_link" -p udp -j DROP
udp_rule_added=1

printf '%s\n' "$code" | ip netns exec "$receiver_ns" "$binary" receive \
    --code-stdin \
    --peer "$peer" \
    --address "/ip4/$receiver_host/tcp/$port" \
    --lan \
    --output "$test_root/received.env" \
    > "$test_root/receiver.out" 2> "$test_root/receiver.err"

wait "$sender_pid"
sender_pid=
cmp "$test_root/input.env" "$test_root/received.env"

if grep -Fq 'NETWORK_EMULATION_SENTINEL' "$test_root/sender.out" "$test_root/sender.err" "$test_root/receiver.out" "$test_root/receiver.err"; then
    printf '%s\n' "network-emulation logs exposed the payload sentinel" >&2
    exit 1
fi

printf '%s\n' "NAT, TCP fallback, and latency gate passed"
