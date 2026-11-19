<#
.SYNOPSIS
  Copies a built Tributary installer into website/downloads and records it in releases.json.

.DESCRIPTION
  Takes ../target/release/bundle/nsis/Tributary_<Version>_x64-setup.exe, copies it to
  website/downloads/, computes its size and SHA-256, and upserts the entry for <Version> in
  website/downloads/releases.json (the published date is the installer file's date).
  Release notes and requirements come from the table below; add an entry there for a new version.

.EXAMPLE
  ./scripts/publish-installer.ps1 -Version 0.1.0
#>
[CmdletBinding()]
param(
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version = '0.1.0'
)

$ErrorActionPreference = 'Stop'

$siteRoot  = Split-Path -Parent $PSScriptRoot
$repoRoot  = Split-Path -Parent $siteRoot
$fileName  = "Tributary_${Version}_x64-setup.exe"
$source    = Join-Path $repoRoot "target/release/bundle/nsis/$fileName"
$downloads = Join-Path $siteRoot 'downloads'
$manifest  = Join-Path $downloads 'releases.json'

$requirements = @(
    'Windows 10 or 11, 64-bit',
    'Installs for your user only; no administrator rights needed',
    'Microsoft Edge WebView2 (part of Windows 11; the installer fetches it if it is missing)',
    'Chrome or Edge for the browser extension (optional)'
)

$notesByVersion = @{
    '0.1.0' = @(
        'Splits each download across several connections, and resumes after a crash, a shutdown or a dropped connection.',
        'Takes over downloads from Chrome and Edge, with a right-click menu for links and a download button beside video players.',
        'Saves video, including HLS, DASH and YouTube through yt-dlp, and joins picture and sound into one file.',
        'Saves every picture and video on a page, and explores whole websites with the site grabber.',
        'Queues, a scheduler, a speed limiter and quotas, FTP, server logins and proxies.',
        'Keeps passwords in Windows Credential Manager. No account and no telemetry.'
    )
}
if (-not $notesByVersion.ContainsKey($Version)) {
    throw "No release notes for $Version. Add them to `$notesByVersion in this script."
}

if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
    throw "Installer not found: $source. Build it first (npm run tauri build)."
}

New-Item -ItemType Directory -Force -Path $downloads | Out-Null
$target = Join-Path $downloads $fileName
Copy-Item -LiteralPath $source -Destination $target -Force

$item   = Get-Item -LiteralPath $target
$sha256 = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant()
# Date of the installer file (the build), not of the copy.
$published = (Get-Item -LiteralPath $source).LastWriteTime.ToString('yyyy-MM-dd')

$entry = [ordered]@{
    version      = $Version
    file         = $fileName
    size         = [int64]$item.Length
    sha256       = $sha256
    published    = $published
    notes        = @($notesByVersion[$Version])
    requirements = $requirements
}

$releases = @()
if (Test-Path -LiteralPath $manifest) {
    $existing = Get-Content -LiteralPath $manifest -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($existing.releases) { $releases = @($existing.releases | Where-Object { $_.version -ne $Version }) }
}
$releases = @($releases) + @([pscustomobject]$entry) |
    Sort-Object { [version]$_.version } -Descending

$json = [ordered]@{ releases = @($releases) } | ConvertTo-Json -Depth 6
[System.IO.File]::WriteAllText($manifest, $json + "`n", (New-Object System.Text.UTF8Encoding($false)))

Write-Host "Published $fileName"
Write-Host "  size      $($item.Length) bytes"
Write-Host "  sha256    $sha256"
Write-Host "  published $published"
Write-Host "  manifest  $manifest"
