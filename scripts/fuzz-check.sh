#!/bin/sh
# SPDX-License-Identifier: Apache-2.0

set -eu

duration=${1:-60}
case "$duration" in
    ''|*[!0-9]*)
        printf '%s\n' "usage: scripts/fuzz-check.sh [seconds-per-target]" >&2
        exit 2
        ;;
esac
[ "$duration" -ge 1 ] || {
    printf '%s\n' "seconds-per-target must be at least 1" >&2
    exit 2
}

toolchain=nightly-2026-08-21
expected_fuzz_version=0.13.2
repo_root=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
work_root=$(mktemp -d "${TMPDIR:-/tmp}/envshare-fuzz.XXXXXX")

cleanup() {
    rm -rf "$work_root"
}
trap cleanup EXIT HUP INT TERM

rustup run "$toolchain" rustc --version >/dev/null 2>&1 || {
    printf '%s\n' "install $toolchain with: rustup toolchain install $toolchain --profile minimal" >&2
    exit 1
}

fuzz_version=$(cargo "+$toolchain" fuzz --version | awk '{ print $2 }')
[ "$fuzz_version" = "$expected_fuzz_version" ] || {
    printf '%s\n' "expected cargo-fuzz $expected_fuzz_version, found $fuzz_version" >&2
    exit 1
}

cd "$repo_root"
for target in capability transcript cbor frame; do
    corpus="$work_root/$target"
    mkdir "$corpus"
    cp "fuzz/corpus/$target/"* "$corpus/"
    printf 'fuzzing %s for %s seconds\n' "$target" "$duration"
    cargo "+$toolchain" fuzz run "$target" "$corpus" --fuzz-dir fuzz -- \
        "-max_total_time=$duration" -print_final_stats=1
done

printf '%s\n' "all fuzz gates passed"
