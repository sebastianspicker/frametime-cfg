$module = Get-Module -ListAvailable Pester -ErrorAction SilentlyContinue |
    Where-Object Version -ge '5.0'
if (-not $module) {
    Install-Module Pester -Force -Scope CurrentUser -MinimumVersion 5.0 -MaximumVersion 5.99.99
}
