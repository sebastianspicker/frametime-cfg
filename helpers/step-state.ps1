# ==============================================================================
#  helpers/step-state.ps1  —  Step Progress / Resume System
# ==============================================================================

function Load-Progress {
    if (Test-Path $CFG_ProgressFile) {
        try { return (Get-Content $CFG_ProgressFile -Raw -ErrorAction Stop | ConvertFrom-Json) }
        catch {
            Write-Warn "Progress tracking file was corrupted — starting fresh. (Your optimizations are not affected.)"
            try { Copy-Item $CFG_ProgressFile "$CFG_ProgressFile.corrupt" -Force -ErrorAction Stop } catch { Write-DebugLog "Could not preserve corrupted progress file." }
        }
    }
    return $null
}

function Save-Progress($prog) {
    Save-JsonAtomic -Data $prog -Path $CFG_ProgressFile
    Set-SecureAcl -Path $CFG_ProgressFile
    Write-DebugLog "Progress saved: Phase $($prog.phase) Step $($prog.lastCompletedStep)"
}

function Complete-Step([int]$phase, [int]$stepNum, [string]$stepName) {
    if ($SCRIPT:DryRun) { Write-DebugLog "DRY-RUN: Step $stepNum ($stepName) not recorded."; return }
    # Flush backup buffer BEFORE persisting progress — if we crash between flush and
    # Save-Progress, the worst case is re-running a completed step (safe). The reverse
    # (progress saved, backup lost) means we can't rollback a step we recorded as done.
    try { Flush-BackupBuffer } catch {
        Write-Warn "Flush-BackupBuffer in Complete-Step failed: $_ — skipping Save-Progress (step will re-run on resume)."
        return
    }
    $prog = Load-Progress
    if (-not $prog) {
        $prog = [PSCustomObject]@{ phase=0; lastCompletedStep=0; completedSteps=@(); skippedSteps=@(); timestamps=[PSCustomObject]@{} }
    }
    $prog.phase             = $phase
    $prog.lastCompletedStep = $stepNum
    # Use composite key "P{phase}:{step}" to disambiguate Phase 1 vs Phase 3 step numbers
    $stepKey = "P${phase}:${stepNum}"
    if ($stepKey -notin @($prog.completedSteps)) {
        $prog.completedSteps = @($prog.completedSteps) + $stepKey
    }
    # Ensure timestamps is a PSCustomObject for consistent Add-Member behavior.
    # On first call, timestamps may be a hashtable (from the initial @{} literal);
    # Add-Member on a hashtable creates a NoteProperty that is lost during JSON
    # round-trip. Converting to PSCustomObject first ensures the property persists.
    if ($prog.timestamps -is [hashtable]) {
        $prog.timestamps = [PSCustomObject]$prog.timestamps
    }
    $prog.timestamps | Add-Member -NotePropertyName "$phase-$stepNum" `
        -NotePropertyValue (Get-Date -Format "yyyy-MM-dd HH:mm:ss") -Force
    Save-Progress $prog
    Write-DebugLog "Step $stepNum completed: $stepName"
}

function Skip-Step([int]$phase, [int]$stepNum, [string]$stepName) {
    if ($SCRIPT:DryRun) { Write-DebugLog "DRY-RUN: Skip-Step $stepNum ($stepName) not recorded."; return }
    $prog = Load-Progress
    if (-not $prog) {
        $prog = [PSCustomObject]@{ phase=0; lastCompletedStep=0; completedSteps=@(); skippedSteps=@(); timestamps=[PSCustomObject]@{} }
    }
    $stepKey = "P${phase}:${stepNum}"
    if ($stepKey -notin @($prog.skippedSteps)) {
        $prog.skippedSteps = @($prog.skippedSteps) + $stepKey
    }
    # Do NOT add to completedSteps — skippedSteps is a separate semantic.
    # Test-StepDone checks both arrays for resume purposes.
    $prog.phase = $phase
    # Track lastSkippedStep separately — do NOT advance lastCompletedStep,
    # because skipped steps were not actually executed. Resume should use
    # the maximum of lastCompletedStep and lastSkippedStep for determining
    # where to continue from.
    if (-not $prog.PSObject.Properties['lastSkippedStep']) {
        $prog | Add-Member -NotePropertyName "lastSkippedStep" -NotePropertyValue $stepNum -Force
    } else {
        $prog.lastSkippedStep = [math]::Max($prog.lastSkippedStep, $stepNum)
    }
    Save-Progress $prog
}

function Test-StepDone([int]$phase, [int]$stepNum) {
    $prog = Load-Progress
    if (-not $prog) { return $false }
    # A step is "done" for resume purposes if it was either completed or skipped.
    # Uses composite key "P{phase}:{step}" only — bare step numbers are NOT checked
    # because they would collide across phases (e.g., Phase 1 step 5 vs Phase 3 step 5).
    # Legacy progress files with bare numbers are treated as empty (user re-runs from step 1).
    $stepKey = "P${phase}:${stepNum}"
    return ($stepKey -in @($prog.completedSteps) -or $stepKey -in @($prog.skippedSteps))
}

function Show-ResumePrompt($phase, $totalSteps) {
    $prog = Load-Progress
    if (-not $prog -or $prog.phase -ne $phase) { return 1 }
    $hasSkipped = $prog.skippedSteps -and @($prog.skippedSteps).Count -gt 0
    if ($prog.lastCompletedStep -eq 0 -and -not $hasSkipped) { return 1 }
    # Consider both completed and skipped steps for resume position.
    # Use phase-scoped keys to find the highest step number in the current phase,
    # not the raw lastSkippedStep scalar which may belong to a different phase.
    $lastProcessed = $prog.lastCompletedStep
    $phasePrefix = "P${phase}:"
    $lastSkippedInPhase = 0
    if ($prog.skippedSteps) {
        @($prog.skippedSteps) | Where-Object { $_ -like "${phasePrefix}*" } | ForEach-Object {
            if ($_ -match ':(\d+)$') { $lastSkippedInPhase = [math]::Max($lastSkippedInPhase, [int]$Matches[1]) }
        }
    }
    if ($lastSkippedInPhase -gt $lastProcessed) {
        $lastProcessed = $lastSkippedInPhase
    }
    $nextStep = $lastProcessed + 1
    if ($nextStep -gt $totalSteps) {
        Write-Info "All steps in this phase already completed."
        if ($SCRIPT:Profile -eq "YOLO") { return ($totalSteps + 1) }
        $r = Read-Host "  Start over anyway? [y/N]"
        if ($r -match "^[jJyY]$") { Clear-Progress $phase; return 1 }
        return ($totalSteps + 1)  # Sentinel: callers use `for ($step = $startStep; $step -le $totalSteps; ...)` — this skips the loop
    }
    if ($SCRIPT:Profile -in @("SAFE","YOLO")) {
        Write-Info "$($SCRIPT:Profile) profile: auto-resume from step $nextStep (of $totalSteps)."
        return $nextStep
    }
    Write-Blank
    Write-Host "  ┌─────────────────────────────────────────────────────────" -ForegroundColor DarkYellow
    Write-Host "  │  RESUME — Previous session found" -ForegroundColor Yellow
    Write-Host "  │  Last step: $lastProcessed  |  Continue from: $nextStep" -ForegroundColor White
    # Filter completedSteps and skippedSteps to only show steps from the current phase.
    # Without this filter, steps from Phase 1 and Phase 3 would appear mixed together.
    $phasePrefix = "P${phase}:"
    $doneNums = @($prog.completedSteps | Where-Object { $_ -like "${phasePrefix}*" } |
        ForEach-Object { if ($_ -match ':(\d+)$') { $Matches[1] } else { $_ } })
    $doneList = ($doneNums -join ', ')
    $skipNums = @()
    if ($prog.skippedSteps -and @($prog.skippedSteps).Count -gt 0) {
        $skipNums = @($prog.skippedSteps | Where-Object { $_ -like "${phasePrefix}*" } |
            ForEach-Object { if ($_ -match ':(\d+)$') { $Matches[1] } else { $_ } })
    }
    $skipList = if ($skipNums.Count -gt 0) { " | Skipped: $($skipNums -join ', ')" } else { "" }
    Write-Host "  │  Done: steps $doneList$skipList" -ForegroundColor DarkGray
    Write-Host "  │" -ForegroundColor DarkYellow
    Write-Host "  │  [1]  Resume from step $nextStep" -ForegroundColor Green
    Write-Host "  │  [2]  Choose a specific step" -ForegroundColor White
    Write-Host "  │  [3]  Start over (reset progress)" -ForegroundColor DarkGray
    Write-Host "  └─────────────────────────────────────────────────────────" -ForegroundColor DarkYellow
    $r = ""
    do {
        $r = Read-Host "  [1/2/3]"
        if ($r -notin @("1","2","3")) {
            Write-Host "  Invalid choice '$r' — please enter 1, 2, or 3." -ForegroundColor Yellow
        }
    } while ($r -notin @("1","2","3"))
    switch ($r) {
        "1" { return $nextStep }
        "2" {
            $s = Read-Host "  From step (1-$totalSteps)"
            $sv = 1
            if ([int]::TryParse($s,[ref]$sv) -and $sv -ge 1 -and $sv -le $totalSteps) { return $sv }
            Write-Warn "Invalid step '$s' — resuming from step $nextStep."
            return $nextStep
        }
        "3" { Clear-Progress $phase; return 1 }
    }
    return $nextStep
}

function Clear-Progress($phase = $null) {
    if (-not (Test-Path $CFG_ProgressFile)) { return }
    $prog = Load-Progress
    if (-not $prog) { return }

    if ($null -eq $phase -or $prog.phase -eq $phase) {
        # Reset to empty progress rather than deleting the file — avoids race conditions
        # and makes intent clear (file exists but no steps are done)
        $empty = [PSCustomObject]@{ phase=0; lastCompletedStep=0; lastSkippedStep=0; completedSteps=@(); skippedSteps=@(); timestamps=[PSCustomObject]@{} }
        Save-Progress $empty
        Write-DebugLog "Progress reset$(if($phase){" (Phase $phase)"})"
    } else {
        # Cross-phase re-run: progress file has a different phase than requested.
        # Still reset — the user explicitly asked to start over from this phase.
        Write-DebugLog "Cross-phase reset: progress had Phase $($prog.phase), resetting for Phase $phase"
        $empty = [PSCustomObject]@{ phase=0; lastCompletedStep=0; lastSkippedStep=0; completedSteps=@(); skippedSteps=@(); timestamps=[PSCustomObject]@{} }
        Save-Progress $empty
    }
}
