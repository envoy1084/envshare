#!/bin/sh
# SPDX-License-Identifier: Apache-2.0

set -eu

repo_root=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
binary="$repo_root/target/release/envshare"
[ -x "$binary" ] || {
    printf '%s\n' "build target/release/envshare before running this test" >&2
    exit 1
}

case $(uname -s) in
    Darwin) os=apple-darwin ;;
    Linux) os=unknown-linux-gnu ;;
    *) printf '%s\n' "unsupported installer test host" >&2; exit 1 ;;
esac
case $(uname -m) in
    x86_64|amd64) arch=x86_64 ;;
    arm64|aarch64) arch=aarch64 ;;
    *) printf '%s\n' "unsupported installer test architecture" >&2; exit 1 ;;
esac

target="$arch-$os"
archive="cli-$target.tar.xz"
test_root=$(mktemp -d "${TMPDIR:-/tmp}/envshare-installer-test.XXXXXX")
cleanup() {
    rm -rf "$test_root"
}
trap cleanup EXIT HUP INT TERM

fixture_dir="$test_root/fixtures"
archive_root="$test_root/cli-$target"
mock_dir="$test_root/mock-bin"
install_dir="$test_root/install"
mkdir -p "$fixture_dir" "$archive_root" "$mock_dir"
cp "$binary" "$archive_root/envshare"
tar -cJf "$fixture_dir/$archive" -C "$test_root" "cli-$target"

if command -v sha256sum >/dev/null 2>&1; then
    digest=$(sha256sum "$fixture_dir/$archive" | awk '{ print $1 }')
else
    digest=$(shasum -a 256 "$fixture_dir/$archive" | awk '{ print $1 }')
fi
printf '%s  %s\n' "$digest" "$archive" > "$fixture_dir/$archive.sha256"

cat > "$mock_dir/curl" <<'EOF'
#!/bin/sh
set -eu
output=
url=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --output) output=$2; shift 2 ;;
        --proto|--connect-timeout|--max-time|--retry|--retry-delay) shift 2 ;;
        --tlsv1.2) shift ;;
        --fail|--location|--silent|--show-error) shift ;;
        *) url=$1; shift ;;
    esac
done
[ -n "$output" ] && [ -n "$url" ]
cp "$ENVSHARE_INSTALL_FIXTURES/${url##*/}" "$output"
EOF
chmod 0755 "$mock_dir/curl"

ENVSHARE_INSTALL_FIXTURES=$fixture_dir
export ENVSHARE_INSTALL_FIXTURES
PATH="$mock_dir:$PATH" "$repo_root/install.sh" --install-dir "$install_dir"
cmp "$binary" "$install_dir/envshare"
"$install_dir/envshare" --version >/dev/null

if PATH="$mock_dir:$PATH" "$repo_root/install.sh" --install-dir "$install_dir" >/dev/null 2>&1; then
    printf '%s\n' "installer unexpectedly replaced an existing binary" >&2
    exit 1
fi

installed_digest=$(
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$install_dir/envshare" | awk '{ print $1 }'
    else
        shasum -a 256 "$install_dir/envshare" | awk '{ print $1 }'
    fi
)
printf '%064d  %s\n' 0 "$archive" > "$fixture_dir/$archive.sha256"
if PATH="$mock_dir:$PATH" "$repo_root/install.sh" --install-dir "$install_dir" --force >/dev/null 2>&1; then
    printf '%s\n' "installer accepted a corrupt checksum" >&2
    exit 1
fi
after_failure=$(
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$install_dir/envshare" | awk '{ print $1 }'
    else
        shasum -a 256 "$install_dir/envshare" | awk '{ print $1 }'
    fi
)
[ "$installed_digest" = "$after_failure" ]

printf '%s\n' "install.sh smoke test passed for $target"
