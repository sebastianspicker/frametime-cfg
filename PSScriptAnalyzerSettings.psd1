# PSScriptAnalyzer settings for CS2 Optimization Suite
# Run: Invoke-ScriptAnalyzer -Path . -Recurse -Settings .\PSScriptAnalyzerSettings.psd1
#
# Last reviewed: 2026-03-21 (E3 CI audit)
# All exclusions validated against current codebase — each has documented justification.
@{
    Severity = @('Error', 'Warning')

    ExcludeRules = @(
        # ── Intentional by design ─────────────────────────────────────────
        # This is a CLI tool — Write-Host with colors is the correct output
        'PSAvoidUsingWriteHost',

        # UTF-8 without BOM is preferred (modern tooling, git compatibility)
        'PSUseBOMForUnicodeEncodedFile',

        # Empty catches are intentional best-effort patterns (7 instances):
        # Setup-Profile.ps1 state load, PostReboot-Setup.ps1 state save,
        # process-priority.ps1 affinity, CS2-Optimize-GUI.ps1 teardown (4x)
        'PSAvoidUsingEmptyCatchBlock',

        # $global:ProgressPreference needed for PS 5.1 Invoke-WebRequest compat
        'PSAvoidGlobalVars',

        # config.env.ps1 exports variables consumed via dot-sourcing;
        # PSScriptAnalyzer can't track cross-file variable usage
        'PSUseDeclaredVarsMoreThanAssignments',

        # Suite has its own DRY-RUN / consent system (Invoke-TieredStep);
        # ShouldProcess would be redundant
        'PSUseShouldProcessForStateChangingFunctions',

        # Internal functions not exported as a module — verb/noun conventions
        # are relaxed for readability (Load-State, Apply-*, Ensure-Dir, etc.)
        'PSUseApprovedVerbs',

        # Plural nouns are clearer for functions that operate on collections
        # (Restore-DrsSettings, Backup-DrsSettings, etc.)
        'PSUseSingularNouns',

        # logging.ps1 overrides Write-Log with a custom implementation
        # for the suite's logging system (Write-Debug renamed to Write-DebugLog)
        'PSAvoidOverwritingBuiltInCmdlets',

        # Some params are used inside scriptblocks passed to Invoke-DrsSession
        # which PSScriptAnalyzer can't track; others are API-contract placeholders
        'PSReviewUnusedParameter',

        # Pagefile code in Optimize-SystemBase.ps1 requires WMI .Put() methods
        # that have no simple Get-CimInstance equivalent; annotated in code
        'PSAvoidUsingWMICmdlet'
    )

    # ── Rules to include explicitly ───────────────────────────────────────
    # These detect real bugs that are easy to introduce accidentally.
    Rules = @{
        PSAvoidUsingConvertToSecureStringWithPlainText = @{ Enable = $true }
        PSAvoidUsingInvokeExpression                   = @{ Enable = $true }
        PSAvoidUsingPlainTextForPassword               = @{ Enable = $true }
        PSAvoidUsingUsernameAndPasswordParams          = @{ Enable = $true }
        PSUseCompatibleSyntax                          = @{
            Enable         = $true
            TargetVersions = @('5.1', '7.4')
        }
    }
}
