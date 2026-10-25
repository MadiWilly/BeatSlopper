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

## 2026-10-25: M4 polish findings
- Window size: eframe restores a persisted size and never clamps it to the work area, so an old
  smaller size stuck across launches. The app now re-requests the size on the first frame when
  the restored size is more than 10 % off, clamped to the monitor minus a margin. Displays
  with fewer than 1140 logical points of height (1080p at 125 %+) get the clamped size and scroll.
- Undo snapshots include the project path so undoing an Open does not retarget Ctrl+S.
- The UI->engine MIDI path uses a bounded `sync_channel` for the UI mirror because the
  unbounded std channel allocates per send inside the MIDI callback.

## 2026-10-25: Mixer (M6 engine)
- Pan law: equal-power sin/cos normalised to unity at centre (centre 1.0/1.0, hard pan
  sqrt 2 in one speaker), so existing absolute-level tests and the default sound stay intact;
  centre sits -3 dB relative to hard pan. Hard-panning is therefore +3 dB in that speaker.
- Delay and reverb became parallel send buses with return levels (DelayMix/ReverbMix) instead
  of the M2 serial dry/wet chain: drums no longer pass through the drive, delay no longer feeds
  the reverb, and the dry synth is no longer attenuated by the old crossfades (about +4.4 dB
  at default settings). Wet levels are unchanged.
- Meters are block peaks published per render through atomics; the UI applies the decay.

## 2026-10-25: Polyphonic notes (M7 engine)
- The synth lane is now a derived view of a polyphonic note list; grid edits upsert length-1
  notes. A note ends on the sample before its end step's trigger (length-1 notes therefore
  hand over legato to a same-pitch note on the next step instead of the old 90 % gate). The
  M3 sequencer timing tests were restated for the new rule with the same step coverage; no
  assertion was weakened.
- Notes that run past the pattern end are released at the wrap; a pattern shortened below
  the playing bar still finishes that bar.

## 2026-10-25: Phase 3 direction
The user wants a large preset library in EDM and Joji-style lofi. Because a plain subtractive
synth cannot make electric pianos, plucked guitars or supersaws, M8 adds FM, a Karplus-Strong
pluck mode, 7-voice unison, sub and noise sources, a filter ADSR with LP/BP/HP types, a pitch
envelope, velocity response, a lo-fi insert (bits, rate, wobble, vinyl noise, tone), chorus and
a kick-triggered sidechain ducker. M9 builds about 80 presets on top with a browser. Sound
design is done by DSP reasoning and measurable tests since the build machine cannot listen.

## 2026-10-25: M8 synth DSP choices
- Velocity-to-cutoff is relative to full velocity (soft notes close the filter; a full-velocity
  note leaves the cutoff knob as set) so existing patches and level tests keep their meaning.
- Pluck damping/brightness cutoffs are defined relative to the fundamental so the knobs mean
  the same across the keyboard; decay time is time-based via the loop gain (8 s to 0.25 s).
- Unison copy 0 is untouched so unison 1 is bit-identical to the pre-M8 oscillator.
## 2026-10-25: Plugins (EQ + reverb pop-out windows, side branch)
- Pop-out windows are egui *deferred* viewports so they keep repainting (spectrum, meters)
  while the main window sleeps. Their callback must be `Send + Sync + 'static`, so the windows
  draw from a `PluginShared` behind an `Arc<Mutex>` and send edits back as `PluginEdit`s that
  the main frame routes through the `AppState` setters (engine message, dirty, undo). The lock
  is never touched by the audio thread. Under `egui_kittest` the window bodies render as plain
  panels; viewports themselves are not testable headless.
- The EQ sits on the master bus after the effect returns and before master gain / limiter,
  and bypasses the diagnostic test tone, so every existing level test is unchanged. Flat and
  off states are bit-transparent (identity biquads); band / master toggles are 10 ms fades.
- `ReverbState` is the source of truth for the reverb; `ParamId::ReverbSize/Damping/Mix` keep
  working by writing into it (both in the engine and in `AppState::set_param`). On load a
  version-6 file's reverb state wins over a stale param; a version-5 file builds the state from
  its params.
- The spectrum analyser uses a hand-written radix-2 FFT (2048 points) instead of `rustfft`:
  the engine keeps its tiny dependency set and `deny.toml` is untouched.
- RT60 is an analytic estimate from the longest comb's loop time and feedback (plus a small
  damping term) rather than measured from the rendered tail, so the UI can show it instantly
  and the plot span never depends on a render; the rendered envelope is computed on a worker
  thread 100 ms after the last change.
- `FORMAT_VERSION` was bumped 5 -> 6 on this branch as instructed; main's M8 also bumps it, so
  the merge must resolve the version (and the doc list) by hand. Existing tests now compare
  against `FORMAT_VERSION` instead of a literal.
