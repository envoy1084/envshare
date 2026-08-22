#!/bin/sh
# SPDX-License-Identifier: Apache-2.0

set -eu

allow_dirty=0
if [ "${1:-}" = "--allow-dirty" ]; then
    allow_dirty=1
    shift
fi
[ "$#" -eq 0 ] || {
    printf '%s\n' "usage: scripts/release-check.sh [--allow-dirty]" >&2
    exit 2
}

repo_root=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
cd "$repo_root"

command -v dist >/dev/null 2>&1 || {
    printf '%s\n' "dist 0.32.0 is required" >&2
    exit 1
}
command -v jq >/dev/null 2>&1 || {
    printf '%s\n' "jq is required" >&2
    exit 1
}
command -v shellcheck >/dev/null 2>&1 || {
    printf '%s\n' "shellcheck is required" >&2
    exit 1
}
command -v yq >/dev/null 2>&1 || {
    printf '%s\n' "yq is required" >&2
    exit 1
}

if [ "$allow_dirty" -ne 1 ] && [ -n "$(git status --porcelain)" ]; then
    printf '%s\n' "release check requires a clean working tree" >&2
    exit 1
fi

dist_version=$(dist --version | awk '{ print $2 }')
[ "$dist_version" = "0.32.0" ] || {
    printf '%s\n' "expected dist 0.32.0, found $dist_version" >&2
    exit 1
}

metadata=$(cargo metadata --locked --no-deps --format-version 1)
client_version=$(printf '%s' "$metadata" | jq -r '.packages[] | select(.name == "cli") | .version')
node_version=$(printf '%s' "$metadata" | jq -r '.packages[] | select(.name == "node") | .version')
[ "$client_version" = "$node_version" ] || {
    printf '%s\n' "client and node versions differ" >&2
    exit 1
}

shell_installer_version=$(sed -n 's/^INSTALLER_VERSION="\([^"]*\)"/\1/p' install.sh)
powershell_installer_version=$(sed -n 's/.*else { "\([^"]*\)" }).*/\1/p' install.ps1)
[ "$client_version" = "$shell_installer_version" ] || {
    printf '%s\n' "install.sh version does not match Cargo metadata" >&2
    exit 1
}
[ "$client_version" = "$powershell_installer_version" ] || {
    printf '%s\n' "install.ps1 version does not match Cargo metadata" >&2
    exit 1
}
grep -Fq "## [$client_version]" CHANGELOG.md || {
    printf '%s\n' "CHANGELOG.md has no section for $client_version" >&2
    exit 1
}

plan=$(mktemp "${TMPDIR:-/tmp}/envshare-release-plan.XXXXXX")
cleanup() {
    rm -f "$plan"
}
trap cleanup EXIT HUP INT TERM

dist plan --tag "v$client_version" --output-format=json --no-local-paths > "$plan"

for artifact in \
    cli-aarch64-apple-darwin.tar.xz \
    cli-aarch64-unknown-linux-gnu.tar.xz \
    cli-x86_64-apple-darwin.tar.xz \
    cli-x86_64-pc-windows-msvc.zip \
    cli-x86_64-unknown-linux-gnu.tar.xz \
    cli-installer.sh \
    cli-installer.ps1 \
    envshare.rb \
    cli.cdx.xml \
    node-aarch64-unknown-linux-gnu.tar.xz \
    node-x86_64-unknown-linux-gnu.tar.xz \
    node.cdx.xml
do
    jq -e --arg artifact "$artifact" \
        '[.releases[].artifacts[]] | index($artifact) != null' "$plan" >/dev/null || {
        printf '%s\n' "release plan is missing $artifact" >&2
        exit 1
    }
done

if jq -e '[.releases[] | select(.app_name == "node") | .artifacts[]] | any(test("apple|windows"))' "$plan" >/dev/null; then
    printf '%s\n' "node release unexpectedly includes a non-Linux archive" >&2
    exit 1
fi

dist build --tag "v$client_version" --artifacts=lies --output-format=json >/dev/null
shellcheck -s sh install.sh scripts/test-install.sh scripts/release-check.sh
yq eval '.' .github/workflows/ci.yml >/dev/null
yq eval '.' .github/workflows/release.yml >/dev/null

if command -v pwsh >/dev/null 2>&1; then
    pwsh -NoLogo -NoProfile -Command \
        '[scriptblock]::Create((Get-Content -Raw install.ps1)) | Out-Null; [scriptblock]::Create((Get-Content -Raw scripts/test-install.ps1)) | Out-Null'
fi

printf 'release dry-run passed for v%s\n' "$client_version"
