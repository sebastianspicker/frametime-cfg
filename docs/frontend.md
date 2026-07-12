# Frontend architecture and conventions

The desktop frontend is a Windows Presentation Foundation (WPF) application
hosted by PowerShell. It is an operational companion to the terminal workflows:
it assesses system state, exposes setup and verification entry points, presents
benchmark and network history, manages video settings, and provides recovery
controls.

## Structure

- `CS2-Optimize-GUI.ps1` owns startup, XAML loading, navigation, asynchronous
  operation lifecycle, Windows High Contrast adaptation, and shutdown cleanup.
- `ui/CS2-Optimize-GUI.xaml` owns layout, resources, shared control styles,
  accessible names, labels, and live regions.
- `helpers/gui-panels.ps1` owns panel data mapping and interaction handlers.
- Existing domain helpers remain the source of truth for assessment, backup,
  benchmark, network, tier, and video behavior.

Do not embed XAML back into the PowerShell entrypoint. Keep domain operations
out of click handlers when a shared helper already owns them.

## Navigation and page patterns

Navigation follows the normal task sequence:

1. Overview
2. Assess
3. Setup and verify, including profile and preview mode
4. Benchmark
5. Network
6. Video settings
7. Recovery

Overview pages use a vertically scrolling summary and explicit next actions.
Data workflows use a page header, a sortable table, and a wrapping action
footer. Consequential restore, clear, DNS, video-file, and setup operations must
retain their existing confirmation or preview boundary.

## Styling

`DESIGN.md` is the source of truth for visual tokens and component rules.
Resources in the XAML use `DynamicResource` so Windows High Contrast changes
can replace brushes at runtime. Keep the interface dark, restrained, and
information-oriented. Use the orange accent for the principal action or active
state, not for decoration.

Shared styles are intentionally limited to navigation, primary/secondary/danger
buttons, cards, form controls, progress, and data grids. Page-specific layout
should remain local. Do not add a runtime styling framework or a generic wrapper
component layer.

## Accessibility

The target is WCAG 2.2 AA-equivalent desktop behavior plus Windows UI
Automation conventions:

- all actions must be reachable by keyboard with a visible focus indicator;
- use native WPF controls and native window chrome;
- pair inputs with `Label.Target` and provide an automation name when visual
  context alone is insufficient;
- announce changing operation status with polite live regions;
- never encode status by color alone;
- preserve Windows High Contrast and reduced-motion preferences;
- avoid nonessential animation;
- keep long-running assessment work cancellable;
- verify tab order and focus restoration after dialogs on Windows.

## Responsive behavior

The supported target is Windows desktop at 100–200% scaling with an effective
content area down to 960 by 540 pixels. The window is resizable with a
900-by-500 minimum safety floor. Toolbars and action groups should use wrapping
layouts; tables retain horizontal scrolling rather than hiding critical
columns. A phone or touch-first layout is not a goal.

## Supported runtime

The shipping target remains the Windows and PowerShell versions documented by
the repository and CI. WPF behavior must be validated on Windows. macOS can
verify XML, PowerShell parsing, documentation contracts, and non-WPF helper
logic, but it cannot validate UI Automation, High Contrast rendering, native
window behavior, or screenshots.

## Verification

Run the focused frontend contracts:

```powershell
Invoke-Pester -Path @(
  "tests/gui-design-contract.Tests.ps1",
  "tests/helpers/gui-panels.Tests.ps1"
) -CI
```

Also parse all PowerShell files, run `xmllint --noout
ui/CS2-Optimize-GUI.xaml` when available, and run the full local test entrypoint
before release:

```powershell
./scripts/Invoke-LocalTests.ps1
```

On Windows, manually verify Overview, Assess including cancellation, setup
launch, benchmark invalid/valid input, empty and populated Recovery, Network
failure/retry, and video write confirmation. Repeat with keyboard only, Windows
High Contrast, 200% scaling, and a 960-by-540 effective viewport. Capture new
screenshots only after those states pass. Legacy screenshots were removed
because they no longer represented the current task structure.

## Content conventions

Use domain terms that match the terminal workflow. Prefer explicit verbs such as
`Run full scan`, `Verify supported settings`, `Restore selected step`, and
`Write video.txt`. Avoid promotional copy, decorative glyphs, vague actions,
and claims of zero risk. Explain consequence and recovery near risky actions.

## Known limitations

- WPF does not provide a meaningful mobile experience.
- Phase 2 runs in Safe Mode where the GUI is unavailable.
- The benchmark chart remains a custom canvas; its table is the accessible data
  alternative.
- Visual regression is currently a Windows manual workflow; the repository has
  no automated WPF screenshot harness.
