# frametime.cfg

frametime.cfg is a Windows PowerShell toolkit for inspecting and changing
Counter-Strike 2 configuration and selected Windows, driver, power, storage,
audio, and network settings. Its main workflow runs in three phases across a
normal boot, a Safe Mode boot, and a final normal boot. A WPF desktop interface
exposes assessment, launch, verification, benchmark, network, video, and
recovery functions.

## Alpha status

The source currently identifies itself as `v3.0.0-alpha.1`. That version is not
tagged or packaged. The repository contains a `v2.2` tag, but state from the old
`C:\CS2_OPTIMIZE` layout is not compatible with the current
`C:\FRAMETIME_CFG` layout.

## Project purpose and scope

The project provides an inspectable workflow for applying and reviewing a
defined set of CS2 and Windows configuration changes. It is intended for local
use on an x64 Windows gaming system by an operator who can review privileged
PowerShell code and recover the system if a boot, driver, or configuration
operation fails.

The repository does not establish that its settings improve performance on all
systems. It contains no representative benchmark dataset and is not a Windows
image builder, driver distribution, system snapshot tool, or general-purpose
PC tuning framework.

## Current capabilities and limitations

The retained PowerShell implementation is designed to:

- inventory the CPU, GPU, memory, storage, network adapters, Windows state, and
  CS2 installation;
- run 38 profile-filtered Phase 1 steps for system and game configuration;
- publish a fixed Phase 2 and Phase 3 runtime payload with an exact-file-set
  SHA-256 manifest;
- clear the Safe Mode boot flag and remove verified display-driver packages in
  Phase 2 with Windows-native tools;
- download, validate, install, and configure an NVIDIA driver in Phase 3 when a
  supported package is available;
- apply narrower AMD and Intel paths with manual vendor-driver guidance;
- generate `optimization.cfg`, add its bootstrap to `autoexec.cfg`, and deploy
  optional CFG files from [`cfgs/`](cfgs);
- record supported pre-change registry, service, boot, task, power, network,
  pagefile, DNS, and NVIDIA DRS state in `backup.json`;
- verify selected settings, calculate an FPS cap, store benchmark summaries,
  run network diagnostics, inspect storage maintenance state, and restore
  supported backup entries;
- preview one or all four GPU branches across all three phases without
  elevation or persistent state changes; and
- provide a WPF desktop interface for the non-Safe-Mode management surfaces.

Current limitations:

- The portable tree exposes only smoke checks and the strict no-mutation
  preview. All live Phase 1, cleanup, restore, verification, FPS, and WPF routes
  fail closed until an installer or independently signed payload establishes
  source identity before elevation.
- The privileged three-phase workflow and recovery paths have not completed a
  release-candidate run on a disposable Windows system.
- NVIDIA automation is more complete than AMD and Intel automation.
- The desktop interface has not completed keyboard, screen-reader, High
  Contrast, 200 percent scaling, or minimum-window validation. Current
  screenshots are intentionally absent.
- Windows PowerShell 5.1 is the supported live host. PowerShell 7 lacks some
  Windows-only cmdlets and WMI behavior used by live steps.
- ARM64, Windows Server, LTSC, Constrained Language Mode, and restricted AppX
  environments use reduced or fallback behavior and are not fully validated.
- Restore is not a system snapshot. Removed driver packages, removed AppX
  packages, CS2 file edits, and some external application state require
  separate or manual recovery.
- Benchmark history stores summary values, not per-frame telemetry. CapFrameX
  is an optional external application and is not bundled.

## Requirements and prerequisites

Live execution is intended for:

- x64 Windows 10 or Windows 11 desktop;
- Windows PowerShell 5.1;
- an administrator account used for Phase 1, the Safe Mode sign-in, and the
  following normal-mode sign-in;
- Steam and Counter-Strike 2 for game-specific file operations; and
- free space under `C:\FRAMETIME_CFG` for logs, state, backups, runtime payloads,
  and an optional NVIDIA driver package.

Internet access is required only for automatic NVIDIA driver retrieval,
user-initiated live network diagnostics, and links opened for manual downloads.
Full DRY-RUN and most local inspection paths do not require network access.

Developer validation uses PowerShell 7, Pester 5, and PSScriptAnalyzer 1.24.0.
Windows PowerShell 5.1 is also required to reproduce the compatibility gates.
CI installs Pester 5.7.1.

## Installation

Clone the repository or extract a source archive to a local folder. Keep the
directory structure intact because scripts load configuration, helpers, CFG
files, and XAML relative to the repository root.

```powershell
git clone https://github.com/sebastianspicker/frametime-cfg.git
Set-Location .\frametime-cfg
cmd.exe /d /c START.bat dry-run all
```

The portable tree is preview-only. Live Phase 1, cleanup, restore, verification,
FPS, and WPF entrypoints fail closed because no trusted installer or signed
payload currently establishes their source identity before elevation. Existing
Phase 2 and Phase 3 handoffs remain recoverable only from their protected,
manifest-verified generation under `C:\FRAMETIME_CFG\runtime-generations`.

The PowerShell toolkit has no build or installation step. It does not install
a PowerShell module or register a Windows application. The portable launchers
do not request elevation; only the strict no-mutation preview is enabled.

The repository also contains a self-contained, source-only
[`rust/`](rust/README.md) rewrite. It has its own Cargo build, validation, and
portable packaging commands, but remains an alpha parity effort rather than a
release-equivalent replacement for the PowerShell product.

## Configuration

[`config.env.ps1`](config.env.ps1) is the central configuration file for the
retained implementation. Entry points that pass their fail-closed boundary load
it as executable PowerShell code, so any future trusted live release must bind
its exact bytes before elevation.

The main configuration groups are:

| Variable or group | Default or purpose |
| --- | --- |
| `$CFG_WorkDir` | `C:\FRAMETIME_CFG` |
| `$CFG_LogMaxFiles` | Retains five rotated logs |
| `$CFG_FpsCap_Percent` | Uses `0.09` in the FPS-cap calculation |
| `$CFG_FpsCap_Min` | Minimum calculated cap of `60` |
| `$CFG_ShaderCache_Paths` | Steam shader-cache locations checked for app ID 730 |
| `$CFG_Autostart_Remove` | Exact startup value names considered by cleanup |
| `$CFG_XboxServices` | Xbox service names considered by service steps |
| `$CFG_VirtualAdapterFilter` | Adapter descriptions excluded from NIC and DNS operations |
| `$CFG_NIC_Tweaks` | Requested NIC advanced-property values |
| `$CFG_DNS_Cloudflare`, `$CFG_DNS_Google` | DNS choices exposed by the workflow |
| `$CFG_CS2_Autoexec` | Ordered values written to `optimization.cfg` |
| `$CFG_URL_AMD_Chipset`, `$CFG_URL_Intel_Chipset` | Manual chipset-driver links |

`$CFG_FpsCap_Percent` is constrained to `0.01` through `0.50`, and
`$CFG_FpsCap_Min` is constrained to `30` through `500`; invalid values are
replaced with repository defaults when the configuration loads.

The public entry points do not define project-specific environment variables.
They do read standard Windows variables such as `ProgramFiles`, `LOCALAPPDATA`,
and `SAFEBOOT_OPTION`. Do not store secrets in `config.env.ps1`, saved state, or
logs.

The current runtime handoff and restore validation assume
`C:\FRAMETIME_CFG`. Changing `$CFG_WorkDir` is a code-level change and requires
corresponding test and trust-boundary updates.

## Usage

### Preview the workflow

Preview all mutually exclusive GPU branches:

```powershell
cmd.exe /d /c START.bat dry-run all
```

Preview one branch, where `1` is NVIDIA RTX 5000, `2` is other NVIDIA, `3` is
AMD, and `4` is Intel Arc:

```powershell
cmd.exe /d /c START.bat dry-run 2
```

The equivalent direct command for one branch is:

```powershell
powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass `
    -File .\Run-Optimize.ps1 -FullDryRun -DryRunGpu 2
```

A successful Full DRY-RUN exits with code `0`, renders all three phases, and
does not create or modify work-directory state, logs, backups, downloads,
handoffs, clipboard content, or system configuration. It exercises control
flow and guards, but it cannot prove that a later privileged Windows operation
will succeed. See [`docs/dry-run.md`](docs/dry-run.md).

### Run the terminal workflow

Live terminal execution from the portable tree is unavailable. `START.bat`
returns a nonzero exit for any route except `dry-run`. Enabling the three-phase
workflow requires an installed or independently signed payload that is
authenticated before any portable script is loaded.

If an already-published protected Phase 2/3 generation is pending, use that
generation's validated entrypoint or elevation bootstrap directly; do not route
recovery through the portable checkout.

### Run the desktop interface

`START-GUI.bat` is intentionally fail-closed for portable copies. The browser
demonstration remains available for non-privileged interface review. See
[`docs/gui.md`](docs/gui.md).

### Explore the browser demonstration

Open [`demo/index.html`](demo/index.html) directly in a browser. The static demo
mirrors the desktop information architecture with sanitized fixture data. It
does not run PowerShell, inspect the host, contact a service, or write suite
state. See [`demo/README.md`](demo/README.md) for its local checks and trust
boundary.

### Profiles

| Profile | Tier and risk behavior |
| --- | --- |
| `SAFE` | Runs T1 and T2 operations classified `SAFE`; skips other T2 and all T3 operations. |
| `RECOMMENDED` | Runs T1; prompts for T2 through `MODERATE`; skips T3. |
| `COMPETITIVE` | Runs T1; prompts for T2 and T3 through `AGGRESSIVE`. |
| `CUSTOM` | Prompts for each tiered operation and permits `CRITICAL` operations. |
| `YOLO` | Automatically runs eligible operations through `AGGRESSIVE`. |

These are repository policy labels, not compatibility or safety guarantees.
Phase setup and reboot handoffs have separate workflow logic and are not
cancelled merely by choosing a lower profile.

## Repository structure

| Path | Contents |
| --- | --- |
| `START.bat` | Strict preview launcher; portable live routes fail closed |
| `START-GUI.bat` | Fail-closed portable WPF launcher |
| `Launcher-Action.ps1` | Fail-closed legacy menu-action surface |
| `Run-Optimize.ps1` | Phase 1 orchestration and Full DRY-RUN lifecycle |
| `Optimize-SystemBase.ps1` | Phase 1 system steps |
| `Optimize-Hardware.ps1` | Phase 1 hardware, driver, and network steps |
| `Optimize-RegistryTweaks.ps1` | Phase 1 registry and service steps |
| `Optimize-GameConfig.ps1` | Phase 1 CS2 configuration and Phase 2 handoff |
| `SafeMode-DriverClean.ps1` | Phase 2 Safe Mode entry point |
| `PostReboot-Setup.ps1` | Phase 3 normal-mode entry point |
| `Cleanup.ps1`, `FpsCap-Calculator.ps1`, `Verify-Settings.ps1` | Standalone operator tools |
| `frametime-gui.ps1`, `ui/` | WPF controller and XAML layout |
| `demo/` | Dependency-free browser demonstration and Node static checks |
| `helpers/` | State, backup, safety, hardware, driver, network, and GUI functions |
| `config.env.ps1` | Executable central configuration |
| `cfgs/` | CS2 CFG sources and Valve latency target data |
| `tests/` | Pester unit, integration, contract, and process tests |
| `scripts/` | Local test runner |
| `.github/` | CI, validation scripts, templates, and repository policy |
| `docs/` | Operational and implementation-specific documentation |
| `rust/` | Self-contained native alpha rewrite, compatibility ledger, and Cargo workspaces |

Read [`docs/architecture.md`](docs/architecture.md) for control flow, runtime
payload validation, state persistence, and phase handoff details.
The [documentation index](docs/README.md) groups the remaining operator and
maintainer references by task.

## Development workflow

Work from the repository root. The PowerShell toolkit has no compile or package
step; its validation commands are listed below. Native Rust development uses
the separate commands documented in [`rust/README.md`](rust/README.md).

For a behavior change:

1. update the phase or helper implementation;
2. update `helpers/step-catalog.ps1` when step metadata changes;
3. add focused Pester coverage, including failure and preview behavior;
4. update the relevant operational document; and
5. run the applicable local and Windows compatibility gates.

State-changing code must preserve the dry-run boundary, validate persisted
input, capture supported pre-change state before mutation, and avoid marking a
required operation complete after failure.

## Testing

Install Pester 5 in the current-user scope and run all tests:

```powershell
pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File .\scripts\Invoke-LocalTests.ps1
```

If Pester 5 is already installed, prevent dependency installation:

```powershell
pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File .\scripts\Invoke-LocalTests.ps1 -SkipInstall
```

Run a focused test file:

```powershell
pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File .\scripts\Invoke-LocalTests.ps1 `
    -Path .\tests\helpers\system-utils.Tests.ps1 `
    -SkipInstall
```

Additional repository gates:

```powershell
pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File .\.github\scripts\verify-syntax.ps1

powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File .\.github\scripts\verify-syntax.ps1

pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File .\.github\scripts\run-psscriptanalyzer.ps1

pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File .\.github\scripts\verify-launcher-contracts.ps1

pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File .\.github\scripts\smoke-entrypoints.ps1 -Engine pwsh

pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File .\.github\scripts\smoke-entrypoints.ps1 -Engine powershell

pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass `
    -File .\frametime-gui.ps1 -SmokeTest

cmd.exe /d /c START.bat dry-run all
```

The browser demonstration uses the Node.js runtime bundled on CI runners and
has no package dependencies or lockfile:

```console
node --check demo/app.js
node --test demo/demo.test.mjs
```

The analyzer runner installs PSScriptAnalyzer 1.24.0 if needed. Dependency
installation requires access to PowerShell Gallery. CI runs Windows and macOS
Pester jobs, Windows PowerShell compatibility and entry-point smoke jobs, an
end-to-end process smoke job, syntax and analyzer checks, repository contracts,
and separate secret and script-safety checks.

## Deployment and operation

The PowerShell product has no deployment pipeline, installer, package manifest,
or service. A source checkout or extracted archive supports preview and smoke
validation only. The separate Rust alpha has its own build and packaging lane.

The retained live Phase 1 design has Step 38 copy a fixed runtime file set to a new directory
under `C:\FRAMETIME_CFG\runtime-generations`, writes and verifies a SHA-256
manifest, and updates `runtime-current.json`. Phase 2 and Phase 3 validate that
payload before loading helpers or making system changes.

Live state is stored under `C:\FRAMETIME_CFG`:

- `state.json` and `progress.json` for workflow state;
- `backup.json` and `backup.lock` for supported recovery records;
- `Logs\frametime_current.log` and rotated logs;
- `benchmark_history.json` and `latency_history.json`;
- `runtime-current.json` and immutable runtime generations; and
- temporary driver and setup files.

Do not delete this directory while a phase handoff or recovery operation is
pending. Read [`docs/backup-restore.md`](docs/backup-restore.md) before a live
run.

## Troubleshooting

- `Portable live execution is unavailable`: use the strict preview. A future
  live release requires a trusted installer or independently signed payload.
- `Pester 5.x is not installed`: rerun `scripts\Invoke-LocalTests.ps1` without
  `-SkipInstall`, or install a Pester version from 5.0.0 through 5.99.99.
- Phase 3 does not start: sign in with the same administrator account used in
  Phase 2 and invoke the protected generation's elevation bootstrap directly.
- Runtime payload validation fails: do not bypass manifest validation by
  launching a copied phase script. Preserve the work directory and use standard
  Windows recovery; the preview-only portable tree cannot publish a replacement
  generation.
- Windows remains in Safe Mode: from an elevated Command Prompt, run
  `bcdedit /deletevalue {current} safeboot`, verify with
  `bcdedit /enum {current} /v`, then restart.
- A step was interrupted: inspect the current log and backup summary before
  resuming or restoring. Preserve `C:\FRAMETIME_CFG` until recovery is complete.
- Full DRY-RUN exits nonzero: find the first `preview issue (DRY-RUN)` or
  `FATAL ERROR` marker in its output.
- CS2 files are not found: confirm that Steam and app ID 730 are installed in a
  detected Steam library before changing repository paths.

## Security considerations

- Protected Phase 2/3 handoffs and any future trusted live release run
  PowerShell as administrator and can change boot state,
  drivers, services, tasks, registry values, power configuration, network
  configuration, AppX packages, and application files.
- `config.env.ps1` is executable code. Review every change before running it.
- The portable Phase 1 checkout is not accepted as a live trust root. The
  repository does not currently ship an installer or independently signed
  payload that authenticates it across the elevation boundary.
- Automatic NVIDIA downloads are restricted to validated NVIDIA URLs and must
  pass path, file, and Authenticode checks before execution.
- User-initiated network diagnostics can fetch Valve SDR configuration, send
  ICMP probes, and open TCP connections to configured candidates.
- Logs, state, backup files, and exported diagnostics can contain user names,
  machine paths, adapter details, and hardware information. Sanitize them
  before sharing.
- Full DRY-RUN verifies preview behavior, not the success or safety of a future
  privileged operation.
- Restore coverage is partial. Keep an independent restore point, system image,
  and normal Windows recovery media for live testing.

Report vulnerabilities according to [`.github/SECURITY.md`](.github/SECURITY.md).

## Contribution guidance

Read [`CONTRIBUTING.md`](CONTRIBUTING.md) before changing stateful code. Keep
pull requests scoped, document runtime and recovery effects, add focused tests,
and list validation commands and unresolved results. Do not commit runtime
state, logs, reports, credentials, personal paths, or raw machine diagnostics.

## License

The repository is licensed under the [MIT License](LICENSE). Third-party source
references and unresolved provenance checks are listed in
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).

The project is not affiliated with Valve, NVIDIA, AMD, Intel, Microsoft, or the
maintainers of referenced third-party projects.
