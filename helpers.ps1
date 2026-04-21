# ==============================================================================
#  helpers.ps1  —  Backward-compatible loader (dot-sources all helper modules)
# ==============================================================================
#  SECURITY: This file and all helpers/*.ps1 are dot-sourced — same trust boundary
#  as config.env.ps1. See config.env.ps1 header for dot-sourcing security analysis.

# Ensure strict mode is active even when loaded in a runspace pool (GUI async)
# where the entry point's Set-StrictMode is not inherited.
Set-StrictMode -Version Latest

$helpersRoot = if ($PSScriptRoot) { "$PSScriptRoot\helpers" }
               elseif ((Get-Variable -Name ScriptRoot -ErrorAction SilentlyContinue) -and $ScriptRoot) { "$ScriptRoot\helpers" }
               else { ".\helpers" }

# ── Core modules (CLI + GUI) ──────────────────────────────────────────────
. "$helpersRoot\logging.ps1"
. "$helpersRoot\tier-system.ps1"
. "$helpersRoot\step-state.ps1"
. "$helpersRoot\system-utils.ps1"
. "$helpersRoot\hardware-detect.ps1"
. "$helpersRoot\debloat.ps1"
. "$helpersRoot\msi-interrupts.ps1"
. "$helpersRoot\gpu-driver-clean.ps1"
. "$helpersRoot\nvidia-driver.ps1"
. "$helpersRoot\nvidia-drs.ps1"
. "$helpersRoot\backup-restore.ps1"
. "$helpersRoot\network-diagnostics.ps1"
. "$helpersRoot\storage-health.ps1"
. "$helpersRoot\nvidia-profile.ps1"
. "$helpersRoot\benchmark-history.ps1"
. "$helpersRoot\power-plan.ps1"
. "$helpersRoot\process-priority.ps1"
# ── GUI-only modules (loaded separately by CS2-Optimize-GUI.ps1) ──────────
# gui-panels.ps1      — WPF panel builders and event handlers
# step-catalog.ps1    — Step metadata table for Optimize panel display
# system-analysis.ps1 — Non-destructive health checks for Analyze panel
