# Pester 5 tests for install-vbcable.ps1 (run in CI on windows-latest).
BeforeAll {
    $script = Join-Path $PSScriptRoot '..\resources\install-vbcable.ps1'
    # Dot-source to load the functions without running the entry point.
    . $script -Action Detect -RegistryKey 'HKCU:\Software\TunedUpTest' -PackDir (Join-Path $TestDrive 'pack')
}

Describe 'Test-VBCableInstalled' {
    It 'is true when a VB-Audio PnP device exists' {
        Mock Get-PnpDevice { [pscustomobject]@{ FriendlyName = 'CABLE Output (VB-Audio Virtual Cable)'; Status = 'OK' } }
        Test-VBCableInstalled | Should -BeTrue
    }
    It 'is false with no device and no endpoint' {
        Mock Get-PnpDevice { @() }
        Mock Test-Path { $false } -ParameterFilter { $Path -like '*MMDevices*' }
        Test-VBCableInstalled | Should -BeFalse
    }
}

Describe 'Get-RegValue' {
    It 'returns $null for a missing value under StrictMode' {
        New-Item -Path 'HKCU:\Software\TunedUpTest\RegValue' -Force | Out-Null
        Get-RegValue 'HKCU:\Software\TunedUpTest\RegValue' 'Nope' | Should -BeNullOrEmpty
        Remove-Item -Path 'HKCU:\Software\TunedUpTest\RegValue' -Recurse -Force
    }
}

Describe 'Test-VBCableActive (ADR 0011)' {
    It 'needs both sides active' {
        Mock Get-VBCableEndpointState { @(
                [pscustomobject]@{ Flow = 'Render'; State = 1 },
                [pscustomobject]@{ Flow = 'Capture'; State = 1 }
            ) }
        Test-VBCableActive | Should -BeTrue
    }
    It 'is false when one side is disabled or unplugged' {
        Mock Get-VBCableEndpointState { @(
                [pscustomobject]@{ Flow = 'Render'; State = 1 },
                [pscustomobject]@{ Flow = 'Capture'; State = 2 }
            ) }
        Test-VBCableActive | Should -BeFalse
    }
    It 'is false with no endpoints' {
        Mock Get-VBCableEndpointState { }
        Test-VBCableActive | Should -BeFalse
    }
}

Describe 'Invoke-Install (FR-15, ADR 0011)' {
    BeforeEach {
        $script:DryRun = $true
        Mock Test-VBCableInstalled { $false }
        Mock Get-DriverPack { 'C:\pack\VBCABLE_Setup_x64.exe' }
        Mock Test-TrustedVBAudioSignature { $true }
        Mock Add-VBAudioTrustedPublisher { }
        Mock Invoke-Setup { 0 }
        Mock Save-Marker { }
        Mock Invoke-AudioServiceRestart { }
        Mock Invoke-App { }
    }
    It 'does nothing when VB-Cable is already installed' {
        Mock Test-VBCableInstalled { $true }
        Invoke-Install | Should -Be 0
        Should -Invoke Invoke-Setup -Times 0
    }
    It 'needs no restart when the cable comes up right away' {
        Mock Test-VBCableActive { $true }
        Invoke-Install | Should -Be 0
        Should -Invoke Invoke-Setup -Times 1 -ParameterFilter { $Arguments -contains '-i' -and $Arguments -contains '-h' }
        Should -Invoke Save-Marker -Times 1 -ParameterFilter { $Installed -eq $true }
        Should -Invoke Invoke-AudioServiceRestart -Times 0
    }
    It 'saves the default devices before and restores them after' {
        Mock Test-VBCableActive { $true }
        Invoke-Install | Out-Null
        Should -Invoke Invoke-App -Times 1 -ParameterFilter { $Verb -eq 'snapshot' }
        Should -Invoke Invoke-App -Times 1 -ParameterFilter { $Verb -eq 'settle' }
    }
    It 'restarts the audio services, then asks for a reboot if the cable is still missing' {
        Mock Test-VBCableActive { $false }
        Invoke-Install | Should -Be 3010
        Should -Invoke Invoke-AudioServiceRestart -Times 1
        Should -Invoke Save-Marker -Times 1 -ParameterFilter { $Installed -eq $true }
    }
    It 'fails without recording the marker when the driver did not install' {
        $script:DryRun = $false
        Mock Invoke-Setup { 1 }
        { Invoke-Install } | Should -Throw '*not installed*'
        Should -Invoke Save-Marker -Times 0
    }
}

Describe 'Invoke-Repair (ADR 0011)' {
    BeforeEach {
        $script:DryRun = $true
        Mock Invoke-AudioServiceRestart { }
        Mock Install-FromPack { }
        Mock Save-Marker { }
        Mock Invoke-App { }
    }
    It 'installs VB-Cable when it is missing' {
        Mock Test-VBCableInstalled { $false }
        Mock Test-VBCableActive { $true }
        Invoke-Repair | Should -Be 0
        Should -Invoke Install-FromPack -Times 1
        Should -Invoke Save-Marker -Times 1 -ParameterFilter { $Installed -eq $true }
    }
    It 'does nothing when the cable already works' {
        Mock Test-VBCableInstalled { $true }
        Mock Test-VBCableActive { $true }
        Invoke-Repair | Should -Be 0
        Should -Invoke Invoke-AudioServiceRestart -Times 0
        Should -Invoke Install-FromPack -Times 0
    }
    It 'restarts the audio services first' {
        Mock Test-VBCableInstalled { $true }
        $script:calls = 0
        Mock Test-VBCableActive { $script:calls++; $script:calls -gt 1 }
        Invoke-Repair | Should -Be 0
        Should -Invoke Invoke-AudioServiceRestart -Times 1
        Should -Invoke Install-FromPack -Times 0
    }
    It 'reinstalls over the top, and asks for a reboot if that is not enough' {
        Mock Test-VBCableInstalled { $true }
        Mock Test-VBCableActive { $false }
        Invoke-Repair | Should -Be 3010
        Should -Invoke Install-FromPack -Times 1
        Should -Invoke Save-Marker -Times 0
    }
}

Describe 'Exit codes are clean integers' {
    It 'logging does not leak into return values' {
        Mock Test-VBCableInstalled { $true }
        $r = Invoke-Install
        $r | Should -BeOfType [int]
        @($r).Count | Should -Be 1
    }
    It 'holds for a fresh install too' {
        $script:DryRun = $true
        Mock Test-VBCableInstalled { $false }
        Mock Install-FromPack { }
        Mock Save-Marker { }
        Mock Invoke-App { }
        Mock Invoke-AudioServiceRestart { }
        Mock Test-VBCableActive { $false }
        $r = Invoke-Install
        $r | Should -BeOfType [int]
        @($r).Count | Should -Be 1
    }
}

Describe 'Invoke-Uninstall' {
    BeforeEach {
        $script:DryRun = $true
        Mock Invoke-AudioServiceRestart { }
    }
    It 'leaves VB-Cable alone when another installer put it there' {
        Mock Test-Marker { $false }
        Mock Invoke-Setup { 0 }
        Invoke-Uninstall | Should -Be 0
        Should -Invoke Invoke-Setup -Times 0
    }
    It 'removes VB-Cable without a reboot once its endpoints are gone' {
        Mock Test-Marker { $true }
        Mock Test-Path { $true }
        Mock Invoke-Setup { 0 }
        Mock Save-Marker { }
        Mock Test-VBCableActive { $false }
        Invoke-Uninstall | Should -Be 0
        Should -Invoke Invoke-Setup -Times 1 -ParameterFilter { $Arguments -contains '-u' }
        Should -Invoke Invoke-AudioServiceRestart -Times 0
    }
    It 'asks for a reboot when the endpoints stay' {
        Mock Test-Marker { $true }
        Mock Test-Path { $true }
        Mock Invoke-Setup { 0 }
        Mock Save-Marker { }
        Mock Test-VBCableActive { $true }
        Invoke-Uninstall | Should -Be 3010
        Should -Invoke Invoke-AudioServiceRestart -Times 1
    }
}
