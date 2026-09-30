<#
.SYNOPSIS
  CI (ci.yml, install-e2e): installs the release installer silently the way
  the updater and scripted installs do, checks the result, exercises the
  uninstall helper, then uninstalls and checks cleanup.

.DESCRIPTION
  -Phase Install    install and check (leave it installed for app-window-e2e.ps1)
  -Phase Uninstall  check the uninstall helper, uninstall and check cleanup
  -Phase All        both (the default)

  VB-Cable is skipped (TUNEDUP_SKIP_VBCABLE=1): CI runners have no audio stack.
#>
[CmdletBinding()]
param(
    [string] $Installer = 'tunedup-setup.exe',
    [string] $InstallDir = 'C:\Program Files\TunedUp',
    [ValidateSet('All', 'Install', 'Uninstall')] [string] $Phase = 'All'
)
$ErrorActionPreference = 'Stop'
$env:TUNEDUP_SKIP_VBCABLE = '1'
$exe = Join-Path $InstallDir 'tunedup.exe'

if ($Phase -in 'All', 'Install') {
    if (-not (Test-Path $Installer)) { throw "No installer at $Installer" }
    Write-Output "Installing $Installer"
    $p = Start-Process -FilePath $Installer -ArgumentList '/S' -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "Installer exited with $($p.ExitCode)" }

    foreach ($path in @($exe, (Join-Path $InstallDir 'uninstall.exe'), (Join-Path $InstallDir 'installer\install-vbcable.ps1'), (Join-Path $InstallDir 'installer\user-registry.ps1'))) {
        if (-not (Test-Path $path)) { throw "Missing after install: $path" }
    }
    $uninstallKey = Get-ChildItem 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall', 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall' -ErrorAction SilentlyContinue |
        Where-Object { (Get-ItemProperty $_.PSPath).DisplayName -eq 'TunedUp' }
    if (-not $uninstallKey) { throw 'Add/Remove Programs entry missing' }
    Write-Output 'Install OK'
}

if ($Phase -in 'All', 'Uninstall') {
    Get-Process tunedup -ErrorAction SilentlyContinue | Stop-Process -Force

    # The uninstaller's restore path must work headless and exit 0.
    $r = Start-Process -FilePath $exe -ArgumentList '--restore-defaults' -Wait -PassThru
    if ($r.ExitCode -ne 0) { throw "--restore-defaults exited with $($r.ExitCode)" }
    Write-Output 'Restore-defaults helper OK'

    $u = Start-Process -FilePath (Join-Path $InstallDir 'uninstall.exe') -ArgumentList '/S', "_?=$InstallDir" -Wait -PassThru
    if ($u.ExitCode -ne 0) { throw "Uninstaller exited with $($u.ExitCode)" }
    Remove-Item (Join-Path $InstallDir 'uninstall.exe') -Force -ErrorAction SilentlyContinue
    if (Test-Path $exe) { throw 'tunedup.exe still present after uninstall' }
    if (Test-Path (Join-Path $InstallDir 'installer')) { throw 'installer helper folder not cleaned up' }
    Write-Output 'Uninstall OK'
}
