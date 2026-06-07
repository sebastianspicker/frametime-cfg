if (-not (Get-Module -ListAvailable PSScriptAnalyzer)) {
    Install-Module PSScriptAnalyzer -Force -Scope CurrentUser
}

$excludedRoots = @(
    "docs/archive",
    "docs/agent",
    "vendor",
    "third_party",
    "third-party",
    "3rdparty",
    "external"
)
$root = (Get-Location).Path
$pssaPaths = Get-ChildItem -Recurse -Filter "*.ps1" |
    Where-Object {
        $relative = [System.IO.Path]::GetRelativePath($root, $_.FullName).Replace("\", "/")
        $_.Name -ne "_TestInit.ps1" -and
            -not ($excludedRoots | Where-Object { $relative -eq $_ -or $relative.StartsWith("$($_)/") })
    }
if (-not $pssaPaths) {
    throw "No PowerShell files found for PSScriptAnalyzer"
}

$results = @()
foreach ($file in $pssaPaths) {
    try {
        $results += Invoke-ScriptAnalyzer -Path $file.FullName -Settings .\PSScriptAnalyzerSettings.psd1 -ErrorAction Stop
    } catch {
        throw "PSScriptAnalyzer failed on $($file.FullName): $($_.Exception.Message)"
    }
}
if ($results) {
    $results | Format-Table -AutoSize Severity, ScriptName, Line, RuleName, Message
    throw "$($results.Count) PSScriptAnalyzer issue(s) found"
}
Write-Output "PSScriptAnalyzer: all clean"
