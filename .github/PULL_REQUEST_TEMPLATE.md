# Pull request

## What does this PR do?

<!-- Brief description of the change -->

## Evidence

<!-- Link benchmarks, testing results, or sources that support this change -->

## Scope

- [ ] Runtime behavior changed
- [ ] Documentation/configuration only
- [ ] Desktop interface or accessibility contract changed

## Checklist

- [ ] Tested on a real system (not just theory)
- [ ] CI `PSScriptAnalyzer` passes clean
- [ ] Native Rust source gates pass when `rust/` changes
- [ ] Driver Foundry Windows tests pass when its workspace changes
- [ ] New state-changing paths have a useful Full DRY-RUN plan and focused no-mutation test
- [ ] `START.bat dry-run all` exits cleanly with no preview issues and unchanged suite state (if runtime behavior changed)
- [ ] Backup/restore handles the new changes
- [ ] README and relevant docs updated (if applicable)
- [ ] Public docs describe the current checkout and contain no stale screenshots
- [ ] New runtime dependencies are documented and justified
- [ ] Local-only artifacts remain ignored or archived intentionally
- [ ] Codacy local evidence and Codacy Cloud status are not conflated

## Security

- [ ] No secrets, tokens, API keys, or credentials in the diff
- [ ] No personal paths, raw diagnostics, runtime state, or private workspace artifacts
- [ ] No `Invoke-Expression` / `iex` / `-EncodedCommand` usage
- [ ] New system-modifying calls respect `$SCRIPT:DryRun` guard
- [ ] No new `Invoke-WebRequest` calls to untrusted URLs
- [ ] Workflow changes (if any): actions pinned to SHA, permissions minimal
