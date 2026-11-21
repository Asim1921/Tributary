<#
.SYNOPSIS
  Copies a built Tributary installer into website/downloads and records it in releases.json.

.DESCRIPTION
  Takes ../target/release/bundle/nsis/Tributary_<Version>_x64-setup.exe, copies it to
  website/downloads/, computes its size and SHA-256, and upserts the entry for <Version> in
  website/downloads/releases.json (the published date is the installer file's date).
  The same manifest is also written to website/backend/releases.json: the API project on Vercel
  only sees the backend folder, so it reads that copy. Commit it; the installer itself stays out
  of git (upload it to Vercel Blob, see README "Deploying on Vercel").
  Release notes and requirements come from the table below; add an entry there for a new version.
  If the build left an updater signature (<installer>.sig, made when TAURI_SIGNING_PRIVATE_KEY
  is set), it is recorded too, and the app's updater is offered this release.

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
$apiManifest = Join-Path (Join-Path $siteRoot 'backend') 'releases.json'

$requirements = @(
    'Windows 10 or 11, 64-bit',
    'Installs for your user only; no administrator rights needed',
    'Microsoft Edge WebView2 (part of Windows 11; the installer fetches it if it is missing)',
    'Chrome or Edge for the browser extension (optional)'
)

$notesByVersion = @{
    # The first release since 0.1.0 to be published, so its notes cover 0.2.0 as well.
    '0.3.0' = @(
        'Downloads torrents and magnet links: you choose which files to get, and it stops sharing when done unless you ask it to share as much as it got.',
        'Speaks Urdu, Arabic, Hindi and Spanish as well as English, right to left for Urdu and Arabic, following the language of Windows until you choose another.',
        'Updates itself: a newer version is fetched in the background, checked against its signature, and installed when you say so.',
        'Checks downloads against the checksum a site gives, and marks them as coming from the internet so Windows checks them before they open.',
        'Saves whole playlists and channels, only part of a video, or just its sound as M4A or MP3, with subtitles and the cover put inside.',
        'Fetches one file from several servers at once, from mirrors and Metalinks, and carries on with a fresh link when a link expires.',
        'Works in Chromium browsers such as Vivaldi, Brave and Opera as well as Chrome and Edge, and takes links sent from your phone on the home network.',
        'Shows a Windows notification when downloads finish in the background, one for a whole batch or queue.'
    )
    '0.2.0' = @(
        'Updates itself: a newer version is fetched in the background, checked against its signature, and installed when you say so.',
        'Checks downloads against the MD5, SHA-1, SHA-256 or SHA-512 a site gives, and can find one published beside the file.',
        'Marks downloads as coming from the internet, as browsers do, so Windows checks them before they open.',
        'Saves just the sound of a video, as M4A or MP3, and puts subtitles and the cover picture into videos.',
        'Shows a Windows notification when downloads finish in the background, one for a whole batch or queue.'
    )
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
$signatureFile = "$source.sig"
if (Test-Path -LiteralPath $signatureFile -PathType Leaf) {
    $entry.signature = (Get-Content -LiteralPath $signatureFile -Raw).Trim()
}

$releases = @()
if (Test-Path -LiteralPath $manifest) {
    $existing = Get-Content -LiteralPath $manifest -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($existing.releases) { $releases = @($existing.releases | Where-Object { $_.version -ne $Version }) }
}
$releases = @($releases) + @([pscustomobject]$entry) |
    Sort-Object { [version]$_.version } -Descending

$json = [ordered]@{ releases = @($releases) } | ConvertTo-Json -Depth 6
$utf8 = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($manifest, $json + "`n", $utf8)
[System.IO.File]::WriteAllText($apiManifest, $json + "`n", $utf8)

Write-Host "Published $fileName"
Write-Host "  size      $($item.Length) bytes"
Write-Host "  sha256    $sha256"
Write-Host "  published $published"
Write-Host "  manifest  $manifest"
Write-Host "  api copy  $apiManifest"
