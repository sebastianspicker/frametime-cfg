# Product

## Register

product

## Users

The primary user is a Counter-Strike 2 player optimizing a personal Windows
gaming PC for the first time. They understand the game, but may not understand
Windows services, Safe Mode, registry changes, driver handoffs, or the suite's
risk model. The interface must guide assessment, profile selection, recovery
readiness, the three terminal phases, verification, and measurement without
hiding the technical detail needed to make a safe decision.

Repeat competitive users are the primary expert audience. They return after
Windows, driver, hardware, or CS2 updates and need dense scanning, filters,
freshness timestamps, keyboard operation, quick reruns, benchmark comparison,
and unambiguous state provenance.

Maintainers and support users inspect exports, logs, backups, exact settings,
and partial failures. They need auditable labels, paths, timestamps, and error
details. All real operations run on an x64 Windows desktop with administrator
rights; there is no application role or account model.

## Product Purpose

CS2 Optimize is an evidence-led Windows operations suite for assessing,
applying, verifying, measuring, and reversing Counter-Strike 2 performance
changes. The WPF application is the management surface; the three optimization
phases continue to run in resumable terminal processes across Normal Mode, Safe
Mode, and the final normal boot.

Success means a user can identify the next safe action, understand its evidence
and risk, preserve a recovery path, distinguish recorded execution from current
system state, complete or resume the phase handoff, and measure the result.

## Brand Personality

Calm, precise, accountable.

The product behaves like a match engineer: inspect evidence before changing the
system, state assumptions and freshness, explain risk without drama, keep useful
instrumentation close, and verify the outcome.

## Anti-references

- RGB or neon gaming launchers that use spectacle instead of system status.
- Generic SaaS dashboards made from interchangeable cards and decorative
  metrics.
- Glassmorphism, gradients, glow, oversized rounded containers, and ornamental
  motion.
- Consumer-style simplification that hides expert detail or consequential
  actions.
- Terminal cosplay that sacrifices Windows conventions, readability, or
  accessibility.
- Marketing language, vague reassurance, fake activity, and claims detached
  from evidence.

## Design Principles

1. Evidence and freshness before confidence.
2. One clear next action without hiding expert detail.
3. Recorded execution and observed system state are different facts.
4. Interruption must match consequence and reversibility.
5. Recovery is part of every mutation workflow.
6. Native Windows semantics before custom interaction.
7. Dense tables where comparison matters; cards only for real functional units.
8. Every asynchronous operation ends with deterministic cleanup.

## Accessibility & Inclusion

The release target is WCAG 2.2 AA-equivalent behavior for a Windows desktop
application, plus Windows UI Automation support. Every primary and recovery
workflow must work with keyboard only, visible focus, Narrator, Windows High
Contrast, reduced animation, and 200% display scaling. The interface must remain
fully operable at 960 by 540 effective pixels. Status and risk may never rely on
color alone. Light mode, mobile, and browser interfaces are not release targets.
