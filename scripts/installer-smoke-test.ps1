<#
.SYNOPSIS
  Installs the freshly built NSIS installer silently, checks the result,
  exercises the uninstall helper, then uninstalls and checks cleanup.
  VB-Cable is skipped (TUNEDUP_SKIP_VBCABLE=1): CI runners have no audio stack.
#>
[CmdletBinding()]
param(
    [string] $BundleDir = 'target/release/bundle/nsis',
    [string] $InstallDir = 'C:\Program Files\TunedUp'
)
$ErrorActionPreference = 'Stop'
$env:TUNEDUP_SKIP_VBCABLE = '1'

$installer = Get-ChildItem -Path $BundleDir -Filter '*-setup.exe' | Select-Object -First 1
if (-not $installer) { throw "No installer found in $BundleDir" }
Write-Output "Installing $($installer.Name)"
$p = Start-Process -FilePath $installer.FullName -ArgumentList '/S' -Wait -PassThru
if ($p.ExitCode -ne 0) { throw "Installer exited with $($p.ExitCode)" }

$exe = Join-Path $InstallDir 'tunedup.exe'
foreach ($path in @($exe, (Join-Path $InstallDir 'uninstall.exe'), (Join-Path $InstallDir 'installer\install-vbcable.ps1'))) {
    if (-not (Test-Path $path)) { throw "Missing after install: $path" }
}
$uninstallKey = Get-ChildItem 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall', 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall' -ErrorAction SilentlyContinue |
    Where-Object { (Get-ItemProperty $_.PSPath).DisplayName -eq 'TunedUp' }
if (-not $uninstallKey) { throw 'Add/Remove Programs entry missing' }
Write-Output 'Install OK'

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
