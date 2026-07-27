# Security policy

## Scope

frametime.cfg runs PowerShell with administrator privileges and can modify the
registry, boot configuration, services, scheduled tasks, power plans, network
adapters, AppX packages, and display-driver packages. Recovery can also remove
legacy Defender exclusions that were recorded by an older checkout.
Security reports are especially relevant when they concern:

- command, path, registry, JSON, or configuration injection
- bypass of Full DRY-RUN or confirmation boundaries
- unsafe handling of persisted state, backups, or runtime payloads
- execution of an untrusted download or local executable
- privilege-boundary or ACL failures
- incomplete failure handling that reports a required operation as applied
- exposure of logs, inventory, paths, credentials, or other local data

## Version support

There is no tagged public alpha and no stated maintenance or security-support
period. Reports against the current release candidate are accepted. A version
support table should be added only when tagged versions and a maintenance policy
exist.

## Reporting a vulnerability

Use GitHub private vulnerability reporting when the repository Security tab
shows a `Report a vulnerability` action. This setting must be confirmed before
the public alpha. If the action is unavailable, open a public issue containing
no technical details and ask the maintainer to provide a private reporting
channel. Do not include exploit details, proof-of-concept code, logs, or
affected-system data in a public issue.

Include, where available:

- affected version, commit, or source snapshot
- affected entry point and execution mode
- reproduction steps and required privileges
- expected and observed trust-boundary behavior
- sanitized output or a minimal proof of concept
- a suggested remediation, if known

No response-time guarantee is stated for the alpha period.

## Data and network behavior

The project does not automatically send logs, inventory, backup data, or state
files to its maintainers. It does collect that information locally under
`C:\FRAMETIME_CFG` during live execution.

Automatic NVIDIA driver retrieval contacts NVIDIA. User-initiated network
diagnostics can make HTTP requests, send ICMP probes to live Valve SDR targets,
or open TCP connections to port 27017 on checked-in Steam connection-manager
candidates. Links opened or copied for manual downloads are visible in the source.
Do not place credentials or secrets in repository configuration or runtime
state.

## Download and execution boundaries

- The only implemented automatic executable download is an NVIDIA driver package
  from an allowlisted NVIDIA host.
- A downloaded driver must pass path, file, and NVIDIA Authenticode validation
  before an installation process is started.
- Phase 2 and Phase 3 run from a fixed runtime payload whose exact file set and
  SHA-256 manifest are validated.
- The codebase rejects `Invoke-Expression`, encoded-command download cradles,
  and untrusted workflow triggers through local and CI checks.

## Preview and recovery boundaries

`Run-Optimize.ps1 -FullDryRun` and `START.bat dry-run` provide the strict
no-persistence preview contract. Preview success confirms control-flow and
guard behavior. It does not prove that a privileged Windows API call will
succeed during live execution. See [`docs/dry-run.md`](../docs/dry-run.md).

Supported restore entries are recorded in `C:\FRAMETIME_CFG\backup.json` before
their corresponding mutation. AppX removal, driver-package removal, and some
file operations have separate or incomplete recovery behavior. See
[`docs/backup-restore.md`](../docs/backup-restore.md).

## Automated checks

The repository workflows parse PowerShell, run PSScriptAnalyzer and Pester,
exercise entrypoint previews, scan common credential and unsafe-execution
patterns, and require pinned GitHub Action revisions. These checks reduce known
risk but do not replace review or live validation.
