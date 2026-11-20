# Signs one file as Tauri bundles it, once a code-signing certificate has been set up.
# Until then it does nothing, and the installer is built unsigned, as before.
#
# Either of these turns signing on:
#   TRIBUTARY_SIGN_CERT      a .pfx certificate file, with TRIBUTARY_SIGN_PASSWORD
#   TRIBUTARY_AZURE_SIGN     Azure Trusted Signing, as "endpoint|account|profile", signed
#                            with trusted-signing-cli and the Azure login in effect
# Written for Windows PowerShell 5.1, which every Windows has.
param([Parameter(Mandatory = $true)][string]$File)

$ErrorActionPreference = 'Stop'
$timestamp = 'http://timestamp.digicert.com'

if ($env:TRIBUTARY_SIGN_CERT) {
    $signtool = Get-Command signtool.exe -ErrorAction SilentlyContinue
    if ($signtool) {
        $signtool = $signtool.Source
    } else {
        $signtool = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe' -ErrorAction SilentlyContinue |
            Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
    }
    if (-not $signtool) { throw 'signtool.exe was not found. Install the Windows SDK signing tools.' }
    & $signtool sign /fd SHA256 /f $env:TRIBUTARY_SIGN_CERT /p $env:TRIBUTARY_SIGN_PASSWORD /tr $timestamp /td SHA256 $File
    exit $LASTEXITCODE
}

if ($env:TRIBUTARY_AZURE_SIGN) {
    $endpoint, $account, $certificateProfile = $env:TRIBUTARY_AZURE_SIGN -split '\|'
    & trusted-signing-cli -e $endpoint -a $account -c $certificateProfile -d Tributary $File
    exit $LASTEXITCODE
}

# Nothing set up: the file stays unsigned.
exit 0
