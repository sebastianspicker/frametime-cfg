BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "Get-PhaseGpuInput" {

    It "returns null when gpuInput is missing" {
        $state = [PSCustomObject]@{ mode = "CONTROL" }

        Get-PhaseGpuInput -State $state | Should -BeNullOrEmpty
    }

    It "returns null when the state document contains JSON null" {
        Get-PhaseGpuInput -State $null | Should -BeNullOrEmpty
    }

    It "returns null for unsupported gpuInput values" {
        foreach ($value in @("0", "NVIDIA", "5")) {
            $state = [PSCustomObject]@{ gpuInput = $value }

            Get-PhaseGpuInput -State $state | Should -BeNullOrEmpty
        }
    }

    It "returns null for a compound gpuInput value" {
        $state = [PSCustomObject]@{ gpuInput = @("1", "2") }

        Get-PhaseGpuInput -State $state | Should -BeNullOrEmpty
    }

    It "accepts each valid scalar GPU branch" {
        foreach ($value in @("1", 2, "3", 4)) {
            $state = [PSCustomObject]@{ gpuInput = $value }

            Get-PhaseGpuInput -State $state | Should -Be ([string]$value)
        }
    }
}
