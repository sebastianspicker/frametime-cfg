$errors = 0
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
Get-ChildItem -Recurse -Filter "*.ps1" | Where-Object {
    $relative = [System.IO.Path]::GetRelativePath($root, $_.FullName).Replace("\", "/")
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
