## What does this PR do?

<!-- Brief description of the change -->

## Evidence

<!-- Link benchmarks, testing results, or sources that support this change -->

## Scope

- [ ] Runtime behavior changed
- [ ] Documentation/configuration only
- [ ] Historical archive or local-agent status files updated

## Checklist

- [ ] Tested on a real system (not just theory)
- [ ] CI `PSScriptAnalyzer` passes clean
- [ ] DRY-RUN mode works correctly for any new registry/boot changes
- [ ] Backup/restore handles the new changes
- [ ] README and relevant docs updated (if applicable)
- [ ] No external tool dependencies added
- [ ] Local-only artifacts remain ignored or archived intentionally
- [ ] Codacy local evidence and Codacy Cloud status are not conflated

## Security

- [ ] No secrets, tokens, API keys, or credentials in the diff
- [ ] No `Invoke-Expression` / `iex` / `-EncodedCommand` usage
- [ ] New system-modifying calls respect `$SCRIPT:DryRun` guard
- [ ] No new `Invoke-WebRequest` calls to untrusted URLs
- [ ] Workflow changes (if any): actions pinned to SHA, permissions minimal
