$requiredVersion = [version]'5.7.1'
$module = Get-Module -ListAvailable -Name Pester -ErrorAction SilentlyContinue |
    Where-Object { $_.Version -eq $requiredVersion } |
    Select-Object -First 1

if (-not $module) {
    Install-Module -Name Pester -RequiredVersion $requiredVersion.ToString() -Force -Scope CurrentUser -ErrorAction Stop
}

Remove-Module -Name Pester -Force -ErrorAction SilentlyContinue
Import-Module -Name Pester -RequiredVersion $requiredVersion.ToString() -Force -ErrorAction Stop
$importedModule = Get-Module -Name Pester
if (-not $importedModule -or $importedModule.Version -ne $requiredVersion) {
    $actualVersion = if ($importedModule) { $importedModule.Version } else { '<not imported>' }
    throw "Expected Pester $requiredVersion, but imported $actualVersion"
}

Write-Output "Pester $($importedModule.Version) imported successfully"
