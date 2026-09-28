<#
.SYNOPSIS
  Installs or removes the VB-Audio Virtual Cable driver for Voice Tuner (FR-15).

.DESCRIPTION
  Called by the NSIS installer hooks (installer/nsis/hooks.nsh), elevated.

  Install:
    1. Exit 0 if VB-Cable is already present.
    2. Use the driver pack in -PackDir (bundled when licensed), otherwise
       download it from VB-Audio (link-out install) and keep a copy in
       -PackDir so the uninstaller can remove the driver later.
    3. Verify the Authenticode signatures of the setup program.
    4. Trust VB-Audio's publisher certificate (TrustedPublisher) so the driver
       install shows no Windows Security prompt.
    5. Run VBCABLE_Setup_x64.exe -i -h silently.
    6. Record InstalledVBCable=1 under -RegistryKey and exit 3010 (reboot).

  Uninstall:
    Runs VBCABLE_Setup_x64.exe -u -h only if -RegistryKey says we installed it.

  Exit codes: 0 = nothing to do, 3010 = done (reboot required), 1 = failed.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File install-vbcable.ps1 -Action Install -PackDir C:\x -DryRun
#>
# Script parameters are read inside the functions below; PSScriptAnalyzer
# cannot see that across scopes.
[Diagnostics.CodeAnalysis.SuppressMessageAttribute('PSReviewUnusedParameter', '', Justification = 'Used by functions in this script')]
[CmdletBinding()]
param(
    [ValidateSet('Install', 'Uninstall', 'Detect')]
    [string] $Action = 'Install',
    [string] $PackDir = (Join-Path $PSScriptRoot 'vbcable'),
    [string] $RegistryKey = 'HKLM:\Software\VoiceTuner',
    [string] $DownloadUrl = 'https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip',
    [switch] $DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$SetupName = 'VBCABLE_Setup_x64.exe'
$LogFile = Join-Path $env:TEMP 'voice-tuner-vbcable.log'

function Write-VtLog([string] $Message) {
    $line = '{0:u} {1}' -f (Get-Date), $Message
    Write-Output $line
    try { Add-Content -Path $LogFile -Value $line -ErrorAction Stop }
    catch { Write-Verbose "could not write ${LogFile}: $_" }
}

function Test-VBCableInstalled {
    # Endpoint names are the most reliable signal: "CABLE Input"/"CABLE Output (VB-Audio Virtual Cable)".
    $devices = @(Get-PnpDevice -FriendlyName '*VB-Audio Virtual Cable*' -ErrorAction SilentlyContinue |
        Where-Object { $_.Status -ne 'Unknown' })
    if ($devices.Count -gt 0) { return $true }
    $mmdev = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render'
    if (Test-Path $mmdev) {
        foreach ($ep in Get-ChildItem $mmdev -ErrorAction SilentlyContinue) {
            $props = Join-Path $ep.PSPath 'Properties'
            $name = (Get-ItemProperty -Path $props -ErrorAction SilentlyContinue).'{a45c254e-df1c-4efd-8020-67d146a850e0},2'
            if ($name -like 'CABLE Input*') { return $true }
        }
    }
    return $false
}

function Test-TrustedVBAudioSignature([string] $Path) {
    $sig = Get-AuthenticodeSignature -FilePath $Path
    if ($sig.Status -ne 'Valid') { return $false }
    return ($sig.SignerCertificate.Subject -match 'VB-Audio|Burel')
}

function Get-DriverPack {
    $setup = Join-Path $PackDir $SetupName
    if (Test-Path $setup) { return $setup }
    Write-VtLog "Driver pack not bundled; downloading $DownloadUrl"
    if ($DryRun) { return $setup }
    New-Item -ItemType Directory -Force -Path $PackDir | Out-Null
    $zip = Join-Path $env:TEMP 'VBCABLE_Driver_Pack.zip'
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $zip -UseBasicParsing
    Expand-Archive -Path $zip -DestinationPath $PackDir -Force
    Remove-Item $zip -Force -ErrorAction SilentlyContinue
    if (-not (Test-Path $setup)) { throw "$SetupName not found in the downloaded pack" }
    return $setup
}

function Add-VBAudioTrustedPublisher([string] $Dir) {
    # The driver catalogs are signed with VB-Audio's certificate; trusting it
    # suppresses the "Would you like to install this device software?" prompt.
    $files = @(Get-ChildItem -Path $Dir -Include '*.cat', $SetupName -Recurse -ErrorAction SilentlyContinue)
    $store = New-Object System.Security.Cryptography.X509Certificates.X509Store('TrustedPublisher', 'LocalMachine')
    $store.Open('ReadWrite')
    try {
        foreach ($f in $files) {
            $sig = Get-AuthenticodeSignature -FilePath $f.FullName
            $cert = $sig.SignerCertificate
            if ($sig.Status -eq 'Valid' -and $cert -and $cert.Subject -match 'VB-Audio|Burel') {
                Write-VtLog "Trusting publisher $($cert.Subject) ($($cert.Thumbprint))"
                if (-not $DryRun) { $store.Add($cert) }
            }
        }
    }
    finally { $store.Close() }
}

function Save-Marker([bool] $Installed) {
    if ($DryRun) { Write-VtLog "Would set InstalledVBCable=$Installed at $RegistryKey"; return }
    if ($Installed) {
        New-Item -Path $RegistryKey -Force | Out-Null
        New-ItemProperty -Path $RegistryKey -Name 'InstalledVBCable' -PropertyType DWord -Value 1 -Force | Out-Null
    }
    elseif (Test-Path $RegistryKey) {
        Remove-ItemProperty -Path $RegistryKey -Name 'InstalledVBCable' -ErrorAction SilentlyContinue
    }
}

function Test-Marker {
    if (-not (Test-Path $RegistryKey)) { return $false }
    return ((Get-ItemProperty -Path $RegistryKey -ErrorAction SilentlyContinue).InstalledVBCable -eq 1)
}

function Invoke-Setup([string] $Setup, [string[]] $Arguments) {
    Write-VtLog "Running $Setup $($Arguments -join ' ')"
    if ($DryRun) { return 0 }
    $p = Start-Process -FilePath $Setup -ArgumentList $Arguments -Wait -PassThru -WindowStyle Hidden
    return $p.ExitCode
}

function Invoke-Install {
    if (Test-VBCableInstalled) { Write-VtLog 'VB-Cable already installed.'; return 0 }
    $setup = Get-DriverPack
    if (-not $DryRun -and -not (Test-TrustedVBAudioSignature $setup)) {
        throw "$setup is not signed by VB-Audio; refusing to run it"
    }
    Add-VBAudioTrustedPublisher (Split-Path $setup)
    $code = Invoke-Setup $setup @('-i', '-h')
    Write-VtLog "VB-Cable setup exited with $code"
    Save-Marker $true
    return 3010
}

function Invoke-Uninstall {
    if (-not (Test-Marker)) { Write-VtLog 'VB-Cable was not installed by Voice Tuner; leaving it.'; return 0 }
    $setup = Join-Path $PackDir $SetupName
    if (-not (Test-Path $setup)) { $setup = Get-DriverPack }
    $code = Invoke-Setup $setup @('-u', '-h')
    Write-VtLog "VB-Cable removal exited with $code"
    Save-Marker $false
    return 3010
}

if ($MyInvocation.InvocationName -ne '.') {
    try {
        switch ($Action) {
            'Detect' { if (Test-VBCableInstalled) { exit 0 } else { exit 2 } }
            'Install' { exit (Invoke-Install) }
            'Uninstall' { exit (Invoke-Uninstall) }
        }
    }
    catch {
        Write-VtLog "ERROR: $_"
        exit 1
    }
}
