# Pester 5 tests for install-vbcable.ps1 (run in CI on windows-latest).
BeforeAll {
    $script = Join-Path $PSScriptRoot '..\resources\install-vbcable.ps1'
    # Dot-source to load the functions without running the entry point.
    . $script -Action Detect -RegistryKey 'HKCU:\Software\VoiceTunerTest' -PackDir (Join-Path $TestDrive 'pack')
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

Describe 'Invoke-Install (FR-15)' {
    BeforeEach { $script:DryRun = $true }
    It 'does nothing when VB-Cable is already installed' {
        Mock Test-VBCableInstalled { $true }
        Invoke-Install | Should -Be 0
    }
    It 'installs silently and requests a reboot when missing' {
        Mock Test-VBCableInstalled { $false }
        Mock Get-DriverPack { 'C:\pack\VBCABLE_Setup_x64.exe' }
        Mock Add-VBAudioTrustedPublisher { }
        Mock Invoke-Setup { 0 }
        Mock Save-Marker { }
        Invoke-Install | Should -Be 3010
        Should -Invoke Invoke-Setup -Times 1 -ParameterFilter { $Arguments -contains '-i' -and $Arguments -contains '-h' }
        Should -Invoke Save-Marker -Times 1 -ParameterFilter { $Installed -eq $true }
    }
}

Describe 'Invoke-Uninstall' {
    BeforeEach { $script:DryRun = $true }
    It 'leaves VB-Cable alone when another installer put it there' {
        Mock Test-Marker { $false }
        Mock Invoke-Setup { 0 }
        Invoke-Uninstall | Should -Be 0
        Should -Invoke Invoke-Setup -Times 0
    }
    It 'removes VB-Cable when Voice Tuner installed it' {
        Mock Test-Marker { $true }
        Mock Test-Path { $true }
        Mock Invoke-Setup { 0 }
        Mock Save-Marker { }
        Invoke-Uninstall | Should -Be 3010
        Should -Invoke Invoke-Setup -Times 1 -ParameterFilter { $Arguments -contains '-u' }
    }
}
