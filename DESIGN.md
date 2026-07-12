---
name: CS2 Optimize
description: Evidence-led Windows performance operations for Counter-Strike 2
colors:
  app-background: "#0B0D10"
  surface: "#11151A"
  surface-raised: "#181D23"
  divider: "#313943"
  control-border: "#667085"
  text-primary: "#F4F6F8"
  text-secondary: "#B8C0CC"
  text-muted: "#9AA5B4"
  accent: "#E8520A"
  accent-hover: "#F05A16"
  accent-pressed: "#D94B08"
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

### Creative North Star

The creative north star is **The Match Engineer**.

The interface is a calm technical workstation used beside a gaming PC before
and after consequential Windows changes. It prioritizes evidence, current
system state, recovery readiness, and the next safe action. Expert detail stays
visible in compact tables and filters; novice guidance appears where a decision
or handoff can fail.

The system is restrained rather than theatrical. It rejects RGB gaming-launcher
spectacle, generic SaaS card dashboards, terminal cosplay, decorative motion,
glass, gradients, glow, and marketing copy. Orange identifies product focus and
primary action, not generic importance or risk.

**Key Characteristics:**

- Dark neutral surfaces for a low-light desktop gaming environment.
- Medium-high density with readable 12–13 DIP operational text.
- Flat tonal hierarchy, explicit boundaries, and no decorative shadows.
- Text, symbols, and UI Automation semantics for every status.
- Native Windows interaction, High Contrast, and keyboard operation.

## Colors

The palette is a neutral operational dark theme with one established orange
accent and a separate semantic state vocabulary.

### Primary

- **CS2 Orange:** The sole brand and primary-action accent. It occupies less
  than ten percent of a screen and always uses near-black text on filled buttons.

### Secondary

- **System Blue:** Informational and observed-state feedback only.
- **Verified Green, Caution Amber, and Failure Red:** Semantic states that must
  always be paired with text or a symbol.

### Neutral

- **App Black:** Window background and primary-button text.
- **Control Surface:** Inputs, selected regions, and functional containers.
- **Operational Text:** Primary, secondary, and muted values remain readable at
  normal text sizes; muted never means low contrast.

**The One Accent Rule.** Orange is for identity, focus, selection, and the one
primary action. It is forbidden as a decorative highlight or risk substitute.

**The System Contrast Rule.** High Contrast replaces all custom colors with
Windows SystemColors resources rather than approximating the theme.

## Typography

**Display Font:** Segoe UI Variable Text with Segoe UI fallback
**Body Font:** Segoe UI Variable Text with Segoe UI fallback
**Label/Mono Font:** Segoe UI; technical values remain in the same family

**Character:** Familiar Windows typography keeps the application operational
and legible. Hierarchy comes from weight, spacing, and grouping rather than
display typography.

### Hierarchy

- **Headline** (600, 22 DIP, 1.25): page titles only.
- **Title** (600, 16 DIP, 1.3): functional sections and state summaries.
- **Body** (400, 13 DIP, 1.4): instructions, explanations, and form content.
- **Label** (600, 12 DIP, 1.3): controls, table headers, and compact metadata.
- **Table data** (400, 12 DIP): dense comparisons; numerical columns use
  tabular alignment.

**The Operational Floor Rule.** No consequential label, status, help text, or
metadata is smaller than 12 DIP.

## Elevation

The application is flat by default. Background, surface tone, dividers, and
control borders establish hierarchy. Shadows are prohibited; modal separation
uses the native Windows dialog and backdrop behavior.

**The Earned Container Rule.** A bordered surface must represent a real
functional unit such as recovery readiness, a staged change, or a distinct data
region. It may not exist merely to make a grid of cards.

## Components

### Buttons

- **Shape:** compact rectangular controls with gently curved corners (4 DIP).
- **Primary:** orange fill, near-black text, 36 DIP height; one per task region.
- **Hover / Focus:** brighter orange on hover and a persistent 2 DIP focus outline.
- **Secondary:** raised neutral surface, visible control border, primary text.
- **Destructive:** dark red fill with white text; never styled as the page primary.

### Cards / Containers

- **Corner Style:** restrained rounding (6 DIP).
- **Background:** base or raised surface according to hierarchy.
- **Shadow Strategy:** none.
- **Border:** subtle dividers for grouping; controls use the higher-contrast border.
- **Internal Padding:** 16 DIP default, 12 DIP in dense regions.

### Inputs / Fields

- **Style:** raised neutral fill, identifiable border, connected visible label.
- **Focus:** 2 DIP orange outline in the custom theme and system highlight in
  High Contrast.
- **Error / Disabled:** persistent error text; disabled state remains legible
  and is not color-only.

### Navigation

Navigation is a vertical, grouped selection surface using sentence-case labels.
Selection uses a filled neutral background, stronger text, and UI Automation
selection state—never a thick colored side stripe. Keyboard selection and
Ctrl+number shortcuts are equivalent to pointer interaction.

### Tables

DataGrid remains the canonical pattern for assessment, setup, benchmark,
network, video, and recovery comparisons. Headers are sortable where useful;
numeric data aligns consistently; status includes words; fixed columns may
scroll horizontally rather than truncate critical content.

## Do's and Don'ts

### Do

- **Do** show source, freshness, and recovery status beside consequential
  actions.
- **Do** use the exact semantic tokens and test their effective backgrounds.
- **Do** keep tables dense and use sentence-case labels.
- **Do** expose loading, empty, stale, error, cancellation, and partial-failure
  states.
- **Do** use native Windows controls and behavior wherever they meet the task.
- **Do** preserve expert filters and exact domain vocabulary.

### Don't

- **Don't** use RGB or neon gaming-launcher styling, glow, gradients, or
  glassmorphism.
- **Don't** build generic SaaS card grids or decorative metrics.
- **Don't** use tiny uppercase section eyebrows or thick colored side stripes.
- **Don't** place more than one primary action in a task region.
- **Don't** use ornamental Unicode glyphs as icons or accessible names.
- **Don't** hide critical actions in menus, behind hover, or behind color alone.
- **Don't** use marketing language, vague reassurance, or claims detached from evidence.
- **Don't** simplify away risk, provenance, expert data, or recovery controls.
