param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("ShowLog", "ResetProgress", "Restore", "BackupSummary", "Phase3")]
    [string]$Action
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

throw "Portable live execution is unavailable until a trusted installer or signed payload establishes the source identity."
