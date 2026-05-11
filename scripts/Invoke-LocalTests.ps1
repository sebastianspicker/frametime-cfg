<#
.SYNOPSIS
  Bootstrap Pester 5 and run the repo's local Pester checks.

.DESCRIPTION
  Windows PowerShell ships with Pester 3.x on many machines, but this repo's
  tests and CI use Pester 5.x command shapes such as -CI and
  New-PesterConfiguration. This wrapper installs Pester 5 in CurrentUser scope
  when needed, imports it explicitly, then runs the requested test paths.
#>
param(
    [string[]]$Path = @("./tests"),
    [switch]$SkipInstall,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$AdditionalPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$testPaths = @(
    foreach ($entry in @($Path + $AdditionalPath)) {
        foreach ($part in ($entry -split ",")) {
            $trimmed = $part.Trim()
            if ($trimmed) { $trimmed }
        }
    }
)

$pesterModule = Get-Module -ListAvailable Pester |
    Where-Object { $_.Version -ge [version]"5.0.0" -and $_.Version -lt [version]"6.0.0" } |
    Sort-Object Version -Descending |
    Select-Object -First 1

if (-not $pesterModule) {
    if ($SkipInstall) {
        throw "Pester 5.x is not installed. Re-run without -SkipInstall or install Pester 5 in CurrentUser scope."
    }

    Write-Host "Pester 5.x not found. Installing in CurrentUser scope..." -ForegroundColor Yellow
    Install-Module Pester -Force -Scope CurrentUser -MinimumVersion 5.0 -MaximumVersion 5.99.99 -SkipPublisherCheck

    $pesterModule = Get-Module -ListAvailable Pester |
        Where-Object { $_.Version -ge [version]"5.0.0" -and $_.Version -lt [version]"6.0.0" } |
        Sort-Object Version -Descending |
        Select-Object -First 1
}

if (-not $pesterModule) {
    throw "Pester 5.x could not be found after installation."
}

Remove-Module Pester -Force -ErrorAction SilentlyContinue
Import-Module $pesterModule.Path -Force

$imported = Get-Module Pester
Write-Host "Using Pester $($imported.Version) from $($imported.Path)" -ForegroundColor Green

Invoke-Pester -Path $testPaths -CI
