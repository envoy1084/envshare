# Installation

Envshare publishes binaries for macOS, Linux, and Windows on
[GitHub Releases](https://github.com/envoy1084/envshare/releases).

## Homebrew

Install Envshare from the official tap on macOS or Linux:

```sh
brew install envoy1084/tap/envshare
```

## macOS and Linux

```sh
curl -fsSL https://github.com/envoy1084/envshare/releases/latest/download/install.sh | sh
```

The default destination is `$HOME/.local/bin/envshare`. If the command is not
found, add that directory to `PATH`:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

Add the same line to your shell startup file to keep it after a restart.

## Windows PowerShell

```powershell
$script = Invoke-RestMethod "https://github.com/envoy1084/envshare/releases/latest/download/install.ps1"
& ([scriptblock]::Create($script))
```

The default destination is
`%LOCALAPPDATA%\Programs\Envshare\bin\envshare.exe`. Open a new PowerShell
window after adding that directory to your user `PATH`.

## Verify the installation

```sh
envshare --version
envshare doctor
```

`doctor` checks local file safety, DNS, discovery, and relay connectivity. It
does not read an environment file.

## Install a specific version

Download the archive and matching checksum file from
[GitHub Releases](https://github.com/envoy1084/envshare/releases). Verify the
SHA-256 checksum, extract the archive, and place the binary on `PATH`.

## Update

Homebrew installations update with:

```sh
brew upgrade envshare
```

For installer-based installations, run the installer again. It downloads and
verifies the latest release before replacing the installed binary.

## Uninstall

macOS and Linux:

```sh
rm "$HOME/.local/bin/envshare"
```

Homebrew:

```sh
brew uninstall envshare
```

Windows PowerShell:

```powershell
Remove-Item "$env:LOCALAPPDATA\Programs\Envshare\bin\envshare.exe"
```

Envshare does not install a client background service. Removing the binary does
not remove received files or client configuration.
