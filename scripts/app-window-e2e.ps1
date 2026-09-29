<#
.SYNOPSIS
  CI: starts the installed app and checks that its window opens with a
  WebView2 profile, settings and logs in the signed-in user's folders.

.DESCRIPTION
  WebView2 shows "Microsoft Edge can't read and write to its data directory"
  when its profile folder isn't writable by the account it runs as.

  -AsOtherAdmin  starts TunedUp elevated as a second admin account in this
                 desktop session, as Windows 11's Administrator Protection
                 does with its hidden admin account (and as over-the-shoulder
                 UAC does). The app must then use the signed-in user's
                 %APPDATA% / %LOCALAPPDATA%: that's where the uninstaller's
                 --restore-defaults looks, and WebView2 may drop elevation to
                 that user, who can't write the other account's profile.
#>
[CmdletBinding()]
param(
    [string] $Exe = 'C:\Program Files\TunedUp\tunedup.exe',
    [switch] $AsOtherAdmin,
    [string] $Screenshot = 'app-window.png'
)
$ErrorActionPreference = 'Stop'
$Identifier = 'io.github.singerbj.tunedup'

Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class TopWindows {
    delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr l);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    public static List<string> List() {
        var all = new List<string>();
        EnumWindows((h, l) => {
            if (!IsWindowVisible(h)) return true;
            var s = new StringBuilder(512);
            GetWindowText(h, s, s.Capacity);
            if (s.Length == 0) return true;
            uint pid; GetWindowThreadProcessId(h, out pid);
            all.Add(pid + "\t" + s);
            return true;
        }, IntPtr.Zero);
        return all;
    }
}
'@

function Get-VisibleWindow {
    [TopWindows]::List() | ForEach-Object {
        $windowPid, $title = $_ -split "`t", 2
        [pscustomobject]@{ Pid = [int]$windowPid; Title = $title }
    }
}

function Get-ProcessOwner([int] $ProcessId) {
    $p = Get-CimInstance Win32_Process -Filter "ProcessId = $ProcessId" -ErrorAction SilentlyContinue
    if ($p) {
        $o = Invoke-CimMethod -InputObject $p -MethodName GetOwner
        "$($o.Domain)\$($o.User)"
    }
}

function Close-TunedUp {
    Get-Process tunedup, msedgewebview2 -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Seconds 2
}

function Get-LocalAppData([string] $Sid) {
    $profilePath = (Get-ItemProperty "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\$Sid").ProfileImagePath
    Join-Path $profilePath 'AppData\Local'
}

$sessionUser = (Get-CimInstance Win32_ComputerSystem).UserName
if (-not $sessionUser) { $sessionUser = "$env:USERDOMAIN\$env:USERNAME" }
$sessionSid = ([Security.Principal.NTAccount] $sessionUser).Translate([Security.Principal.SecurityIdentifier]).Value
$expect = Join-Path (Get-LocalAppData $sessionSid) $Identifier
"Session $((Get-Process -Id $PID).SessionId), signed-in user $sessionUser ($sessionSid), this process $(whoami)"
Get-ItemProperty HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System |
    Select-Object EnableLUA, ConsentPromptBehaviorAdmin, PromptOnSecureDesktop | Format-List

Close-TunedUp
Remove-Item (Join-Path $expect 'EBWebView'), (Join-Path $expect 'logs') -Recurse -Force -ErrorAction SilentlyContinue

$forbid = $null
if ($AsOtherAdmin) {
    # The second account elevates without a prompt.
    Set-ItemProperty HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System -Name ConsentPromptBehaviorAdmin -Value 0
    Set-ItemProperty HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System -Name PromptOnSecureDesktop -Value 0
    $name = 'tunedup-e2e'
    $password = New-Object Security.SecureString
    foreach ($c in ('Tu-' + [guid]::NewGuid().ToString('N') + '!a1').ToCharArray()) { $password.AppendChar($c) }
    if (-not (Get-LocalUser $name -ErrorAction SilentlyContinue)) {
        New-LocalUser $name -Password $password -PasswordNeverExpires | Out-Null
        Add-LocalGroupMember -Group Administrators -Member $name
    } else {
        Set-LocalUser $name -Password $password
    }
    $cred = New-Object Management.Automation.PSCredential ".\$name", $password
    # A process as that account, which starts TunedUp elevated (RunAs), like a UAC prompt approved by it.
    $launch = "Start-Process -FilePath '$Exe' -Verb RunAs"
    Start-Process powershell.exe -Credential $cred -LoadUserProfile -WorkingDirectory $env:SystemRoot -ArgumentList '-NoProfile', '-Command', $launch
    $otherSid = (Get-LocalUser $name).SID.Value
    $deadline = (Get-Date).AddSeconds(30)
    while (-not (Test-Path "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\$otherSid") -and (Get-Date) -lt $deadline) { Start-Sleep -Seconds 1 }
    $forbid = Join-Path (Get-LocalAppData $otherSid) $Identifier
} else {
    Start-Process -FilePath $Exe
}
"Expecting the WebView2 profile and logs in $expect"

$errorPattern = 'couldn.t create the data directory|can.t read and write|TunedUp couldn.t start'
$deadline = (Get-Date).AddSeconds(90)
$errorDialog = $null
while ((Get-Date) -lt $deadline) {
    $errorDialog = Get-VisibleWindow | Where-Object Title -Match $errorPattern
    if ($errorDialog -or ((Test-Path (Join-Path $expect 'EBWebView\Local State')) -and (Get-VisibleWindow | Where-Object Title -EQ 'TunedUp'))) { break }
    Start-Sleep -Seconds 2
}
Start-Sleep -Seconds 5 # let the page render for the screenshot

'Visible windows:'
Get-VisibleWindow | ForEach-Object { "  [$($_.Pid) $(Get-ProcessOwner $_.Pid)] $($_.Title)" }
'Processes:'
Get-Process tunedup, msedgewebview2 -ErrorAction SilentlyContinue |
    Select-Object -First 4 | ForEach-Object { "  $($_.Id) $($_.ProcessName) as $(Get-ProcessOwner $_.Id)" }
try {
    $b = [Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bmp = New-Object Drawing.Bitmap $b.Width, $b.Height
    [Drawing.Graphics]::FromImage($bmp).CopyFromScreen($b.Location, [Drawing.Point]::Empty, $b.Size)
    $bmp.Save($Screenshot)
} catch { "No screenshot: $_" }

$failures = @()
if ($errorDialog) { $failures += "error dialog: $($errorDialog.Title)" }
if (-not (Get-VisibleWindow | Where-Object Title -EQ 'TunedUp')) { $failures += 'no TunedUp window' }
if (-not (Test-Path (Join-Path $expect 'EBWebView'))) { $failures += "no WebView2 profile in $expect" }
if (-not (Get-ChildItem (Join-Path $expect 'logs') -Filter 'tunedup*.log' -ErrorAction SilentlyContinue)) { $failures += "no log in $expect\logs" }
if ($forbid -and (Test-Path $forbid)) { $failures += "the app used the elevated account's $forbid" }
if ($AsOtherAdmin) {
    $app = Get-Process tunedup -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($app) { "TunedUp runs as $(Get-ProcessOwner $app.Id)" }
    # What WebView2 needs if it drops elevation: the signed-in user's own write access.
    $acl = if (Test-Path $expect) { (Get-Acl $expect).Access } else { @() }
    $rights = $acl | Where-Object {
        $_.AccessControlType -eq 'Allow' -and
        $(try { $_.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value } catch { '' }) -eq $sessionSid
    } | ForEach-Object { $_.FileSystemRights }
    "Rights of $sessionUser on ${expect}: $($rights -join ', ')"
    $modify = [Security.AccessControl.FileSystemRights]::Modify
    if (-not ($rights | Where-Object { ($_ -band $modify) -eq $modify })) {
        $failures += "$sessionUser can't write $expect"
    }
}

Close-TunedUp
if ($failures) { throw ($failures -join "`n") }
"OK: the TunedUp window opened with its WebView2 profile in $expect"
