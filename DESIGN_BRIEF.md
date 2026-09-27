# Design brief — frametime.cfg

Scope: the static GitHub Pages site (`site/`) and a visual/copy pass on the
native Win32 GUI (`crates/frametime-gui`). Work is left uncommitted for the
maintainer to review and commit (AGENTS.md: only the user commits).

## 1. Product

`frametime.cfg` is an alpha, native Rust tool for x64 Windows that inspects,
plans, applies, verifies and recovers selected Counter-Strike 2, GPU-driver,
network, power and Windows settings. Its substance is not "more FPS" but
*controlled change*: every supported change is a typed plan, gated by an
authenticated package, preceded by a state capture, and followed by a
verified readback, with restore bound to the exact target identity.

The catalog (`frametime_domain::catalog::STEPS`) has 54 steps across three
reboot phases (38 normal boot, 3 Safe Mode, 13 normal boot). In the
`dry-run all` preview, 25 of those steps are *advisory*: they deliberately
preserve a Windows default and explain when a change would be justified
("Acceleration off is a consistency preference, not a frametime
optimization"). Benchmark evidence (VProf P1 values from at least five runs)
must support any nonzero FPS cap.

**Moment of value.** On the site: the visitor sees the exact preview, step by
step, for their GPU, and understands that the tool changes little, shows
everything, and keeps a way back. In the product: the `dry-run` transcript and
the GUI's "237 FPS is supported" evaluation.

## 2. Audience

**Primary: the self-administering competitive CS2 player.** Plays seriously
(knows fps_max, P1 lows, VRR ceilings, VProf), administers their own Windows
PC, and has been burned by tweak lists: registry "boosts" that did nothing,
a bricked driver install, a debloater that broke Windows Update. Uses Steam,
the CS2 console, HWiNFO/CapFrameX/PresentMon, GitHub. Distrusts: percentage
FPS claims, before/after slider marketing, closed-source "optimizers" asking
for admin, anything that hides what it writes. Quality to them means: exact
numbers, named registry/driver targets, reversible steps, restraint.

**Secondary: reviewers and contributors** (Rust developers, security-minded)
arriving from GitHub, judging whether the trust model is serious. Quality to
them: precise language, visible limits, no overclaiming.

## 3. Key journeys (site)

1. Land → grasp what it is, its status, and that it is not a booster (≤ 5 s).
2. Inspect the register: pick a GPU branch, see what each step would do.
3. Understand the safety model: capture → apply → read back → restore, and
   the three reboot phases.
4. Understand the evidence rule for FPS caps.
5. Try the preview from source → go to docs / GitHub.

## 4. Brand traits

| Trait | …not |
| --- | --- |
| Skeptical | cynical or preachy |
| Exact | pedantic or cold |
| Protective | timid or alarmist |
| Plainspoken | dry or bureaucratic |
| Competitive-literate | gamer-hype |

## 5. Market observations

(No live web research in this session; from knowledge of the category —
see assumption A4.) CS2/Windows "optimizers" and FPS guides (commercial
booster apps, debloat scripts, YouTube "FPS BOOST" guides, tweak packs) share
conventions: near-black backgrounds, RGB/neon accents, aggressive italic or
techno display fonts, big percentage-gain claims, before/after bars, "one-click
optimize" buttons, and feature-card grids. Open-source Rust tools lean the
other way: GitHub-flavored defaults (system font, #0969da blue, cards).

- **Honor:** code blocks for commands, a plain link to GitHub and docs, a
  dark mode (players live in dark UIs at night).
- **Break:** neon-on-black, gain claims, one-click CTAs, card grids, and the
  GitHub-clone look the current site has.

## 6. What to keep

- The name `frametime.cfg` in lowercase with its file-extension dot.
- The GUI's classic desktop skin (approved reference
  `docs/assets/concepts/guided-session-flow.png`) and its **navy title strip,
  `#000080`** — the product's one real brand color. The site adopts it.
- The existing copy's honesty (status notice, "illustrative data").
- The concept images and lightbox (functionality).

## 7. Current weaknesses (site)

- Reads as a GitHub README with a default Primer palette and system font;
  nothing in the visual system is specific to this product.
- Five identical feature cards with shadows; centered-ish hero with two
  buttons; frosted sticky header.
- The product's most distinctive content (the 54-step register, the
  preserved defaults, the evidence rule) is absent.
- The lightbox close button is fixed white-on-black with no focus style; no
  skip link; inline style attribute.

## 8. Constraints

- Static site, no build step; `pages.yml` requires `site/index.html`,
  `site/styles.css` and copies `docs/assets/concepts/*.png` to
  `assets/concepts/`. Keep those paths.
- All external links, the concept images and the lightbox behavior stay.
- No claims beyond the repo's: no FPS gains, no qualification.
- WCAG 2.2 AA, keyboard, reduced motion, no layout shift, light and dark.
- GUI: native HWNDs only; High Contrast must keep system colors; the approved
  Win98 reference governs visual parity; files stay under the 600-line cap;
  must pass the Windows-target clippy. It cannot be rendered on this host.

## 9. Assumptions log

| # | Assumption | Evidence | Confidence |
| --- | --- | --- | --- |
| A1 | Primary audience is the self-administering competitive CS2 player, not IT admins of fleets | CS2 CFG assets, VProf/P1 vocabulary, GPU branches, autostart list (Discord, Spotify, Steam paths); README "machines managed by an administrator" | medium |
| A2 | The site is the project's public face (landing from GitHub/README "live demo") | README links it as "live demo"; `pages.yml` | high |
| A3 | `#000080` navy is intended brand equity | GUI `TITLE_COLOR`, approved concept reference, docs "navy task title strip" | high |
| A4 | Category conventions are neon/dark/booster marketing | general knowledge; no web research this session | medium |
| A5 | Showing the real dry-run register is acceptable public content | it is the output of a public source command, documented in README | high |
| A6 | Google Fonts (OFL) are acceptable for the site | prompt permits; no font assets in repo; self-hosting would add binaries | medium |
| A7 | The register snapshot may drift from the catalog | it is captured from `dry-run all` at 2026-09-27; no generator in CI | high (that it can drift) |

---

## Design Direction

The domain's own artifacts: a numbered step register (P1:1 … P3:13), a
dry-run transcript that says "Would plan / Would report / preserves", a
three-phase reboot sequence, receipts and readbacks, the engineering
discipline of capture-before-change. Emotionally the user is *wary*: about to
let a program touch drivers, Safe Mode and the registry on their gaming PC.

### Direction 1 — "The Register" (chosen)

**Concept.** The site is typeset like a controlled engineering document — a
change register with a drawing title block — because the product's promise is
documentary: nothing happens that is not written down first. The centerpiece
is the real 54-step register, filterable by GPU branch, where half the rows
say "leaves as is."

- **Type.** Archivo (variable width + weight, OFL): condensed heavy cuts for
  headings, like equipment placards and drawing title blocks; normal width
  for body. IBM Plex Mono for step codes, numbers and commands (tabular,
  unambiguous 0/O, 1/l). Scale ≈ 1.25, headings in a condensed width so long
  honest sentences still fit at display size.
- **Color.** Paper, ink, a muted ink, hairline rules, **navy `#000080`** as
  the single authority/interactive color (links, active filters, "changes"
  rows), and a signal red used only for the status stamp and Critical risk.
  Dark mode inverts to near-black paper with a lifted navy.
- **Layout.** A document grid: a narrow left margin column carries section
  numbers and step codes; content on a readable measure; the register runs
  wide. Hairline rules instead of boxes; density closer to a spec sheet than
  a landing page. Mobile is a single column with the margin labels inline and
  register rows stacked.
- **Motion.** Almost none: filter changes are instant (a reader comparing
  branches needs stability), row disclosure animates height only when motion
  is allowed, focus rings are immediate.
- **Signatures.** (1) The title block — a ruled metadata grid (Status,
  Platform, Toolchain, License) in the hero, like the corner of a technical
  drawing, with a boxed status stamp. (2) The register's verdict column with
  live counts: "25 of 54 steps leave Windows as it is."
- **Against the category.** Paper-light by default, no gain claims, no
  one-click CTA; the proof is the list of things it *won't* do.
- **Refuses.** Cards, shadows, gradients, icons in circles, scroll
  animations, hero screenshots.

### Direction 2 — "Transcript"

**Concept.** The page *is* a dry-run: dark console, every section rendered as
`[DRY-RUN] Would …` lines, with a blinking-free prompt and filters as CLI
flags. Type: JetBrains Mono throughout with a monospaced display cut; palette
of terminal black, one green for "plan", amber for "advisory". Layout: single
80-column measure. Signature: command-line navigation (`dry-run 3` shows the
AMD branch). Stands apart from boosters by being literal output rather than
marketing. Refuses: proportional type, images.
**Weakness:** hacker-terminal is itself a cliché in dev tooling; low
readability for long explanatory prose; weak on mobile; hides the GUI.

### Direction 3 — "Classic Desktop"

**Concept.** Extend the GUI's Windows 98-inspired skin to the web: gray face,
bevelled buttons, navy title strips, each section a window. Type: a Tahoma-
like humanist sans (e.g. a web-safe fallback plus a pixel-hinted display
face), palette `#C0C0C0` / `#000080` / white. Signature: a draggable
"FPS Strategy" window replaying the four-stage flow.
**Weakness:** nostalgia reads as a joke to the skeptical audience, bevels are
poor at small sizes and in dark mode, and the look imitates a famous OS.

### Choice

Direction 1. It grows directly from the product's substance (register,
capture/readback, evidence), serves both audiences (players scan the verdict
column; reviewers read the limits), works on mobile as a stacked list, and
carries the GUI's navy as the thread that connects site and app without
copying the Win98 skin. Traded away: the immediate "it's a real app" feel of
Direction 3 (mitigated by keeping the GUI concept images) and the raw
literalness of Direction 2 (mitigated by exposing the exact dry-run sentence
in each register row).

### GUI pass (native)

The approved classic skin stays; the pass is limited to what the Win32
controls allow and what can be verified by compile and host tests: copy that
is generic or inconsistent, and small, token-like consistency in
`app/retro.rs`. Native rendering remains unverified on this host (see
`docs/native/gui.md`).
