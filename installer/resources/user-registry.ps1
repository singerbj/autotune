<#
.SYNOPSIS
  Writes TunedUp's per-user startup entries into the signed-in user's
  registry hive, from the elevated installer.

.DESCRIPTION
  Called by the NSIS installer hooks (installer/nsis/hooks.nsh), elevated.

  The installer's HKCU is the elevated account's. With plain UAC that is the
  signed-in user, but under Windows 11 Administrator Protection elevated
  programs run as a hidden admin account, and when another admin approves the
  UAC prompt they run as that admin. A RunOnce entry written there never runs
  for the user, and an autostart entry left there is never removed. This
  script finds the user signed in to the installer's Windows session (the
  owner of its explorer.exe) and uses HKEY_USERS\<their SID>.

  SetRunOnce: RunOnce\TunedUpSetup = "<-Exe>" --first-run (reopens the app
              in the setup wizard after the VB-Cable reboot).
  Cleanup:    removes RunOnce\TunedUpSetup and the autostart entry
              (Run\TunedUp and its StartupApproved flag).

  Exit codes: 0 = done, 1 = failed (no signed-in user or hive not loaded;
  the caller falls back to HKCU).

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File user-registry.ps1 -Action SetRunOnce -Exe 'C:\Program Files\TunedUp\tunedup.exe'
#>
[Diagnostics.CodeAnalysis.SuppressMessageAttribute('PSReviewUnusedParameter', '', Justification = 'Used by functions in this script')]
[CmdletBinding()]
param(
    [ValidateSet('SetRunOnce', 'Cleanup')]
    [string] $Action = 'Cleanup',
    [string] $Exe,
    # Tests point these at a scratch key and a fake SID.
    [string] $UsersRoot = 'Registry::HKEY_USERS',
    [string] $Sid
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$RunOnceName = 'TunedUpSetup'
# Tauri's autostart plugin names the value after the product.
$AutostartName = 'TunedUp'
$CurrentVersion = 'Software\Microsoft\Windows\CurrentVersion'

function Get-SessionUserSid {
    $session = (Get-Process -Id $PID).SessionId
    $explorer = Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe' AND SessionId = $session" -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($explorer) {
        $owner = Invoke-CimMethod -InputObject $explorer -MethodName GetOwnerSid
        if ($owner.ReturnValue -eq 0 -and $owner.Sid) { return $owner.Sid }
    }
    # No shell in this session (e.g. a remote script): the console user, if any.
    $name = (Get-CimInstance Win32_ComputerSystem).UserName
    if ($name) {
        return ([Security.Principal.NTAccount] $name).Translate([Security.Principal.SecurityIdentifier]).Value
    }
    return $null
}

function Get-UserHive {
    $userSid = if ($Sid) { $Sid } else { Get-SessionUserSid }
    if (-not $userSid) { throw 'No user is signed in to this Windows session.' }
    $hive = Join-Path $UsersRoot $userSid
    # The hive is loaded while the user is signed in.
    if (-not (Test-Path $hive)) { throw "The registry hive of $userSid is not loaded." }
    return $hive
}

function Write-UserRunOnce([string] $Hive, [string] $Value) {
    $key = Join-Path $Hive "$CurrentVersion\RunOnce"
    if (-not (Test-Path $key)) { New-Item -Path $key -Force | Out-Null }
    Set-ItemProperty -Path $key -Name $RunOnceName -Value $Value
}

function Clear-UserStartupEntry([string] $Hive) {
    foreach ($entry in @(
            @{ Key = "$CurrentVersion\RunOnce"; Name = $RunOnceName },
            @{ Key = "$CurrentVersion\Run"; Name = $AutostartName },
            @{ Key = "$CurrentVersion\Explorer\StartupApproved\Run"; Name = $AutostartName })) {
        Remove-ItemProperty -Path (Join-Path $Hive $entry.Key) -Name $entry.Name -ErrorAction SilentlyContinue
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    try {
        $hive = Get-UserHive
        switch ($Action) {
            'SetRunOnce' {
                if (-not $Exe) { throw '-Exe is required.' }
                Write-UserRunOnce $hive "`"$Exe`" --first-run"
            }
            'Cleanup' { Clear-UserStartupEntry $hive }
        }
        exit 0
    }
    catch {
        Write-Host "user-registry: $_"
        exit 1
    }
}
