# ==============================================================================
#  tests/helpers/storage-health.Tests.ps1
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "Parse-TrimFsutilOutput" {

    It "parses NTFS and ReFS trim states from fsutil output" {
        $states = @(Parse-TrimFsutilOutput -OutputLines @(
            "NTFS DisableDeleteNotify = 0",
            "ReFS DisableDeleteNotify = 1"
        ))

        $states.Count | Should -Be 2
        (@($states | Where-Object { $_.FileSystem -eq "NTFS" })[0]).TrimEnabled | Should -Be $true
        (@($states | Where-Object { $_.FileSystem -eq "ReFS" })[0]).TrimEnabled | Should -Be $false
    }
}

Describe "Get-TrimHealthStatus" {

    BeforeEach {
        Reset-TestState
    }

    It "reports retrimmable fixed volumes and whether any trim support is disabled" {
        Mock fsutil {
            @(
                "NTFS DisableDeleteNotify = 0",
                "ReFS DisableDeleteNotify = 1"
            )
        }
        Mock Get-Volume {
            @(
                [PSCustomObject]@{ DriveLetter = "C"; DriveType = "Fixed"; FileSystem = "NTFS" },
                [PSCustomObject]@{ DriveLetter = "D"; DriveType = "Fixed"; FileSystem = "ReFS" }
            )
        }

        $status = Get-TrimHealthStatus

        $status.AnyTrimDisabled | Should -Be $true
        $status.RetrimAvailable | Should -Be $true
        @($status.RetrimmableVolumes) | Should -Contain "C"
        $status.Summary | Should -Match "NTFS: enabled"
    }
}

Describe "Enable-TrimSupport" {

    BeforeEach {
        Reset-TestState
    }

    It "enables trim through fsutil and reports success" {
        Mock fsutil {
            $global:LASTEXITCODE = 0
            @("NTFS DisableDeleteNotify = 0")
        }

        $result = Enable-TrimSupport

        $result.Success | Should -Be $true
        Should -Invoke fsutil -Exactly 1
    }
}

Describe "Invoke-StorageRetrim" {

    It "calls Optimize-Volume with ReTrim for the selected drive" {
        Mock Optimize-Volume {}

        Invoke-StorageRetrim -DriveLetter C

        Should -Invoke Optimize-Volume -Exactly 1 -ParameterFilter { $DriveLetter -eq "C" -and $ReTrim }
    }
}
