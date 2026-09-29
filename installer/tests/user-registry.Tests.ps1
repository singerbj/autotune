# Pester 5 tests for user-registry.ps1 (run in CI on windows-latest).
BeforeAll {
    $script:Script = Join-Path $PSScriptRoot '..\resources\user-registry.ps1'
    $script:Sid = 'S-1-5-21-1-2-3-1001'
    $script:Root = 'TestRegistry:\Users'
    $script:Hive = Join-Path $Root $Sid
    $script:Cv = Join-Path $Hive 'Software\Microsoft\Windows\CurrentVersion'
    . $Script -Action Cleanup -UsersRoot $Root -Sid $Sid

    # Runs the script as NSIS does, in a child process, and returns its exit code.
    function Invoke-UserRegistry([string[]] $Arguments) {
        $root = (Get-Item $Root).PSPath
        & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $Script @Arguments -UsersRoot $root -Sid $Sid | Out-Null
        return $LASTEXITCODE
    }
}

Describe 'SetRunOnce' {
    BeforeEach {
        Remove-Item $Root -Recurse -Force -ErrorAction SilentlyContinue
        New-Item $Hive -Force | Out-Null
    }
    It 'writes RunOnce in the signed-in user''s hive' {
        Invoke-UserRegistry @('-Action', 'SetRunOnce', '-Exe', 'C:\Program Files\TunedUp\tunedup.exe') | Should -Be 0
        (Get-ItemProperty (Join-Path $Cv 'RunOnce')).TunedUpSetup | Should -Be '"C:\Program Files\TunedUp\tunedup.exe" --first-run'
    }
    It 'fails (so NSIS falls back to HKCU) when the hive is not loaded' {
        Remove-Item $Hive -Recurse -Force
        Invoke-UserRegistry @('-Action', 'SetRunOnce', '-Exe', 'x') | Should -Be 1
    }
    It 'fails without -Exe' {
        Invoke-UserRegistry @('-Action', 'SetRunOnce') | Should -Be 1
    }
}

Describe 'Cleanup' {
    BeforeEach {
        Remove-Item $Root -Recurse -Force -ErrorAction SilentlyContinue
        foreach ($k in 'RunOnce', 'Run', 'Explorer\StartupApproved\Run') { New-Item (Join-Path $Cv $k) -Force | Out-Null }
        Set-ItemProperty -Path (Join-Path $Cv 'RunOnce') -Name TunedUpSetup -Value 'x'
        Set-ItemProperty -Path (Join-Path $Cv 'Run') -Name TunedUp -Value 'x'
        Set-ItemProperty -Path (Join-Path $Cv 'Run') -Name OtherApp -Value 'y'
        Set-ItemProperty -Path (Join-Path $Cv 'Explorer\StartupApproved\Run') -Name TunedUp -Value ([byte[]](2, 0, 0, 0))
    }
    It 'removes TunedUp''s startup entries and nothing else' {
        Invoke-UserRegistry @('-Action', 'Cleanup') | Should -Be 0
        (Get-ItemProperty (Join-Path $Cv 'RunOnce')).PSObject.Properties.Name | Should -Not -Contain 'TunedUpSetup'
        (Get-ItemProperty (Join-Path $Cv 'Run')).PSObject.Properties.Name | Should -Not -Contain 'TunedUp'
        (Get-ItemProperty (Join-Path $Cv 'Run')).OtherApp | Should -Be 'y'
        (Get-ItemProperty (Join-Path $Cv 'Explorer\StartupApproved\Run')).PSObject.Properties.Name | Should -Not -Contain 'TunedUp'
    }
    It 'succeeds when there is nothing to remove' {
        Invoke-UserRegistry @('-Action', 'Cleanup') | Should -Be 0
        Invoke-UserRegistry @('-Action', 'Cleanup') | Should -Be 0
    }
}

Describe 'Get-SessionUserSid' {
    It 'is the owner of explorer.exe in this session' {
        Mock Get-CimInstance { [pscustomobject]@{ Name = 'explorer.exe' } } -ParameterFilter { $ClassName -eq 'Win32_Process' }
        Mock Invoke-CimMethod { [pscustomobject]@{ ReturnValue = 0; Sid = 'S-1-5-21-9-9-9-1001' } }
        Get-SessionUserSid | Should -Be 'S-1-5-21-9-9-9-1001'
    }
    It 'resolves to a real SID on this machine (when anyone is signed in)' {
        $found = Get-SessionUserSid
        if ($found) { $found | Should -Match '^S-1-5-' }
    }
}
