## What does this PR do?

<!-- Brief description of the change -->

## Evidence

<!-- Link benchmarks, testing results, or sources that support this change -->

## Checklist

- [ ] Tested on a real system (not just theory)
- [ ] `Invoke-ScriptAnalyzer -Settings .\PSScriptAnalyzerSettings.psd1` passes clean
- [ ] DRY-RUN mode works correctly for any new registry/boot changes
- [ ] Backup/restore handles the new changes
- [ ] README and relevant docs updated (if applicable)
- [ ] No external tool dependencies added

## Security

- [ ] No secrets, tokens, API keys, or credentials in the diff
- [ ] No `Invoke-Expression` / `iex` / `-EncodedCommand` usage
- [ ] New system-modifying calls respect `$SCRIPT:DryRun` guard
- [ ] No new `Invoke-WebRequest` calls to untrusted URLs
- [ ] Workflow changes (if any): actions pinned to SHA, permissions minimal
