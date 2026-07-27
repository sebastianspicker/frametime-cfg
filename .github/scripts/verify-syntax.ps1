Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$errors = 0
$excludedRoots = @(
    "docs/archive",
    "vendor",
    "third_party",
    "third-party",
    "3rdparty",
    "external"
)
$root = (Get-Location).Path.TrimEnd([char[]]@("\", "/"))
$rootPrefix = "$root$([System.IO.Path]::DirectorySeparatorChar)"
Get-ChildItem -Recurse -Filter "*.ps1" | Where-Object {
    if (-not $_.FullName.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Syntax-check path is outside the repository root: $($_.FullName)"
    }

    # Path.GetRelativePath is unavailable in the .NET Framework used by
    # Windows PowerShell 5.1. The enumerated files are currently below
    # $root, so removing the validated root prefix is equivalent and works in
    # both Windows PowerShell and PowerShell 7.
    $relative = $_.FullName.Substring($rootPrefix.Length).Replace("\", "/")
    -not ($excludedRoots | Where-Object { $relative -eq $_ -or $relative.StartsWith("$($_)/") })
} | ForEach-Object {
    $parseErrors = $null
    $null = [System.Management.Automation.Language.Parser]::ParseFile($_.FullName, [ref]$null, [ref]$parseErrors)
    foreach ($e in $parseErrors) {
        Write-Error "$($_.Name):$($e.Extent.StartLineNumber) - $($e.Message)"
        $errors++
    }
}
if ($errors -gt 0) {
    throw "$errors parse error(s) found"
}
Write-Output "Syntax check: all files parse cleanly"
