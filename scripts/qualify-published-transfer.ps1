# SPDX-License-Identifier: Apache-2.0

[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$Tag)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
if ($Tag -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$') {
    throw "release tag must be a semantic version prefixed by v"
}
$Version = $Tag.TrimStart("v")
$WorkRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("envshare-transfer-" + [guid]::NewGuid())
$Sender = $null

function Wait-ForText([string]$Path, [string]$Text, [System.Diagnostics.Process]$Process) {
    for ($attempt = 0; $attempt -lt 60; $attempt++) {
        if ((Test-Path -LiteralPath $Path) -and (Select-String -LiteralPath $Path -SimpleMatch $Text -Quiet)) {
            return
        }
        if ($Process.HasExited) {
            throw "sender exited before reporting $Text"
        }
        Start-Sleep -Milliseconds 500
    }
    throw "timed out waiting for $Text"
}

function Get-ValueAfter([string]$Path, [string]$Prefix) {
    $line = Get-Content -LiteralPath $Path | Where-Object { $_.StartsWith($Prefix) } | Select-Object -First 1
    if (-not $line) { throw "missing sender output: $Prefix" }
    return $line.Substring($Prefix.Length)
}

function Run-DirectCase([string]$Transport, [string]$Listen) {
    $script:Sender = $null
    $InputPath = Join-Path $WorkRoot "$Transport.env"
    $OutputPath = Join-Path $WorkRoot "$Transport.received.env"
    $SenderLog = Join-Path $WorkRoot "$Transport.sender.log"
    $SenderError = Join-Path $WorkRoot "$Transport.sender.err"
    [System.IO.File]::WriteAllText($InputPath, "QUALIFICATION_$Transport=exact-private-value`n")
    $script:Sender = Start-Process -FilePath $Binary -ArgumentList @(
        "send", $InputPath, "--network", "qualification-direct", "--verbose",
        "--expires", "30s", "--listen", $Listen
    ) -RedirectStandardOutput $SenderLog -RedirectStandardError $SenderError -PassThru
    Wait-ForText $SenderLog "Direct address: " $script:Sender
    $Code = Get-ValueAfter $SenderLog "Share code: "
    $Peer = Get-ValueAfter $SenderLog "Sender peer: "
    $Address = Get-ValueAfter $SenderLog "Direct address: "
    & $Binary receive --network qualification-direct --code $Code --peer $Peer `
        --address $Address --output $OutputPath |
        Out-File -LiteralPath (Join-Path $WorkRoot "$Transport.receiver.log")
    if ($LASTEXITCODE -ne 0) { throw "$Transport receiver failed" }
    $InputBytes = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes($InputPath))
    $OutputBytes = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes($OutputPath))
    if ($InputBytes -ne $OutputBytes) {
        throw "$Transport payload changed"
    }
    if (-not $script:Sender.WaitForExit(30000)) { throw "$Transport sender did not exit" }
    if ($script:Sender.ExitCode -ne 0) { throw "$Transport sender failed" }
    $script:Sender = $null
}

try {
    New-Item -ItemType Directory -Path $WorkRoot | Out-Null
    $BinDir = Join-Path $WorkRoot "bin"
    $Installer = Join-Path $WorkRoot "install.ps1"
    Invoke-WebRequest -Uri "https://github.com/envoy1084/envshare/releases/download/$Tag/install.ps1" -OutFile $Installer -UseBasicParsing
    & $Installer -Version $Version -InstallDir $BinDir | Out-Null
    $Binary = Join-Path $BinDir "envshare.exe"
    if ((& $Binary --version) -ne "envshare $Version") { throw "installed version mismatch" }
    Run-DirectCase "quic" "/ip4/127.0.0.1/udp/0/quic-v1"
    Run-DirectCase "tcp" "/ip4/127.0.0.1/tcp/0"
    Write-Output "published $Tag direct transfer qualification passed"
}
finally {
    if ($Sender -and -not $Sender.HasExited) {
        Stop-Process -Id $Sender.Id -Force
        $Sender.WaitForExit()
    }
    if (Test-Path -LiteralPath $WorkRoot) {
        Remove-Item -LiteralPath $WorkRoot -Recurse -Force
    }
}
