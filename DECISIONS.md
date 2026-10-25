# DECISIONS

## 2026-10-24: Copyright holder name
The brief's `[YOUR NAME]` placeholder was not filled in before sending. Used the git
`user.name` ("Madison") in `LICENSE` and `docs/PLAN.md`. Change it in both files if a
different legal name is wanted.

## 2026-10-24: Default branch
Repository initialised with `main` as the default branch.

## 2026-10-24: Design previews are HTML, not Rust
Per the brief, Phase 0 produces three self-contained HTML mockups restricted to effects egui's
painter can reproduce (rounded rects, two offset soft shadows, solid fills, simple linear
gradients, circles/arcs). No Rust is written until `design/WINNER.md` exists.

## 2026-10-24: Light-theme accent-hover tuned for contrast
All three previews independently found that white text on the brief's light accent-hover
`#3D74FF` lands around 4.1:1, below WCAG AA. Each darkened it slightly (A: #3368F5, B: #2E6AFF,
C: #2A66FF). The winner's value becomes the token in `design/WINNER.md`.

## 2026-10-24: Design A "Studio Hardware" is the winner
User picked A and asked for judgement-call improvements. Changes are listed in
`design/WINNER.md`. Notable calls: pads stay 40px (they define the look) so the main screen is
about 1240x1100; the app opens at 1280x1100 and scrolls vertically below 760px tall rather
than shrinking controls. Panel titles are sentence case; module and knob labels keep hardware
silkscreen caps.

## 2026-10-24: Design round 2 against a reference image
After picking A, the user supplied a neumorphism reference (soft-UI kit: cool off-white
surfaces, large-blur floating shadows, pills, concentric-ring dials, thin sliders with small
accent thumbs, light geometric type, dark charcoal panel with glowing accent circles) and asked
for a redo. Round 1 moved to `design/round1/`; `design/WINNER.md` removed so the loop gate
closes until a new pick. Palette tuning allowed: dark surface may be charcoal (reference) rather
than near-black, as long as orange/blue accents stay high contrast.

## 2026-10-24: Round-2 design A "Soft Console" is the winner
User picked A with no tweaks. `design/WINNER.md` is the token source of truth for the `neu`
crate. Dark surface is charcoal `#262A31` (per the reference) rather than the brief's near-black
starting value; orange/blue accents keep AA contrast. ON states use an accent-coloured glow
shadow plus one dark shadow, staying within the two-shadows-per-element rule.

## 2026-10-24: License allowlist includes font licenses
`cargo deny check licenses` allows MIT, Apache-2.0 (+LLVM exception), BSD-2/3, ISC, Zlib,
Unicode-3.0, BSL-1.0, OFL-1.1 and Ubuntu-font-1.0. The last two are needed only because
`epaint_default_fonts` bundles the Ubuntu and Noto fonts; both licenses permit embedding and
redistribution. No copyleft licenses are allowed.

## 2026-10-24: Effects routing and test patches
- Signal flow: voices -> FxChain (drive -> delay -> reverb) -> master gain -> limiter. The
  M0 diagnostic test tone is summed in AFTER the effects so it stays a clean reference.
- Engine defaults are deliberately a little wet (drive 35 %/50 %, delay 25 %, reverb 20 %) so
  the first sound has some space, as in the mock. DSP tests therefore call a `dry_fx` helper
  (effect mixes to zero) before measuring; the M1 level/shape/pitch tests additionally pin a
  plain sine patch (`sine_patch`) because the default patch is a filtered saw + sine mix. No
  assertion was weakened; only the setup was made explicit.
- `FxChain` is self-contained and tested on its own (`tests/fx.rs`), plus two engine-level
  tests for BPM sync through `SetBpm` and an audible echo.

## 2026-10-24: M3 review outcomes
A read-only reviewer audited the audio path and the UI against the design. No blocking
real-time findings. Fixed: the cpal error callback now sets a sticky atomic flag instead of
logging (cpal may run it on the audio thread); the kick onset test no longer returns early
when the kit is silent; black-key note names use text-primary for AA contrast in light; the
velocity control is a `neu` knob instead of a stock egui slider; egui widget outlines are
zeroed so depth comes only from the kit's shadows. Layout deviations from the token sheet are
recorded in `design/WINNER.md` "As-built tweaks".
