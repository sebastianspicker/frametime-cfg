# frametime.cfg alpha: known issues

Thanks for testing. This is an alpha. Please read this page before you start.
It lists what does not work yet, how to work around it, and what to send us
when something goes wrong.

## Before you start

- **Nothing has been tested on real Windows hardware yet.** Our automated
  checks build and test the code, but no one has yet run the full three-phase
  workflow on a real PC. Use a spare machine or a virtual machine. Do not use a
  PC you depend on.
- **Make a Windows restore point or a full backup first.** The workflow
  changes driver, boot, power, network and game settings.
- **You need an administrator account** and the signed alpha package. A build
  you compile yourself can only preview (`dry-run`) and evaluate benchmarks. It
  cannot change settings.
- **Import the alpha test certificate** that comes with the package, as
  described in the release notes. The package checks its signature online, so
  the PC needs internet access when you run a command that changes settings.

## Where to put the package

- Extract the ZIP into a plain local folder on an NTFS drive, for example
  `C:\frametime-cfg-rust`.
- Do not use a OneDrive, Dropbox or other synced folder, a USB stick, or a
  FAT/exFAT drive. The tool refuses to run from those and says why.
- Do not add files to the package folder. Windows sometimes adds `Thumbs.db` or
  `desktop.ini` by itself. If the tool reports an unexpected file, delete it or
  extract the ZIP again into an empty folder.
- Windows must be installed on drive `C:`. Other system drives are not supported
  yet.

## Using the tool

- **Only NVIDIA graphics cards get the full workflow.** On AMD and Intel Arc,
  `optimize` finishes Phase 1 and then stops. It does not start the Safe Mode
  driver steps. This is intentional for the alpha.
- **Run these first, in this order:**
  1. `frametime baseline-benchmark --vprof-file <your capture>`
  2. On NVIDIA only: `frametime driver prepare-nvidia --official-url <url>`
     (or `--local-installer <path>`)
  3. `frametime optimize`

  If you skip one, `optimize` stops and tells you which command to run.
- `optimize` without `--yes` first asks once whether to continue from the
  state your earlier commands saved (for example the baseline benchmark). It
  then asks before each step that needs your consent. `--yes` answers all of
  these questions with yes.
- **Cleanup deletes Temp files permanently.** Both `cleanup quick` and
  `cleanup full` need `--yes --acknowledge-irreversible`. In this alpha `full`
  does the same as `quick`. The deeper repair actions (shader caches, Prefetch,
  network reset, event logs) are not offered.
- The interactive menu covers only the most common commands. If you type an
  invalid choice, it exits instead of asking again. The menu's FPS-cap option
  only evaluates a capture file. It never saves a cap, and it prints the
  command you can run to save one.
- Benchmark captures must be saved as UTF-8 text. PowerShell 5's `>` saves
  UTF-16, which the tool rejects. Save the capture from a text editor as UTF-8
  instead.
- Ctrl+C is only honoured between workflow steps. Elsewhere, wait for the
  command to finish.

## If something goes wrong

- **Stuck in Safe Mode, or a step keeps failing at every sign-in:** open an
  elevated command prompt in the package folder and run
  `frametime abort-handoff --yes`. It removes only the boot and sign-in entries
  the tool created. If it reports that some entries remain, follow the manual
  steps in the recovery guide that ships in the package (`docs\native\recovery.md`).
  In plain Safe Mode there is no network, and the package's online signature
  check can fail. If that happens, restart into "Safe Mode with Networking", or
  use the manual steps.
- **After an abort, your graphics driver may be missing.** If Phase 2 had
  already removed the NVIDIA driver, `abort-handoff` does not put it back.
  Reinstall the NVIDIA driver after restarting normally.
- **Phase 3 does not start by itself after you sign in:** this may happen on
  PCs where User Account Control is on. We have not confirmed it yet. Open an
  elevated command prompt and run `frametime.exe phase3 --yes` from the runtime
  folder that the recovery guide describes.
- **"suite root DACL is not protected":** a `C:\FRAMETIME_CFG` folder from an
  older version or the retired PowerShell suite is in the way. Back it up, then
  move it aside from an elevated prompt
  (`move C:\FRAMETIME_CFG C:\FRAMETIME_CFG.old`) and run the command again.
- **"live transaction lock unavailable" after a crash:** a lock file was left
  behind. Report it with the files listed below. Do not delete things inside
  `C:\FRAMETIME_CFG` by hand unless we ask you to.
- **Safe Mode sign-in:** Safe Mode may not accept a Windows Hello PIN. Make sure
  you know the account password before you start. If BitLocker is on, have your
  recovery key ready.
- **The GUI:** after it restarts itself with administrator rights, re-import
  your benchmark file. It assumes Steam is in the default folder; if yours is
  elsewhere, enter the path by hand. It does not show log-write warnings.

## Reporting a bug

Open a bug report on the project's GitHub issue tracker using the bug template.
Please include:

1. What you ran (the exact command or GUI action) and the full output.
2. `frametime --version`, and the SHA-256 from the package's `.sha256` file.
3. Whether you ran from an elevated (administrator) prompt.
4. Your Windows version and edition, your graphics card and driver version, and
   whether the PC is a VM.
5. The output of `frametime verify`, `frametime backup-summary` and
   `frametime show-log`. The log is at
   `C:\FRAMETIME_CFG\Logs\frametime_current.log`.
   Benchmark and driver commands do not write to the log yet.
6. For workflow problems: a zipped copy of the whole `C:\FRAMETIME_CFG`
   folder. It contains your account name and hardware identifiers. Review it
   before you share it, or send it privately if the maintainers offer a way.

Never share passwords, signing keys or your BitLocker recovery key.
