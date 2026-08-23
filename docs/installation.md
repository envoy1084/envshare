# Installation

Envshare publishes versioned, attested archives and non-interactive installers
through GitHub Releases. The commands below deliberately pin a version; inspect
the release notes before changing it.

## Supported release targets

| System | Architectures | Installed program |
| --- | --- | --- |
| Linux with glibc | x86-64, Arm64 | `envshare` |
| macOS | Intel, Apple silicon | `envshare` |
| Windows | x86-64 | `envshare.exe` |

The self-hosted `envshare-node` is released as x86-64 and Arm64 glibc Linux
archives. See [deployment](deployment.md) for container and systemd installation.
Musl Linux, 32-bit systems, Windows Arm64, and other Unix systems are not release
targets yet; build from the pinned source and lockfile if you need one of them.

## Linux and macOS

Run the installer directly from the versioned HTTPS release:

```console
curl --proto '=https' --tlsv1.2 -LsSf \
  --connect-timeout 10 --max-time 120 \
  https://github.com/envoy1084/envshare/releases/download/v0.1.3/install.sh | sh
```

The default destination is `$HOME/.local/bin/envshare`. Select an absolute
destination without a prompt:

```console
curl --proto '=https' --tlsv1.2 -LsSf \
  --connect-timeout 10 --max-time 120 \
  https://github.com/envoy1084/envshare/releases/download/v0.1.3/install.sh |
  sh -s -- --version 0.1.3 --install-dir "$HOME/bin"
```

Pass `--force` only when intentionally replacing that path. `--dry-run` performs
platform resolution and prints the URLs and destination without accessing the
network or filesystem. The script does not run under `sudo`, alter `PATH`, edit a
shell profile, or collect telemetry.

## Windows PowerShell

Download and invoke the version-pinned installer:

```powershell
$version = "0.1.3"
$script = Invoke-RestMethod "https://github.com/envoy1084/envshare/releases/download/v$version/install.ps1"
& ([scriptblock]::Create($script)) -Version $version
```

The default destination is
`$env:LOCALAPPDATA\Programs\Envshare\bin\envshare.exe`. Choose another absolute
location with `-InstallDir`, replace intentionally with `-Force`, or resolve the
download without making changes with `-DryRun`:

```powershell
& ([scriptblock]::Create($script)) -Version $version -InstallDir "$HOME\bin" -DryRun
```

The PowerShell installer requires TLS 1.2 or newer and does not modify the user or
system `PATH`.

## Homebrew

Each release includes a cargo-dist-generated `envshare.rb` formula for the macOS
and Linux targets. Install the pinned formula asset after reviewing it:

```console
version=0.1.3
curl --proto '=https' --tlsv1.2 -LsSf \
  "https://github.com/envoy1084/envshare/releases/download/v$version/envshare.rb" \
  -o envshare.rb
brew install --formula ./envshare.rb
rm envshare.rb
```

This local-formula flow is the supported Homebrew route for the initial release.
The project does not yet claim a hosted `envshare/homebrew-tap`; once that
repository and its release credential exist, dist can publish the same generated
formula to it without changing archive names.

To remove the formula-managed installation:

```console
brew uninstall envshare
```

## Verification

Both installers download the target archive and its cargo-dist SHA-256 file over
HTTPS, validate the digest, extract only the expected release layout, and execute
`envshare --version` before atomically placing the binary. A checksum fetched from
the same release protects against corruption but is not an independent signature.

GitHub signs every archive, checksum, SBOM, and installer with a keyless artifact
attestation. With GitHub CLI installed, verify a downloaded installer before
running it:

```console
gh attestation verify install.sh \
  --repo envoy1084/envshare \
  --signer-workflow envoy1084/envshare/.github/workflows/release.yml
```

After installation, make sure the chosen directory is on `PATH`, then run:

```console
envshare --version
envshare --help
```

If the initial `curl ... install.sh | sh` request is blocked by a network or CDN
path, download the same attested release asset with GitHub CLI and run it locally:

```console
gh release download v0.1.3 --repo envoy1084/envshare --pattern install.sh
gh attestation verify install.sh --repo envoy1084/envshare
sh install.sh
```

Envshare is Apache-2.0-only. The release archive includes the project license.
