# ==============================================================================
#  tests/helpers/logging-security.Tests.ps1  --  Logging redaction coverage
# ==============================================================================

BeforeAll {
    . "$PSScriptRoot/_TestInit.ps1"
}

AfterAll {
    if ($SCRIPT:TestTempRoot -and (Test-Path $SCRIPT:TestTempRoot)) {
        Remove-Item $SCRIPT:TestTempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Describe "Redact-Sensitive" {

    BeforeEach { Reset-TestState }

    It "redacts username, computer name, and profile paths" {
        $originalComputerName = $env:COMPUTERNAME
        $originalUsername = $env:USERNAME
        try {
            $env:COMPUTERNAME = "TESTBOX"
            $env:USERNAME = "alice"

            $redacted = Redact-Sensitive "TESTBOX alice C:\Users\alice\Desktop\config.json"

            $redacted | Should -Be "[COMPUTER] [USER] C:\Users\[USER]\Desktop\config.json"
        } finally {
            $env:COMPUTERNAME = $originalComputerName
            $env:USERNAME = $originalUsername
        }
    }
}

Describe "Write-Log redaction" {

    BeforeEach { Reset-TestState }

    It "writes redacted content to the log file" {
        $originalComputerName = $env:COMPUTERNAME
        $originalUsername = $env:USERNAME
        try {
            $env:COMPUTERNAME = "TESTBOX"
            $env:USERNAME = "alice"
            Initialize-Log

            Write-Log "INFO" "TESTBOX alice C:\Users\alice\Secrets\state.json"

            $content = Get-Content $CFG_LogFile -Raw
            $content | Should -Match "\[COMPUTER\]"
            $content | Should -Match "\[USER\]"
            $content | Should -Match "C:\\Users\\\[USER\]\\Secrets\\state.json"
            $content | Should -Not -Match "TESTBOX"
            $content | Should -Not -Match "alice"
        } finally {
            $env:COMPUTERNAME = $originalComputerName
            $env:USERNAME = $originalUsername
        }
    }
}

Describe "Initialize-Log header redaction" {

    BeforeEach { Reset-TestState }

    It "writes placeholders instead of raw host and user values" {
        $originalComputerName = $env:COMPUTERNAME
        $originalUsername = $env:USERNAME
        try {
            $env:COMPUTERNAME = "TESTBOX"
            $env:USERNAME = "alice"

            Initialize-Log

            $content = Get-Content $CFG_LogFile -Raw
            $content | Should -Match "Host:\s+\[COMPUTER\]"
            $content | Should -Match "User:\s+\[USER\]"
            $content | Should -Not -Match "TESTBOX"
            $content | Should -Not -Match "alice"
        } finally {
            $env:COMPUTERNAME = $originalComputerName
            $env:USERNAME = $originalUsername
        }
    }
}
