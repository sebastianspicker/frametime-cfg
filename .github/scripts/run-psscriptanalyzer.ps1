$requiredVersion = [version]'1.24.0'
$module = Get-Module -ListAvailable -Name PSScriptAnalyzer -ErrorAction SilentlyContinue |
    Where-Object { $_.Version -eq $requiredVersion } |
    Select-Object -First 1

if (-not $module) {
    Install-Module -Name PSScriptAnalyzer -RequiredVersion $requiredVersion.ToString() -Force -Scope CurrentUser -ErrorAction Stop
}

Remove-Module -Name PSScriptAnalyzer -Force -ErrorAction SilentlyContinue
Import-Module -Name PSScriptAnalyzer -RequiredVersion $requiredVersion.ToString() -Force -ErrorAction Stop
$importedModule = Get-Module -Name PSScriptAnalyzer
if (-not $importedModule -or $importedModule.Version -ne $requiredVersion) {
    $actualVersion = if ($importedModule) { $importedModule.Version } else { '<not imported>' }
    throw "Expected PSScriptAnalyzer $requiredVersion, but imported $actualVersion"
}

Write-Output "PSScriptAnalyzer $($importedModule.Version) imported successfully"

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
