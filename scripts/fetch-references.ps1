<#
.SYNOPSIS
    Fetches the third-party reference repositories used to derive the FIFA 22
    Blaze Fire2 / Heat2 protocol implementation.

.DESCRIPTION
    The reference repositories are NOT committed to this repository. They are
    third-party projects with their own licenses, and one of them vendors ~97 MB
    of prebuilt OpenSSL binaries. This script clones each one at a pinned commit
    so that protocol research is reproducible byte-for-byte.

    Usage:
        .\scripts\fetch-references.ps1
        .\scripts\fetch-references.ps1 -Force

    If your machine's execution policy blocks local scripts, run it from an
    already-permitted session or ask your administrator to allow
    `LocalPolicy` execution for this file. The script does not need to
    elevate privileges or bypass the policy itself.

.PARAMETER Force
    Re-clone even if the target directory already exists.
#>

[CmdletBinding()]
param(
    [switch]$Force
)

$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$RefDir   = Join-Path $RepoRoot 'references'

# name, url, pinned commit, purpose
$References = @(
    @{
        Name    = 'hp-server'
        Url     = 'https://github.com/mxmstr/hp-server.git'
        Commit  = '59f6468a8b358743962e0acf5d90e42d904e9f1a'
        Purpose = 'Primary Fire2 header + Heat2 TDF codec reference; also contains authentic EA capture fixtures used to validate the codec.'
    }
    @{
        Name    = 'BlazeSDK'
        Url     = 'https://github.com/Khysnik/BlazeSDK.git'
        Commit  = 'a79b3d4a46a394c7bb256bc702bff3c72fd99366'
        Purpose = 'Independent second Fire2/Heat2 implementation, used to confirm the byte layout agrees with hp-server.'
    }
    @{
        Name    = 'openBlase'
        Url     = 'https://github.com/openBlase/openBlase.git'
        Commit  = 'ca33729e35be4be90a6e5df1e50322df9cf2e1b9'
        Purpose = 'Heat2 label compression and the commented-out Tdf::CompressInteger, which corroborates the 6-bit-first-byte varsize scheme.'
    }
    @{
        Name    = 'open-ds2-server'
        Url     = 'https://github.com/lowlevelmetal/open-ds2-server.git'
        Commit  = '7702bbf348813dfce8be62bcfacd565009a34a28'
        Purpose = 'docs/04-blaze-connection-analysis.md - a DIFFERENT Blaze generation (12-byte header, LEB128 varint). Not applicable to FIFA 22; background only.'
    }
    @{
        Name    = 'Skate3BlazeServer'
        Url     = 'https://github.com/skate6743/Skate3BlazeServer.git'
        Commit  = 'f248282ce91f856cbeb2f623adc966df0adfe0e3'
        Purpose = 'TDF schema model classes. Source of the NetworkAddress union case numbers and the INIP/EXIP member tag names.'
    }
)

if (-not (Test-Path $RefDir)) {
    New-Item -ItemType Directory -Path $RefDir | Out-Null
}

foreach ($ref in $References) {
    $target = Join-Path $RefDir $ref.Name

    if ((Test-Path $target) -and -not $Force) {
        Write-Host "[skip] $($ref.Name) already present (use -Force to re-clone)" -ForegroundColor DarkGray
        continue
    }

    if ((Test-Path $target) -and $Force) {
        Write-Host "[rm   ] $($ref.Name)" -ForegroundColor DarkGray
        Remove-Item -Recurse -Force $target
    }

    Write-Host "[clone] $($ref.Name) <- $($ref.Url)" -ForegroundColor Cyan
    & git clone --quiet $ref.Url $target
    if ($LASTEXITCODE -ne 0) {
        throw "git clone failed for $($ref.Name)"
    }

    Push-Location $target
    try {
        & git checkout --quiet $ref.Commit
        if ($LASTEXITCODE -ne 0) {
            throw "git checkout $($ref.Commit) failed for $($ref.Name)"
        }
        $head = (& git rev-parse HEAD).Trim()
    }
    finally {
        Pop-Location
    }

    if ($head -ne $ref.Commit) {
        throw "$($ref.Name) is at $head but expected $($ref.Commit)"
    }

    Write-Host "[ok   ] $($ref.Name) pinned at $head" -ForegroundColor Green
    Write-Host "       $($ref.Purpose)" -ForegroundColor DarkGray
}

Write-Host ""
Write-Host "All reference repositories are present under $RefDir" -ForegroundColor Green
Write-Host "See docs/BLAZE_FIRE2_HEAT2_PROTOCOL.md for what each one established." -ForegroundColor Green
