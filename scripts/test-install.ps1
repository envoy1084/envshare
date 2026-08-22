# SPDX-License-Identifier: Apache-2.0

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
$Binary = Join-Path $RepoRoot "target\release\envshare.exe"
if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) {
    throw "build target/release/envshare.exe before running this test"
}

$TestRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("envshare-installer-test-" + [guid]::NewGuid())
$FixtureDir = Join-Path $TestRoot "fixtures"
$Archive = "cli-x86_64-pc-windows-msvc.zip"
$ArchivePath = Join-Path $FixtureDir $Archive
$InstallDir = Join-Path $TestRoot "install"

try {
    New-Item -ItemType Directory -Path $FixtureDir | Out-Null
    Compress-Archive -LiteralPath $Binary -DestinationPath $ArchivePath
    $Digest = (Get-FileHash -LiteralPath $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content -LiteralPath "$ArchivePath.sha256" -Value "$Digest  $Archive" -NoNewline

    function global:Invoke-WebRequest {
        param(
            [Parameter(Mandatory = $true)][string]$Uri,
            [Parameter(Mandatory = $true)][string]$OutFile,
            [switch]$UseBasicParsing
        )
        $Name = [System.IO.Path]::GetFileName(([uri]$Uri).AbsolutePath)
        Copy-Item -LiteralPath (Join-Path $FixtureDir $Name) -Destination $OutFile
    }

    & (Join-Path $RepoRoot "install.ps1") -InstallDir $InstallDir
    $Installed = Join-Path $InstallDir "envshare.exe"
    if ((Get-FileHash $Installed).Hash -ne (Get-FileHash $Binary).Hash) {
        throw "installed binary differs from the release binary"
    }
    & $Installed --version | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "installed binary failed its version check"
    }

    $RejectedOverwrite = $false
    try {
        & (Join-Path $RepoRoot "install.ps1") -InstallDir $InstallDir
    }
    catch {
        $RejectedOverwrite = $true
    }
    if (-not $RejectedOverwrite) {
        throw "installer unexpectedly replaced an existing binary"
    }

    $InstalledDigest = (Get-FileHash $Installed).Hash
    Set-Content -LiteralPath "$ArchivePath.sha256" -Value (("0" * 64) + "  $Archive") -NoNewline
    $RejectedChecksum = $false
    try {
        & (Join-Path $RepoRoot "install.ps1") -InstallDir $InstallDir -Force
    }
    catch {
        $RejectedChecksum = $true
    }
    if (-not $RejectedChecksum) {
        throw "installer accepted a corrupt checksum"
    }
    if ((Get-FileHash $Installed).Hash -ne $InstalledDigest) {
        throw "checksum failure changed the installed binary"
    }

    Write-Output "install.ps1 smoke test passed for x86_64-pc-windows-msvc"
}
finally {
    Remove-Item function:\Invoke-WebRequest -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath $TestRoot) {
        Remove-Item -LiteralPath $TestRoot -Recurse -Force
    }
}
