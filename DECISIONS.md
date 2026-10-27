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
## 2026-10-25: Visualizers (branch `madi/visualizers-bc3bf5`)
- The master-output tap is a plain `rtrb` ring of whole stereo frames written once per block
  with `push_entire_slice` (no per-sample cost). An SPSC ring cannot drop its oldest entries
  from the producer side, so a full ring (the UI stalled) skips the rest of the block instead;
  the reader drains everything each frame and keeps its own 2048-frame window, so the picture
  catches up after one UI frame. Drum hits are a latched `AtomicU32` bitmask (`fetch_or` on
  the audio thread, `swap(0)` in the UI) rather than a per-block value, so no hit is lost
  between frames.
- The spectrum FFT is hand written (radix-2, fixed 2048 points, precomputed twiddles): no new
  dependency to vet, and the size never changes.
- Visualizer animation state lives in egui temp memory keyed by the widget id, so the kit keeps
  the stateless `fn(ui, theme, input, style)` widget signature while the strip, panel and
  overlay animate independently.
- Layout: the right column's head became the `Project | Chain | Visual` tab alone. Three tabs
  plus a title do not fit the 220 pt inner width, and a second row pushed the PROJECT panel's
  status line into the KEYS panel; the selected tab is set in the title style, so it reads as
  the title. The transport bar had no free width for the 220 pt strip at 1280: the Test tone
  pill moved under the MIDI selector (the column had the vertical room) and the group gap
  drops from 24 to 16 while the strip is shown; the strip then gets about 144 pt at 1280 and
  grows to 220 in wider windows, hiding below 100 pt instead of pushing the right-hand group
  out.
- The visualizer preset is an app setting (eframe storage), not project data; "Off" is a
  preset so the choice is a single persisted index.

## 2026-10-25: Design cleanup (selection states)
- The user could not tell which segment was selected (Synth | Pluck, the wave and filter
  selectors): the mock's "weight 500" cue is invisible with egui's one font weight. The
  selected item now sits on a raised accent chip with the on-glow treatment and on-accent
  text, animated between items (0.12 s, `animation_time` 0 disables it). This is a deliberate
  departure from `design/preview-a.html` recorded in `design/WINNER.md` "As-built tweaks";
  the control's geometry (track, item padding, gap) is unchanged so no panel grew.
- egui has no keyboard-focus-visible notion, so focus rings (knobs, dials, segmented items)
  show on `has_focus()` like the sliders already did, i.e. after a mouse click too.
- Parameters without effect in the current setting are dimmed, not disabled: the PULSE knob
  stays editable with a non-square wave so a value can be set before switching waves.
- The synth body's rows stop straddling the scroll fold (a group head alone above the edge
  read as an orphaned label). Rows that would be cut start at the fold instead; the resulting
  gap is up to one row of blank panel, accepted over a clipped head row.
## 2026-10-25: QOL pass (branch `madi/qol-polish-6ccb28`)
- Typed values are mapped back through each control's *formatter* rather than a new
  per-control parser field: `KnobSpec` / `SliderSpec` are built in many shared files, so a
  new field would touch all of them. An affine fit through three probes inverts every
  formatter in the app exactly (`70 %` -> `0.7`); the non-affine dB-of-gain formatter falls
  back to a bisection, which lands within display precision.
- Step cells act on the *press* (not the click) so drag-painting works like FL Studio; a
  press-and-release is still one toggle. Pressing an "on" cell starts an erase stroke;
  Ctrl-click is an alias of shift-click for the accent. Right-click clearing was dropped
  from this branch (the `test-feedback` session implements it, per the main session).
- `x2` copies the loop into the new half when that half is empty (what a producer expects
  from "double"), `/2` keeps the hidden steps so `x2` restores them. The M5 three-item LENGTH
  segmented did not fit the sequencer head beside the pills at the 1280 design width, so the
  pills flank a length read-out instead (4 bars = two clicks of `x2`).
- RECENT is a sub-view of the PROJECT panel (a 28 pt clock button, BACK returns) rather than
  a third right-column tab: `Project | Recent | Chain` beside the panel title overflowed the
  260 pt column, and a RECENT pill overflowed the NEW / OPEN row by 3 pt.
- Tap tempo uses egui's clock (`InputState::time`), not `Instant`, so kittests can drive it.
- Ctrl+Up / Ctrl+Down nudge the tempo as asked; the M9 brief reserves the same keys for
  preset stepping, so M9 should pick other keys (or drop these) when it lands.
- The window title carries the BPM and the edit pattern; the existing title tests were
  updated to the new format (no assertion weakened).

## 2026-10-25: Merging the QOL pass onto main (after the visualizers and design cleanup)
- The transport bar could not hold QOL's `(-) 128 (+) TAP` row *and* the visualizers' mini
  strip at the 1280 design width: the display-type readout and the TAP pill made the BPM
  group about 86 pt wider, which left the strip 35 pt (it hides below 100). The TAP pill now
  sits centred under the stepper row (the row is as tall as the MASTER dial, so the group has
  the vertical room) and the strip-on group gap takes one more step down the spacing scale
  (16 to 12). The strip gets about 120 pt at 1280 again; every QOL control is unchanged.
- The right column keeps the visualizers' tab-only head (`Project | Chain | Visual`); the
  RECENT sub-view selects the `Project` tab while shown, as the QOL branch intended.
- A sequencer cell evaluates the right-click (clear, `test-feedback`) before the QOL press
  branch (paint / Ctrl accent / Shift), so both keep working; the primary-only press test
  keeps a right-click from starting a stroke.
- QOL's typed readout replaced every stepper's value cell, including the VISUALIZER panel's
  PRESET stepper: the preset name is now the readout's accessible *value* (node
  `"Preset value"`), and the two visualizer tests read it that way instead of looking for the
  name as a label.
- A dimmed knob's value readout (design cleanup) is painted through `ui`, not the dimmed
  painter, so `knob_dimmed` multiplies the readout colour by `DIMMED_OPACITY` itself.

## 2026-10-25: Reverb audit (FX rack REVERB group vs the REVERB plugin window)
The plugins branch gave the reverb two UI surfaces: the FX rack's SIZE / DAMPING / MIX knobs
(`ParamId::ReverbSize / ReverbDamping / ReverbMix`) and the pop-out REVERB window
(`ReverbState`: size, damping, pre-delay, width, low cut, high cut, mix, presets). Audited
whether they fight; findings and choices:
- **They do not fight.** On both sides `ReverbState` is the source of truth and the three
  shared values are written through one path: the engine's `set_param(Reverb*)` writes the
  state and `set_reverb(Size | Damping | Mix)` goes through `set_param`; the app's `set_param`
  mirrors into `reverb`, `set_reverb` / `set_reverb_state` mirror into `params`, `apply_preset`
  copies the three values across, the undo `Snapshot` carries both (they are equal when
  snapshotted, so a restore keeps them equal), `apply_project` takes the three values from the
  file's `reverb` (the engine's `from_json` already synced them), `build_project` writes both
  from the mirrors. Tests now pin every one of these paths (engine `tests/plugins.rs` 13-16,
  app `tests/plugins_ui.rs` "reverb audit").
- **One real bug, fixed:** `Engine::set_reverb_state` (project load, and the only reverb path
  an offline render takes) left the chain's size / damping / mix smoothers and the reverb's
  own pre-delay / width glides running from the previous room. An export therefore heard the
  first hit with a pre-delay gliding up from 0 ms (about 80 ms of a 100 ms pre-delay, with a
  chirp), unlike the live app where the glide had long settled. `set_reverb_state` now snaps
  those smoothers (`FxChain::snap_reverb`, `Reverb::snap`), like `set_mixer_state` and
  `set_eq_state` already did; delay lines and the tail are untouched, so a live load does not
  cut the sound. Other FX smoothers (drive, delay) still glide from their defaults at the start
  of an export: pre-existing, out of this audit's scope, noted as a follow-up.
- **Synth presets keep the room's extras.** `Preset::apply` / `AppState::apply_preset` reset
  every `ParamId` (so size, damping and mix follow the synth preset) but leave pre-delay,
  width and the cuts where the user put them. A synth preset describes a sound, the extras
  describe the room; resetting them would surprise anyone who set up a room and then browsed
  sounds. The REVERB window's MODIFIED lamp shows that the result no longer matches a reverb
  preset. Documented, tested, kept.
- **`resend_all` sends the three shared values twice** (as `SetParam` with every param, then
  inside the seven `SetReverb`), with identical values. Harmless and left alone: the second
  write is the state's, so it can only ever confirm the first, and the count stays well inside
  the queue.
- **Not duplicates, say so.** Mixer REV sends are per-channel amounts *into* the reverb bus;
  MIX (rack and window) is the bus's return level. The rack's REVERB label, both MIX knobs
  (tooltip note) and the mixer's DLY / REV legends now say so, and the window's head carries
  a one-line hint that size, damp and mix are shared with the rack. The rack's REVERB group
  gained a small MORE pill (lit while the window is open) that opens / closes the window: it is
  not labelled OPEN because the PROJECT panel's `Open` button already owns that accessible
  name and the whole-app tests query it by label.
- `neu::knob_noted` (a knob whose tooltip carries a second line) was added rather than
  stacking a second `on_hover_text` on the response, so a knob keeps one tooltip.


## 2026-10-25: Phase 4 (instrument tracks, FL-style mixer) and the milestone order
The user asked for FL-style mixing channels and, as the biggest feature, layering different
synths into a roll (a pad roll with effects and reverb, then a keys roll on top) with an easy
list of them, modelled on how FL Studio and Ableton Live do it. The model and the reasons:
- **Tracks, not a "layer" flag.** FL's Channel Rack and Ableton's track list both treat an
  instrument as a row that owns its sound, its effects and a mixer strip, and FL stores notes
  for *every* channel in *every* pattern. BeatSlopper's `Pattern` therefore carries a note
  list per track, the selected track receives the keyboard and MIDI (FL's channel selector
  rule), and the roll ghosts the other tracks (what producers use to line up a layer).
  Ableton's Instrument Rack (parallel chains under one note stream) was considered and
  rejected for now: two tracks playing the same notes give the same result, and tracks also
  cover the "different notes per layer" case the rack does not.
- **Per-track vs global effects.** Drive, lo-fi, chorus and the sidechain ducker move into
  the track (FL: channel -> its insert; Ableton: the track's device chain). Delay, reverb and
  the master EQ stay shared returns (Ableton return tracks, FL send tracks); the mixer's sends
  already express "how much of this track goes to the reverb".
- **Order M8 -> M10 -> M9 -> M11.** The preset browser (M9) picks a preset *for a track*, so
  tracks land first and the browser is built once. The FL mixer (M11: inserts, effect slots,
  free routing, groups) sits on top of tracks and is the user's second ask.
- **M11 routing rule.** An insert may route only to MASTER or a higher-numbered insert, so a
  feedback loop cannot be expressed (FL greys out sources that would loop; a static rule is
  simpler and testable).
- Sources read for this design: FL Studio manual, Mixer
  (https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/mixer.htm),
  Mixer routing (…/mixer_iorouting.htm), Channel Rack (…/channelrack.htm), Layer channel
  (…/chansettings_layer.htm); Ableton Live 12 manual, Mixing
  (https://www.ableton.com/en/live-manual/12/mixing/), Instrument, Drum and Effect Racks
  (https://www.ableton.com/en/live-manual/12/instrument-drum-and-effect-racks/), Working with
  Instruments and Effects (https://www.ableton.com/en/live-manual/12/working-with-instruments-and-effects/).

## 2026-10-25: M10 engine (instrument tracks) as built
- `ParamId::is_fx()` is narrowed to the global chain (drive for the M2 serial path, delay,
  reverb); lo-fi, chorus and sidechain are `is_synth_insert()` ids, and `is_track()` /
  `is_global()` are the partition the app groups by. Drive, lo-fi, chorus and the ducker live
  in a per-track `SynthInsert`, which ticks its drive smoothers at the end of `process` so the
  v6 regression render stays bit-identical (hash pinned in `tests/tracks.rs`).
- `Pattern` equality ignores the derived `synth` lane and `lane_track` (content equality).
- `preset_index` left the project file in v7; the preset name lives per track
  (`TrackState.preset`); the v6 loader maps the index to a name.
- `SwapTracks` moves the edit track with its content and swaps voice and insert state (no
  click) rather than releasing voices. A switched-off track keeps rendering until its voices
  fade ("draining"), then costs nothing; `NoteOn` on an inactive edit track is ignored.
- `Channel::Synth` stays as a `const` alias of `Channel::Track(0)` (not a variant: `match`
  arms use `Channel::Track(_)`); every track channel defaults to delay and reverb send 1.0.
- `Project.tracks` missing means one default "Synth" track; an explicit empty list means none.
- The engine's eight tracks are boxed once at construction (real-time safe) so `Engine` stays
  movable by value.

## 2026-10-25: M10 app (part 1): engine built off the main thread
The debug app overflowed the main thread's 1 MiB stack at startup once the engine carried
eight tracks (`new_engine` builds `[Track; 8]` on the stack before boxing, under eframe's
event-loop frames); tests never saw it (2 MiB test threads). The app builds the engine on a
helper thread with a 16 MiB stack (`app::new_engine`), boxes the app state and the audio
renderer's engine, and gives the export thread the same stack. The engine is unchanged (a
later clean-up could construct the tracks in place). `select_track` is view state, not an
undo step, but `edit_track` is in the snapshot so undoing a track operation restores the
selection. A fresh track 0 keeps the pre-M10 convention (preset 0 highlighted, not modified).

## 2026-10-25: M9 preset library (engine)
- 91 presets live in `presets::LIBRARY`; `presets::ALL` keeps the original 8 (not in the
  library) until the browser replaces the synth panel's preset row. Names are unique across
  both; `index_of` indexes the library only.
- The lo-fi insert's vinyl noise is a continuous bed, not gated by the voices, so a preset
  with vinyl noise never falls below RMS 1e-4 even with no note. The decay test therefore
  measures each preset's own silent bed (3 s, no note) and requires the tail to fall under
  `max(1e-4, 2 x bed)`; presets without vinyl noise keep the strict 1e-4. Tagging ~35 lofi
  presets "drone" would have been wrong, and the DSP was out of that task's scope.
  `Preset::has_vinyl_noise()` exposes the fact to the UI.

## 2026-10-25: M9 preset browser (app)
- The app's preset universe is one unified index: the eight classics (`engine::presets::ALL`,
  `app::presets::CLASSIC`) then the 91-entry `LIBRARY` (99). Preset 0 stays "Bass" (a
  fresh track must not hiss), every pre-M9 index keeps its meaning, and the classics were
  not folded into the engine library: `ALL` feeds a dozen engine tests and the v6 project
  migration, and the library's render / decay tests would otherwise judge the M2 sounds.
- The browser replaces the SYNTH panel's *body* while open (same panel, same size) rather
  than a popup `Area` or a row-wide overlay: nothing on the page moves, the FX rack stays
  usable, and the 1280 x 1140 whole-sequencer tests hold with it open and closed.
- Preset stepping is Ctrl+PageUp / Ctrl+PageDown (the plan's Ctrl+Up / Down are the QOL
  tempo nudge, kept); Ctrl+Shift+P opens the browser with the search focused. Stepping
  walks the category filter only; the search text narrows the list, not the step order.
- The last preset is persisted by name (`preset_name`); the old `preset_index` is read as
  a fallback once and never written again. The project file was already name-based.
- Category chips use short labels (`Lo Keys · Lo Gtr · Lo Pads · Leads · Bass · Plucks ·
  EDM Pads · Risers`) so nine fit the panel at the minimum width; "FX" was avoided because
  the FX rack's tab is a radio item named "FX".

## 2026-10-25: M10 review fixes (app)
- DUP copies the source track's whole mixer strip (level, pan, sends, mute and solo), not
  just its synth settings: a duplicate is the track as it sounds, notes aside (Ableton's
  duplicate; FL's clone keeps the channel settings too). ADD starts from the default strip
  and DEL resets the slot's strip on both sides, since the engine keeps a strip across
  `SetTrackActive` and a deleted soloed track kept everything else silent.
- The clipboard is reconciled (notes on unused slots dropped) on load and on undo / redo,
  and is not restored by undo: undoing a DEL does not bring the deleted track's notes back
  into the clipboard. Clearing notes in `set_track_active(on)` was left to the engine list.
- The preset is no longer remembered across launches (the `preset_name` / `preset_index`
  storage keys and the track-0 re-apply are gone): the project file carries a preset per
  track, nothing else read the stored name, and re-applying it to track 0 silently changed a
  fresh project's sound.
- Sequencer head room (the chip *and* the name at 1280): rather than a second head row
  (the 1140 pt window has 27 pt to spare under the sequencer) or dropping FOLLOW from the
  ROLL view (a head that changes width between views), the head gap went from 12 to 8, the
  A..H pattern tabs use a 6 pt item padding (`neu::segmented_padded`, the kit's 8 on
  single letters was a third of the track) and the `/2` / `x2` length pills became the
  kit's 28 pt `(-)` / `(+)` steppers around the read-out, the transport's `(-) 120 (+)`
  shape. Together 68 pt; the name gets about 82 pt in both views.
- The grid rows' QOL "Clear" pills (73 pt) are the kit's 28 pt `Icon::Clear` circles: the
  rows were 947 pt in a 914 pt slot at 1280 x 1140, so the right-hand column hung 33 pt past
  the page in GRID view (and jumped 33 pt when switching to ROLL); the existing tests only
  checked the vertical fit and a `<= 1260` on the TRACKS Roll pill that passed by 1 pt.
- The FX rack's track tags get a fixed 96 pt name slot: `right_aligned` pushes a group to
  the edge with *last* frame's width, so a head that grows with a longer name overflows for
  one frame, and because the page's vertical scroll area then widens for the next frame,
  everything below shifts by the growth for a frame (that is how the TRACKS DEL test missed
  its pill). A fixed slot keeps the heads' width constant across names.
- Live render vs export (the reviewer's test): the engine smooths parameter changes
  (mixer gains, reverb, the globals a preset carries, ~10-50 ms) where `render_loop` starts
  from `Project::apply_to`'s snapped values, so a live engine only matches the export once
  its smoothers have settled, and pre-rolling it to settle them desynchronises the drums'
  free-running noise generators (a 0.4 difference at the first hit). The test therefore
  compares a state whose smoothed settings are at their defaults (two tracks, notes,
  routing), bit-exact within 1e-5; a "snap" message would let the app match exactly after
  a load, and is an engine-side item.
- SYNTH panel column gap minimum 8 -> 4 (the scale's step): the design cleanup's 12 pt
  A D S R gap had made the first row 5.3 pt wider than the panel at 1280, which the top row
  tests only bounded with `<=`; they assert equality now.

## 2026-10-25: M11 engine (FL-style mixer inserts) as built
- Migration from v7 and older MOVES each strip onto its insert (level, pan, mute, solo,
  sends; strip i -> insert i+1, source routed one-to-one, slots off, EQ flat, route master)
  and resets the strip to neutral, rather than copying: copying would double level, pan and
  sends. The pre-M11 render is pinned bit-identical (`tests/fixtures/v7_demo.json`).
- Default sends now live on the inserts (2 / 3 reverb 0.3, 6..13 delay and reverb 1.0); the
  per-source strips default neutral and keep working as FL's channel-rack level / pan plus a
  legacy pre-insert send. The UI exposes the insert sends.
- Insert numbers are 1-based; `0` means master as a route and wraps to 16 as an address
  (like `track_index` and `pattern_index`), so a bad message never panics. A route may only
  point to MASTER or a higher-numbered insert (feedback impossible by construction).
- Choosing a slot kind resets its params to that kind's defaults and restarts the effect
  (hard cut, like loading a plugin). Drive is one instance for both channels; lo-fi and
  chorus run L / R instances.
- Compressor: instant-attack / exponential-release peak detector with the attack applied to
  the gain reduction, so a -3 dBFS sine over a -18 dB threshold at 4:1 lands at -14.25 dBFS.
- Insert solo and source solo are independent layers; mute beats solo on each.

## 2026-10-25: M11 mixer (app)
- **Insert names live in the app and ride in a JSON sidecar.** The engine's `Project` has
  no field for names or colours and the engine was off limits, so the app splices an
  `insert_ui` key into the engine's JSON on save and reads it back on load (`serde`
  ignores unknown keys; engine-written files load with the default names). The
  alternative, eframe storage keyed by path, would have lost the names with a moved file.
- **Two visible inserts, four visible sources, everything else scrolls.** The panel has
  436 x ~303 pt. A strip with a name, three slot pills, six labelled `Sm` knobs, a route
  pill and a fader needs 108 pt of width, so the inserts dock shows two at a time; the
  source strips are 36 pt (one `Sm` knob wide) and show four. The alternative of a
  vertically scrolling mixer would have hidden the faders.
- **A slot's knobs replace the strip's EQ / send grid** (3 x 2, the compressor's six
  parameters exactly) instead of opening a pop-out window: no room under the strip, and a
  deferred viewport per slot would have meant 48 plugin kinds in `plugins.rs`. The slot's
  pill stays lit while its knobs are shown and the edit square toggles back.
- **`fader_sized` in the kit.** The 140 pt fader plus its caps label did not fit under the
  blocks above it; the insert fader takes the room left (about 88 pt of track at the design
  size) and names itself in the head.
- **The picker is a number grid, not a name list.** Seventeen 30 pt pills needed a scroll
  area; four columns of 28 pt squares (names in tooltips) fit any popup and read like FL's
  number box.
- **Source strips show no meters and no sends.** The sends moved onto the inserts with the
  engine's defaults; the per-source meter had no room beside two knobs at 36 pt, and the
  insert meters are what a mix engineer watches.
- **A freed track slot forgets its route; a duplicate keeps the source's.** FL's clone keeps
  the mixer track; a deleted track's number box would otherwise point a later track at an
  old group.
- **`Project::from_json` runs on a 16 MiB helper thread** (`parse_project`): the
  deserialiser needs ~1.6 MiB of stack in debug builds, more than the 1 MiB main thread
  and within a few KiB of the 2 MiB test threads before M11. The same pattern as
  `new_engine`.
- **`HINT` lost its Ctrl+T mention** to make room for the LINK button beside the move
  arrows (the ADD tooltip still names the shortcut). `Ctrl+L` is checked last in the
  shortcut order so nothing above it changes.

## 2026-10-25: M12 shell (app)
- **The tab strip spans the workspace, not the main window, and is always shown.** WINNER
  grows it only at narrow, under the main window's header; but with the browser open and
  the rack at its 664 default the main window is 372 wide at 1280 x 800, too narrow for five
  tabs with keycaps. Across the rack + main width it always fits, the F-keys are discoverable
  at every size, and the collapsed rack's header sits under it like FL's view bar.
- **VISUALIZER is a fifth view**, not SONG's side window: at 1280 x 800 the SONG window plus
  a 340 wide visualizer beside it leaves the song table 28 points; M14 revisits when the
  timeline lands. F11 covers the workspace as WINNER says.
- **Draw | Select and the snap dropdown are not drawn.** The roll's selection is Ctrl-click /
  Ctrl-drag (a real "Select" tool needs `PianoRollModel` support, M13's roll restyle) and the
  only snap is the 1/16 grid; a one-entry dropdown and a segmented that changes nothing would
  be fake controls. The toolbar hint says "Ctrl+drag selects".
- **Rack "close" collapses.** A hidden rack would need a second flag and a place to bring
  it back from; the collapsed header is 28 points and F5 / the chevron re-expand it.
- **Row operations live in a right-click menu on the NAME button** (FL's channel button),
  with the DELETE confirm in the add row: WINNER's rack has no per-row buttons for
  duplicate / delete / move / colour / link, and a visible toolbar for the selected row would
  cost a 36 pt row at narrow.
- **A track's lane cells show its own notes on every row** (`lane_cell` derives them), and a
  click on another track's cell selects that track first; the engine's derived lane only
  covers the edit track.
- **The LEN segmented doubles / halves step by step** so a 1 -> 4 bar change still copies the
  loop into the new bars (the QOL behaviour); `set_pattern_length` alone would leave them
  empty.
- **"Test tone" moved into the project menu**: the toolbar has no room and it is a
  diagnostic, not a transport control.
- **The transport's mini visualizer strip is gone**: the Deck toolbar has no slot for it.
- **The keyboard's octave readout is "Keyboard octave"** and the toolbar's unsaved dot is
  "Unsaved dot": "Octave" is a chord of the CHORDS column and kittest folds two same-named
  label nodes into one.
- **The category chip "Bass" reads "EDM Bass"**: the "Bass" preset entry is a button of the
  same list.
- **Narrow defaults apply once per launch** (`narrow_applied`): the first frame at or under
  1000 points collapses the browser and the keyboard; the user may open them again, and an
  open keyboard then floats over the workspace instead of pushing the rack up.
- **`MASTER_WIDTH` 52 -> 60 in the M11 mixer**: the Deck labelled knob block is 56 wide.
  The inserts dock loses 8 points and still shows two strips.
- **`mixer_drawer_open` is derived from the view** in M12 (sanitised on load): the real drawer
  that pushes the workspace up is M13; keeping the flag now means the layout file does not
  change shape then.
- **The hint bar stays beside the toolbar's hint panel**: WINNER folds the status line into
  the toolbar, the brief asks for a bottom hint bar with the audio line; the toolbar panel
  shows the status / default hint, the bar the keycap pairs.

## 2026-10-27: Scope is now "open-source audio toy with full-DAW ambitions"
The user: "I want to build an opensource audio toy in Rust that has all the bells and whistles
of a full app like FL or Ableton. A just for fun project to make beats and have fun." So:
- The "first beat" scope and its out-of-scope list (docs/PLAN.md) are superseded. Piano roll,
  mixer and MIDI were already built; arrangement, recording, audio clips, automation and plugin
  hosting are now allowed future work, listed in README.md "Roadmap". Each still needs its own
  dated brief in docs/PLAN.md before a loop starts it; M13 and M14 stay the current target.
- **License: `MIT OR Apache-2.0`** (the Rust ecosystem's usual pair, and in line with the
  permissive-only dependency policy in deny.toml). `LICENSE` (proprietary) is replaced by
  `LICENSE-MIT` (copyright Madison Williams, 2026, the name from 0e0c842) and
  `LICENSE-APACHE` (the Apache-2.0 terms copied verbatim from the serde crate's
  `LICENSE-APACHE`). The workspace manifest uses
  `license = "MIT OR Apache-2.0"` and the three crates inherit it.
- README.md is now a public-facing readme with a usage guide written against the M12 Deck
  shell (gestures checked in the code, not the older summaries).
- Making the GitHub repo public is left to the user (outward-facing; the brief's "never make
  the repo public" rule still binds the agent).
