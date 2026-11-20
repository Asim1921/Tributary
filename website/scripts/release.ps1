<#
.SYNOPSIS
  Makes a Tributary release in one go: sets the version everywhere, builds the signed
  installer, and records it in both release manifests.

.DESCRIPTION
  1. Sets <Version> in package.json, Cargo.toml, src-tauri/tauri.conf.json and the
     extension's manifest.json, which must all agree.
  2. Builds the installer with `npx tauri build`, signing the update with the key in
     <KeyFile>. The key never goes in the repository; back up its folder, because a lost key
     means users have to reinstall by hand once.
     The installer itself is code-signed too if src-tauri/scripts/sign.ps1 is set up.
  3. Runs publish-installer.ps1, which records size, SHA-256 and the update signature.
  Uploading the installer to Vercel Blob and pushing stay with you.

.EXAMPLE
  ./website/scripts/release.ps1 -Version 0.2.0
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version,
    [string]$KeyFile = 'D:\Tools\tributary-keys\updater.key'
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Set-Version([string]$path, [string]$pattern, [string]$replacement) {
    $full = Join-Path $repoRoot $path
    $text = [System.IO.File]::ReadAllText($full)
    $updated = [regex]::Replace($text, $pattern, $replacement, 'Multiline')
    if ($updated -eq $text -and $text -notmatch [regex]::Escape($Version)) {
        throw "Couldn't find the version in $path"
    }
    [System.IO.File]::WriteAllText($full, $updated, $utf8)
}

Set-Version 'package.json' '^(  "version": )"[^"]+"' "`$1`"$Version`""
Set-Version 'extension/manifest.json' '^(  "version": )"[^"]+"' "`$1`"$Version`""
Set-Version 'src-tauri/tauri.conf.json' '^(  "version": )"[^"]+"' "`$1`"$Version`""
Set-Version 'Cargo.toml' '^(version = )"[^"]+"' "`$1`"$Version`""
Write-Host "Version set to $Version"

if (-not (Test-Path -LiteralPath $KeyFile -PathType Leaf)) {
    throw "The update signing key isn't at $KeyFile."
}
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -LiteralPath $KeyFile -Raw
if (-not $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) { $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '' }

Push-Location $repoRoot
try {
    npx tauri build
    if ($LASTEXITCODE -ne 0) { throw "The build failed ($LASTEXITCODE)." }
} finally {
    Pop-Location
}

& (Join-Path $PSScriptRoot 'publish-installer.ps1') -Version $Version

Write-Host ''
Write-Host 'Left for you:'
Write-Host "  1. Upload website/downloads/Tributary_${Version}_x64-setup.exe to the tributary-downloads Blob store, under tributary/."
Write-Host '  2. Commit the version change and both releases.json files, and push when you are ready.'
Write-Host '  Once the API serves the new manifest, installed copies offer the update by themselves.'
