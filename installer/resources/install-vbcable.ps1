<#
.SYNOPSIS
  Installs, repairs or removes the VB-Audio Virtual Cable driver for TunedUp
  (FR-15, ADR 0012).

.DESCRIPTION
  Called elevated by the NSIS installer hooks (installer/nsis/hooks.nsh) and
  by the app's "Repair" button (repair_virtual_mic).

  Install:
    1. Exit 0 if VB-Cable is already present.
    2. Use the driver pack in -PackDir (bundled when licensed), otherwise
       download it from VB-Audio (link-out install) and keep a copy in
       -PackDir so the uninstaller can remove the driver later.
    3. Verify the Authenticode signatures of the setup program.
    4. Trust VB-Audio's publisher certificate (TrustedPublisher) so the driver
       install shows no Windows Security prompt.
    5. Save the default audio devices (-App: tunedup.exe --cable snapshot).
    6. Run VBCABLE_Setup_x64.exe -i -h silently; fail (1) if the driver
       isn't there afterwards.
    7. Record InstalledVBCable=1 under -RegistryKey.
    8. Wait for both cable endpoints to become active, restarting the
       Windows Audio services once if they don't.
    9. Put back defaults Windows moved onto the cable (--cable settle).
   10. Exit 0 when the cable works now, 3010 when Windows needs a restart.

  Repair:
    Install if missing; otherwise restart the Windows Audio services and, if
    the cable still isn't active, run the setup program again.

  Uninstall:
    Only if -RegistryKey says we installed it: run VBCABLE_Setup_x64.exe -u -h,
    then wait for the endpoints to go away (restarting the audio services
    once if needed). The uninstaller moves defaults off the cable first.

  Exit codes: 0 = done, nothing more needed; 3010 = done, restart Windows;
  1 = failed.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File install-vbcable.ps1 -Action Install -PackDir C:\x -DryRun
#>
# Script parameters are read inside the functions below; PSScriptAnalyzer
# cannot see that across scopes.
[Diagnostics.CodeAnalysis.SuppressMessageAttribute('PSReviewUnusedParameter', '', Justification = 'Used by functions in this script')]
[CmdletBinding()]
param(
    [ValidateSet('Install', 'Repair', 'Uninstall', 'Detect')]
    [string] $Action = 'Install',
    [string] $PackDir = (Join-Path $PSScriptRoot 'vbcable'),
    [string] $RegistryKey = 'HKLM:\Software\TunedUp',
    [string] $DownloadUrl = 'https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip',
    # tunedup.exe, for saving and restoring the default devices (optional).
    [string] $App = '',
    # How long to wait for the cable endpoints to appear or go away.
    [int] $WaitSeconds = 20,
    [switch] $DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$SetupName = 'VBCABLE_Setup_x64.exe'
$LogFile = Join-Path $env:TEMP 'tunedup-vbcable.log'

function Write-VtLog([string] $Message) {
    $line = '{0:u} {1}' -f (Get-Date), $Message
    # Host stream, not the output stream: functions here return exit codes,
    # and anything written to the output stream would become part of them.
    Write-Host $line
    try { Add-Content -Path $LogFile -Value $line -ErrorAction Stop }
    catch { Write-Verbose "could not write ${LogFile}: $_" }
}

function Get-RegValue([string] $Path, [string] $Name) {
    # A registry value or $null; safe under StrictMode for missing values.
    $item = Get-ItemProperty -Path $Path -ErrorAction SilentlyContinue
    if (-not $item) { return $null }
    $prop = $item.PSObject.Properties[$Name]
    if ($prop) { return $prop.Value }
    return $null
}

function Test-VBCableInstalled {
    # Endpoint names are the most reliable signal: "CABLE Input"/"CABLE Output (VB-Audio Virtual Cable)".
    $devices = @(Get-PnpDevice -FriendlyName '*VB-Audio Virtual Cable*' -ErrorAction SilentlyContinue |
        Where-Object { $_.Status -ne 'Unknown' })
    if ($devices.Count -gt 0) { return $true }
    $mmdev = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render'
    if (Test-Path $mmdev) {
        foreach ($ep in Get-ChildItem $mmdev -ErrorAction SilentlyContinue) {
            $name = Get-RegValue (Join-Path $ep.PSPath 'Properties') '{a45c254e-df1c-4efd-8020-67d146a850e0},2'
            if ($name -like 'CABLE Input*') { return $true }
        }
    }
    return $false
}

function Get-VBCableEndpointState {
    # One object per VB-Cable endpoint: its flow and DeviceState (1 = active,
    # 2 = disabled, 4 = not present, 8 = unplugged), from the MMDevices registry.
    $root = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio'
    foreach ($flow in 'Render', 'Capture') {
        $dir = Join-Path $root $flow
        if (-not (Test-Path $dir)) { continue }
        foreach ($ep in Get-ChildItem $dir -ErrorAction SilentlyContinue) {
            $props = Join-Path $ep.PSPath 'Properties'
            $name = [string](Get-RegValue $props '{a45c254e-df1c-4efd-8020-67d146a850e0},2')
            $adapter = [string](Get-RegValue $props '{b3f8fa53-0004-438e-9003-51a46e139bfc},6')
            if ($name -notlike 'CABLE Input*' -and $name -notlike 'CABLE Output*' -and $adapter -notlike '*VB-Audio Virtual Cable*') { continue }
            $state = Get-RegValue $ep.PSPath 'DeviceState'
            if ($null -eq $state) { continue }
            [pscustomobject]@{ Flow = $flow; State = ([int64]$state -band 0xF) }
        }
    }
}

function Test-VBCableActive {
    # Both sides of the cable are usable: apps can play into CABLE Input and
    # record from CABLE Output.
    $states = @(Get-VBCableEndpointState)
    $render = @($states | Where-Object { $_.Flow -eq 'Render' -and $_.State -eq 1 }).Count -gt 0
    $capture = @($states | Where-Object { $_.Flow -eq 'Capture' -and $_.State -eq 1 }).Count -gt 0
    return ($render -and $capture)
}

function Wait-VBCable([bool] $Active, [int] $Seconds) {
    # Wait until the cable is (or is no longer) active; $true if it got there.
    $deadline = (Get-Date).AddSeconds($Seconds)
    while ($true) {
        if ((Test-VBCableActive) -eq $Active) { return $true }
        if ($DryRun -or (Get-Date) -ge $deadline) { return $false }
        Start-Sleep -Seconds 1
    }
}

function Invoke-AudioServiceRestart {
    # Makes Windows pick up added or removed audio endpoints without a reboot.
    # Other apps' sound stops for a second or two.
    Write-VtLog 'Restarting the Windows Audio services'
    if ($DryRun) { return }
    try {
        Restart-Service -Name 'AudioEndpointBuilder' -Force -ErrorAction Stop
        Start-Service -Name 'Audiosrv' -ErrorAction Stop
    }
    catch { Write-VtLog "Restarting the audio services failed: $_" }
}

function Complete-CableChange([bool] $Active) {
    # Wait for the endpoints, then try once more after restarting the audio
    # services. $false means Windows needs a restart.
    if (Wait-VBCable -Active $Active -Seconds $WaitSeconds) { return $true }
    Invoke-AudioServiceRestart
    return (Wait-VBCable -Active $Active -Seconds $WaitSeconds)
}

function Invoke-App([string] $Verb) {
    # tunedup.exe --cable <verb>: saves or restores the default devices.
    # Best effort: a failure here never fails the driver install.
    if (-not $App -or -not (Test-Path $App)) { return }
    Write-VtLog "Running $App --cable $Verb"
    if ($DryRun) { return }
    try {
        $p = Start-Process -FilePath $App -ArgumentList '--cable', $Verb -Wait -PassThru -WindowStyle Hidden
        Write-VtLog "tunedup --cable $Verb exited with $($p.ExitCode)"
    }
    catch { Write-VtLog "tunedup --cable $Verb failed: $_" }
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

function Install-FromPack {
    # Verify, trust and run VB-Audio's setup program; throws if the driver
    # isn't installed afterwards.
    $setup = Get-DriverPack
    if (-not $DryRun -and -not (Test-TrustedVBAudioSignature $setup)) {
        throw "$setup is not signed by VB-Audio; refusing to run it"
    }
    Add-VBAudioTrustedPublisher (Split-Path $setup)
    $code = Invoke-Setup $setup @('-i', '-h')
    Write-VtLog "VB-Cable setup exited with $code"
    if (-not $DryRun -and -not (Test-VBCableInstalled)) {
        throw "VB-Cable setup exited with $code and the driver is not installed"
    }
}

function Invoke-Install {
    if (Test-VBCableInstalled) { Write-VtLog 'VB-Cable already installed.'; return 0 }
    Invoke-App 'snapshot'
    Install-FromPack
    Save-Marker $true
    $ready = Complete-CableChange -Active $true
    Invoke-App 'settle'
    if ($ready) { Write-VtLog 'VB-Cable is ready; no restart needed.'; return 0 }
    Write-VtLog 'VB-Cable needs a Windows restart to finish.'
    return 3010
}

function Invoke-Repair {
    if (-not (Test-VBCableInstalled)) { return (Invoke-Install) }
    if (Test-VBCableActive) { Write-VtLog 'VB-Cable is active; nothing to repair.'; return 0 }
    Invoke-AudioServiceRestart
    if (Wait-VBCable -Active $true -Seconds $WaitSeconds) { return 0 }
    Write-VtLog 'Still not active; running the VB-Cable setup again.'
    Install-FromPack
    if (Complete-CableChange -Active $true) { return 0 }
    return 3010
}

function Invoke-Uninstall {
    if (-not (Test-Marker)) { Write-VtLog 'VB-Cable was not installed by TunedUp; leaving it.'; return 0 }
    $setup = Join-Path $PackDir $SetupName
    if (-not (Test-Path $setup)) { $setup = Get-DriverPack }
    $code = Invoke-Setup $setup @('-u', '-h')
    Write-VtLog "VB-Cable removal exited with $code"
    Save-Marker $false
    if (Complete-CableChange -Active $false) { Write-VtLog 'VB-Cable removed; no restart needed.'; return 0 }
    Write-VtLog 'VB-Cable removal needs a Windows restart to finish.'
    return 3010
}

if ($MyInvocation.InvocationName -ne '.') {
    try {
        switch ($Action) {
            'Detect' { if (Test-VBCableInstalled) { exit 0 } else { exit 2 } }
            'Install' { exit (Invoke-Install) }
            'Repair' { exit (Invoke-Repair) }
            'Uninstall' { exit (Invoke-Uninstall) }
        }
    }
    catch {
        Write-VtLog "ERROR: $_"
        exit 1
    }
}
