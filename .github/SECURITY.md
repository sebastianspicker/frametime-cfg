# Security Policy

## Scope

The CS2 Optimization Suite modifies Windows registry keys, boot configuration, services, power plans, and NIC adapter settings. All changes are **backed up automatically** and reversible via the built-in rollback system.

The suite does **not**:
- Collect or transmit any user data
- Contain or require API keys, tokens, or credentials
- Download executables other than the official NVIDIA driver from nvidia.com
- Disable Windows Defender (only adds game-path exclusions)

## Supported Versions

Only the latest release on the `main` branch receives security fixes.

## Reporting a Vulnerability

If you discover a security issue (e.g., a code path that could be exploited, a bypass of the DRY-RUN safety system, or a way to inject code via crafted input), please report it **privately**:

1. **GitHub Security Advisories** (preferred): Go to the repository's Security tab and click "Report a vulnerability"
2. **Email**: Open a GitHub issue with the title "Security concern" and request private contact — do **not** include exploit details in public issues

Please include:
- Description of the vulnerability
- Steps to reproduce
- Affected file(s) and line number(s)
- Suggested fix (if you have one)

You should receive an acknowledgment within 72 hours.

## Security Design

### DRY-RUN safety
All state-changing operations respect `$SCRIPT:DryRun`. The following code paths are guarded:
- `Set-RegistryValue` / `Set-BootConfig` (centralized interception)
- `Set-NetAdapterAdvancedProperty` (NIC tweaks)
- `Enable-DeviceMSI` / `Set-NicInterruptAffinity` (MSI interrupts)
- Service stop/disable operations
- File writes (autoexec.cfg, optimization.cfg)
- QoS policy creation, URO disable, scheduled task creation

### Backup & rollback
Every modification is backed up to `C:\CS2_OPTIMIZE\backup.json` before execution. Users can roll back individual steps or all changes via `START.bat -> [7] Restore / Rollback`.

### No remote code execution
The codebase uses zero instances of `Invoke-Expression` / `iex`. The only network call is `Invoke-WebRequest` to nvidia.com for driver download, with file-size validation.

### CI enforcement
- PSScriptAnalyzer linting on all PRs
- Automated secret scanning
- Dangerous pattern detection (iex, encoded commands, download cradles)
- Workflow integrity checks (no pull_request_target, pinned action SHAs)
