#!/bin/sh
# SPDX-License-Identifier: Apache-2.0

set -eu

INSTALLER_VERSION="0.1.0-alpha.1"
REPOSITORY="envshare/envshare"

usage() {
    cat <<'EOF'
Install Envshare from a versioned GitHub release.

Usage: install.sh [options]

Options:
  --version VERSION      Release version (default: installer version)
  --install-dir PATH     Destination directory (default: $HOME/.local/bin)
  --force                Replace an existing envshare binary
  --dry-run              Print the resolved download without changing anything
  -h, --help             Show this help

The installer is non-interactive. ENVSHARE_VERSION and ENVSHARE_INSTALL_DIR are
equivalent to the corresponding options. HTTPS is required for downloads.
EOF
}

fail() {
    printf '%s\n' "envshare installer: $*" >&2
    exit 1
}

version=${ENVSHARE_VERSION:-$INSTALLER_VERSION}
install_dir=${ENVSHARE_INSTALL_DIR:-}
force=0
dry_run=0

while [ "$#" -gt 0 ]; do
    case "$1" in
        --version)
            [ "$#" -ge 2 ] || fail "--version requires a value"
            version=$2
            shift 2
            ;;
        --install-dir)
            [ "$#" -ge 2 ] || fail "--install-dir requires a value"
            install_dir=$2
            shift 2
            ;;
        --force)
            force=1
            shift
            ;;
        --dry-run)
            dry_run=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            fail "unknown option: $1"
            ;;
    esac
done

if [ -z "$install_dir" ]; then
    [ -n "${HOME:-}" ] || fail "HOME is unset; pass --install-dir"
    install_dir=$HOME/.local/bin
fi

version=${version#v}
[ -n "$version" ] || fail "version cannot be empty"
case "$version" in
    *[!0-9A-Za-z.-]*) fail "version contains unsupported characters" ;;
esac
[ -n "$install_dir" ] || fail "install directory cannot be empty"
case "$install_dir" in
    /*) ;;
    *) fail "install directory must be an absolute path" ;;
esac

case $(uname -s 2>/dev/null) in
    Darwin) os=apple-darwin ;;
    Linux) os=unknown-linux-gnu ;;
    *) fail "unsupported operating system; use install.ps1 on Windows" ;;
esac

case $(uname -m 2>/dev/null) in
    x86_64|amd64) arch=x86_64 ;;
    arm64|aarch64) arch=aarch64 ;;
    *) fail "unsupported CPU architecture: $(uname -m 2>/dev/null || printf unknown)" ;;
esac

if [ "$os" = unknown-linux-gnu ]; then
    if command -v ldd >/dev/null 2>&1 && ldd --version 2>&1 | grep -qi musl; then
        fail "musl Linux is not supported by this release; use a glibc system or build from source"
    fi
fi

target="$arch-$os"
archive="cli-$target.tar.xz"
base_url="https://github.com/$REPOSITORY/releases/download/v$version"
archive_url="$base_url/$archive"
checksum_url="$archive_url.sha256"
destination="$install_dir/envshare"

if [ "$dry_run" -eq 1 ]; then
    printf 'version=%s\ntarget=%s\narchive_url=%s\nchecksum_url=%s\ninstall_path=%s\n' \
        "$version" "$target" "$archive_url" "$checksum_url" "$destination"
    exit 0
fi

command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v tar >/dev/null 2>&1 || fail "tar with xz support is required"

if [ -e "$destination" ] && [ "$force" -ne 1 ]; then
    fail "$destination already exists; pass --force to replace it"
fi

umask 077
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/envshare-install.XXXXXX") || fail "could not create a temporary directory"
staged_path=
cleanup() {
    if [ -n "$staged_path" ]; then
        rm -f "$staged_path"
    fi
    rm -rf "$work_dir"
}
trap cleanup EXIT HUP INT TERM

curl --proto '=https' --tlsv1.2 --fail --location --silent --show-error \
    --output "$work_dir/$archive" "$archive_url"
curl --proto '=https' --tlsv1.2 --fail --location --silent --show-error \
    --output "$work_dir/$archive.sha256" "$checksum_url"

expected=$(awk 'NR == 1 { print $1 }' "$work_dir/$archive.sha256")
case "$expected" in
    ''|*[!0-9A-Fa-f]*) fail "release checksum is malformed" ;;
esac
[ "${#expected}" -eq 64 ] || fail "release checksum is not SHA-256"

if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$work_dir/$archive" | awk '{ print $1 }')
elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$work_dir/$archive" | awk '{ print $1 }')
else
    fail "sha256sum or shasum is required"
fi
[ "$actual" = "$expected" ] || fail "SHA-256 verification failed"

tar -xJf "$work_dir/$archive" -C "$work_dir"
extracted="$work_dir/cli-$target/envshare"
[ -f "$extracted" ] || fail "release archive does not contain envshare"
chmod 0755 "$extracted"
"$extracted" --version >/dev/null || fail "downloaded binary failed its version check"

mkdir -p "$install_dir"
staged_path=$(mktemp "$install_dir/.envshare.XXXXXX") || fail "cannot stage the binary in $install_dir"
cp "$extracted" "$staged_path"
chmod 0755 "$staged_path"
mv -f "$staged_path" "$destination"
staged_path=

printf 'Installed envshare %s to %s\n' "$version" "$destination"
