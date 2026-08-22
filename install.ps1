# SPDX-License-Identifier: Apache-2.0

[CmdletBinding()]
param(
    [string]$Version = $(if ($env:ENVSHARE_VERSION) { $env:ENVSHARE_VERSION } else { "0.1.1" }),
    [string]$InstallDir = $(if ($env:ENVSHARE_INSTALL_DIR) { $env:ENVSHARE_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\Envshare\bin" }),
    [switch]$Force,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
$Repository = "envoy1084/envshare"

function Stop-Install([string]$Message) {
    throw "envshare installer: $Message"
}

$Version = $Version.TrimStart("v")
if (-not $Version -or $Version -notmatch '^[0-9A-Za-z.-]+$') {
    Stop-Install "version is empty or contains unsupported characters"
}
if (-not [System.IO.Path]::IsPathFullyQualified($InstallDir)) {
    Stop-Install "install directory must be an absolute path"
}

if (-not $IsWindows -and $PSVersionTable.PSEdition -eq "Core") {
    Stop-Install "install.ps1 supports Windows only"
}
$architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
if ($architecture -ne [System.Runtime.InteropServices.Architecture]::X64) {
    Stop-Install "unsupported CPU architecture: $architecture (this release supports x64 Windows)"
}

$Target = "x86_64-pc-windows-msvc"
$Archive = "cli-$Target.zip"
$BaseUrl = "https://github.com/$Repository/releases/download/v$Version"
$ArchiveUrl = "$BaseUrl/$Archive"
$ChecksumUrl = "$ArchiveUrl.sha256"
$Destination = Join-Path $InstallDir "envshare.exe"

if ($DryRun) {
    [ordered]@{
        version = $Version
        target = $Target
        archive_url = $ArchiveUrl
        checksum_url = $ChecksumUrl
        install_path = $Destination
    } | ConvertTo-Json
    exit 0
}

if ((Test-Path -LiteralPath $Destination) -and -not $Force) {
    Stop-Install "$Destination already exists; pass -Force to replace it"
}

[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$WorkDir = Join-Path ([System.IO.Path]::GetTempPath()) ("envshare-install-" + [guid]::NewGuid())
$StagedPath = $null

try {
    New-Item -ItemType Directory -Path $WorkDir | Out-Null
    $ArchivePath = Join-Path $WorkDir $Archive
    $ChecksumPath = "$ArchivePath.sha256"

    Invoke-WebRequest -Uri $ArchiveUrl -OutFile $ArchivePath -UseBasicParsing
    Invoke-WebRequest -Uri $ChecksumUrl -OutFile $ChecksumPath -UseBasicParsing

    $Expected = ((Get-Content -LiteralPath $ChecksumPath -TotalCount 1) -split '\s+')[0]
    if ($Expected -notmatch '^[0-9A-Fa-f]{64}$') {
        Stop-Install "release checksum is malformed"
    }
    $Actual = (Get-FileHash -LiteralPath $ArchivePath -Algorithm SHA256).Hash
    if (-not $Actual.Equals($Expected, [System.StringComparison]::OrdinalIgnoreCase)) {
        Stop-Install "SHA-256 verification failed"
    }

    $ExtractDir = Join-Path $WorkDir "archive"
    Expand-Archive -LiteralPath $ArchivePath -DestinationPath $ExtractDir
    $Extracted = Join-Path $ExtractDir "envshare.exe"
    if (-not (Test-Path -LiteralPath $Extracted -PathType Leaf)) {
        Stop-Install "release archive does not contain envshare.exe"
    }
    & $Extracted --version | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Stop-Install "downloaded binary failed its version check"
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    $StagedPath = Join-Path $InstallDir (".envshare." + [guid]::NewGuid() + ".exe")
    Copy-Item -LiteralPath $Extracted -Destination $StagedPath
    Move-Item -LiteralPath $StagedPath -Destination $Destination -Force:$Force
    $StagedPath = $null

    Write-Output "Installed envshare $Version to $Destination"
}
finally {
    if ($StagedPath -and (Test-Path -LiteralPath $StagedPath)) {
        Remove-Item -LiteralPath $StagedPath -Force
    }
    if (Test-Path -LiteralPath $WorkDir) {
        Remove-Item -LiteralPath $WorkDir -Recurse -Force
    }
}
