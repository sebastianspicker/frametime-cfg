---
name: frametime.cfg
description: Windows configuration and verification interface for Counter-Strike 2
colors:
  app-background: "#0B0D10"
  surface: "#11151A"
  surface-raised: "#181D23"
  divider: "#313943"
  control-border: "#667085"
  text-primary: "#F4F6F8"
  text-secondary: "#B8C0CC"
  text-muted: "#9AA5B4"
  accent: "#D6A43B"
  accent-hover: "#E2B24B"
  accent-pressed: "#B98527"
  success: "#22C55E"
  warning: "#FBBF24"
  danger: "#F87171"
  danger-fill: "#B42318"
  info: "#38BDF8"
typography:
  headline:
    fontFamily: "Segoe UI Variable Text, Segoe UI, Arial, sans-serif"
    fontSize: "22px"
    fontWeight: 600
    lineHeight: 1.25
  title:
    fontFamily: "Segoe UI Variable Text, Segoe UI, Arial, sans-serif"
    fontSize: "16px"
    fontWeight: 600
    lineHeight: 1.3
  body:
    fontFamily: "Segoe UI Variable Text, Segoe UI, Arial, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.4
  label:
    fontFamily: "Segoe UI Variable Text, Segoe UI, Arial, sans-serif"
    fontSize: "12px"
    fontWeight: 600
    lineHeight: 1.3
rounded:
  control: "4px"
  surface: "6px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "24px"
  xxl: "32px"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.app-background}"
    rounded: "{rounded.control}"
    padding: "8px 16px"
    height: "36px"
  button-secondary:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.control}"
    padding: "8px 14px"
    height: "36px"
  input:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.text-primary}"
    rounded: "{rounded.control}"
    padding: "7px 10px"
    height: "36px"
---

## Overview

This file records the visual tokens and component constraints used by the WPF
interface. It is a maintainer reference and a source for automated contrast
checks. Implemented behavior is defined by `ui/frametime-gui.xaml` and
`frametime-gui.ps1`.

The interface uses dark neutral surfaces, compact Windows controls, explicit
borders, and one amber accent. It avoids decorative motion, glow, gradients,
glass effects, and card grids without a functional grouping purpose.

## Colors

Amber is limited to focus, selection, and the primary action in a task region.
Blue is informational. Green, amber, and red semantic states must always be
paired with text or another non-color cue.

High Contrast replaces custom colors with Windows `SystemColors` resources.
The contrast contract tests normal operational text against every dark surface
and primary-button text against each interaction-state fill.

## Typography

Segoe UI Variable Text is preferred, with Segoe UI and Arial fallbacks.
Operational text uses 12 DIP or larger. Page headings use 22 DIP, section
titles use 16 DIP, body text uses 13 DIP, and labels or table data use 12 DIP.
Hierarchy comes from weight, spacing, and grouping rather than decorative type.

## Elevation

The interface is flat by default. Background tone, dividers, and control
borders establish hierarchy. Native Windows dialogs provide modal separation.
Custom shadows are not used.

## Components

### Buttons

- Primary buttons use the amber token, near-black text, a 36 DIP height, and a
  visible keyboard focus outline.
- Secondary buttons use the raised neutral surface and control border.
- Destructive buttons use the danger fill and must not appear as the page's
  ordinary primary action.

### Containers

- A bordered surface represents a distinct functional unit such as recovery
  readiness, staged changes, or a data region.
- Containers use 6 DIP corner rounding and no shadow.
- Default internal padding is 16 DIP, reduced to 12 DIP for dense regions.

### Inputs

- Inputs use a raised neutral fill, visible border, and connected label.
- Error and disabled states remain legible and do not rely on color alone.

### Navigation

Navigation uses sentence-case labels and native selection semantics. Keyboard
selection and Ctrl+number shortcuts are equivalent to pointer interaction.

### Tables

`DataGrid` is the default pattern for repeated assessment, setup, benchmark,
network, video, and recovery data. Numeric values align consistently. Status
columns contain words. Critical content may scroll instead of being truncated.

## Usage rules

- Show source, freshness, and recovery status beside consequential actions.
- Keep tables dense enough for comparison while retaining 12 DIP text.
- Expose loading, empty, stale, error, cancellation, and partial-failure states.
- Use native Windows controls where they satisfy the task.
- Do not use RGB or neon styling, decorative metrics, slogans, or marketing
  claims.
- Do not hide consequential actions behind hover, color, or success-only state.
