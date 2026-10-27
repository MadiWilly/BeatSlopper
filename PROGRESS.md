# PROGRESS

**Scope (2026-10-27):** open-source audio toy growing toward a full DAW; see docs/PLAN.md "Scope update". **Current milestone:** Phase 5 UI overhaul (design B "Deck" picked 2026-10-25; M12-M14); M8-M11 built, m11 tag pending its smoke test; see docs/PLAN.md "Phase 4 brief". M0-M7 tagged. Phase 3 brief: docs/PLAN.md. M0-M3 tagged. Phase 2 brief: docs/PLAN.md "Phase 2 brief" (M4-M7). design/WINNER.md is the token source of truth.

## Phase 0: Repo + design previews
- [x] git init, README.md, .gitignore, LICENSE
- [x] Private GitHub repo created and pushed
- [x] State files: PROGRESS.md, DECISIONS.md, BLOCKERS.md, docs/PLAN.md
- [x] design/preview-a.html ("Studio Hardware")
- [x] design/preview-b.html ("Soft Minimal")
- [x] design/preview-c.html ("Synth Module")
- [x] design/index.html
- [x] Commit and push "Phase 0: design previews"
- [x] Round 1 previews (archived in design/round1/, A had been picked)
- [x] Round 2 previews against the user's reference image: A "Soft Console", B "Glow Grid", C "Hush Console" + index.html
- [x] User picked round-2 A ("Soft Console") -> design/WINNER.md written (Rust gate open)

## M0: Skeleton (done 2026-10-24, tag m0)
- [x] Cargo workspace (engine, neu, app) builds
- [x] App window opens in winning theme, OS light/dark detection + remembered toggle
- [x] engine: Engine::render offline; test tone
- [x] app: cpal output stream plays test tone via button
- [x] neu: theme tokens from WINNER.md, raised/inset dual-shadow painter, panel, button
- [x] --smoke-test CLI flag (about 60 frames, exit 0)
- [x] deny.toml license allowlist, cargo-deny passes
- [x] Gate green, tag m0

## M1: Keyboard piano (done 2026-10-24, tag m1)
- [x] Key map A W S E D F T G Y H U J K O L P ; (about 1.5 octaves), Z/X octave shift
- [x] Fixed velocity, adjustable in UI
- [x] Held-key tracking ignores OS key repeat (test)
- [x] Polyphonic 16 voices, voice stealing, ADSR (tests)
- [x] On-screen piano keys light with accent while held, clickable
- [x] Gate green, tag m1

## M2: Synth + effects (done 2026-10-24, tag m2)
- [x] 2 band-limited oscillators (PolyBLEP: sine/saw/square/triangle), detune, mix
- [x] Lowpass SVF (cutoff, resonance), filter env amount
- [x] Amp ADSR
- [x] LFO (rate, depth, target pitch/cutoff), glide
- [x] FX chain: drive -> BPM-synced delay -> Freeverb-style reverb -> master gain + soft limiter
- [x] Neumorphic knobs/dials/sliders for all params, double-click reset
- [x] 8 presets, switchable from a segmented selector (the mock uses a segmented row, not a dropdown)
- [x] Pitch, NaN/peak, delay-sync tests
- [x] Gate green, tag m2

## M3: First beat (done 2026-10-24, tag m3)
- [x] Transport: play/stop, BPM 60-200, swing, 16-step loop, sample-accurate scheduling (test)
- [x] Procedural drum kit: kick, snare, clap, closed hat, open hat; per-row volume and tune
- [x] Step grid: 5 drum rows + 1 synth lane with per-step note
- [x] Note entry: click step + press key; record-arm overdub quantized
- [x] Accent/velocity per step; clear-row, clear-all
- [x] Save/Load .beatslopper.json (round-trip test)
- [x] Export loop to WAV (1/2/4/8 repeats) via offline renderer (test)
- [x] UI tests (egui_kittest): theme toggle, step toggle, key press state, both themes render
- [x] Reviewer pass: real-time rules + neumorphism fidelity vs WINNER.md (findings fixed or recorded)
- [x] Gate green, tag m3, final summary written here

## Last gate result
2026-10-25 M10 (tracks): all green (fmt, clippy -D warnings, 787 tests, deny licenses ok,
release build + smoke exit 0 from a detached worktree because the running app locks
target/release/beatslopper.exe).

## Next action
Orchestrator took over 2026-10-25 (all side-session branches merged). 1) Finish the M8 engine
wiring (synth insert, ducker, tests), gate, tag m8. 2) M10 instrument tracks (Phase 4 brief).
3) M9 presets + per-track browser. 4) M11 FL-style mixer. Re-merge `madi/test-feedback-deb72b`
whenever it gains commits.

## M4: Polish + MIDI input
- [x] engine: second lock-free queue for MIDI (MidiHandle), drained in render
- [x] app: midir input thread, port selector + lamp, persisted port, note on/off to engine + UI
- [x] app: undo/redo (Ctrl+Z/Y) for pattern and params
- [x] app: shortcuts (Space, R, Ctrl+S/Shift+S/O/E/N), unsaved marker, discard confirm
- [x] app: window opens at requested size at 100/125/150 % scaling (stale persisted size was the cause; first-frame resize + work-area clamp)
- [x] neu: tooltips on knobs/dials/sliders, brand wordmark tracking
- [x] Tests per PLAN.md M4; gate green, tag m4

## M5: Patterns + song mode
- [x] engine: 8 patterns, lengths 16/32/64, queued switch at bar, chain with repeats, v3 migration
- [x] app: pattern tabs, copy/paste/clear, length selector, song/pattern mode, chain editor
- [x] export: song or pattern x N
- [x] Tests per PLAN.md M5; gate green, tag m5

## M6: Mixer view
- [x] engine: six channels with level/pan/mute/solo/sends, meters via atomics, v4 migration
- [x] app: mixer panel with faders, pan, mute/solo, sends, meters
- [x] Tests per PLAN.md M6; gate green, tag m6

## M7: Piano roll
- [x] engine: polyphonic note list per pattern, exact on/off scheduling, v5 migration
- [x] app: piano roll panel (draw, move, resize, delete, accent, zoom), synced with the grid
- [x] Tests per PLAN.md M7; gate green, tag m7, final summary updated

## M8: Synthesis extensions (EDM + lofi tools; done 2026-10-25, tag m8)
- [x] engine: new ParamIds (pulse, unison 1-7, sub, noise, FM, pluck mode, filter type + ADSR, key track, velocity, pitch env, lo-fi, chorus, sidechain); project v6 (bumped by the plugins branch)
- [x] engine: oscillator/FM/noise/pluck DSP, filter types, filter ADSR, pitch env, velocity, unison
- [x] engine: lo-fi insert, chorus, kick-triggered ducker on the synth channel
- [x] app: synth panel and FX rack groups for the new parameters
- [x] Tests per PLAN.md M8; gate green, tag m8 (2026-10-25)

## M9: Preset library + browser (EDM + Joji-style lofi)
- [x] engine: 80+ categorised presets with tags, search helpers, per-preset tests (91 presets, 2026-10-25)
- [x] app: preset browser (category, list, search, prev/next, modified marker, persisted name; 2026-10-25, uncommitted: the loop re-runs the gate and commits)
- [ ] Tests per PLAN.md M9; gate green, tag m9, final summary updated

## M10: Instrument tracks (layer synths into rolls; done 2026-10-25, tag m10)
- [x] engine: `track.rs` (8 tracks: own voice pool, per-track params, stereo insert chain), per-track note lists in every pattern, edit-track routing, lifecycle messages, 13 mixer channels, project v7 with v6 migration (sample-identical render)
- [x] app: TRACKS panel (list, add/dup/del/rename/reorder, M/S, ROLL), synth panel + FX insert groups follow the selected track, roll ghosts, mixer strips per track, undo, shortcuts
- [x] Tests per PLAN.md M10 (incl. the release perf test); gate green, tag m10 (2026-10-25)

## M11: FL-style mixer (inserts, effect slots, routing)
- [x] engine: 16 inserts + master, source -> insert numbers, EQ3 + compressor DSP, 3 effect slots, route-to higher inserts, project v8 with v7 migration
- [x] app: FL mixer layout (scrollable inserts, slot pills, EQ3, route selector, rename/colour, link to new insert), insert number boxes in TRACKS and drum rows, undo (2026-10-25; docs/ARCHITECTURE.md "M11 as built (app)")
- [x] Tests per PLAN.md M11 (863 workspace tests); full gate green 2026-10-25 (release, smoke, perf), tag m11 at b1486f0

## M10 review follow-ups (reviewer pass 2026-10-25; no blockers)
App side (run after the M9 browser lands, before tagging m9):
- [x] remove_track resets the slot's mixer strip to defaults (a deleted soloed track silenced everything); test: solo track 2, delete it, track 1 still meters
- [x] TRACKS row gains the brief's small level knob (ChannelParam::Level) after S
- [x] drop the eframe PRESET_INDEX_KEY start-up re-apply (the project carries the preset per track; the browser persists a name)
- [x] selected TRACKS row gets an accent cue (3 pt accent bar at the left edge or on-glow chip ring), per the design-cleanup rule
- [x] sequencer head shows chip AND name at 1280 (move the tag into the GRID | ROLL row or shorten the right group; seed the measured width so the first frame does not overflow)
- [x] clipboard reconciled on load/undo (notes on inactive slots dropped); optionally clear notes in set_track_active(on)
- [x] mixer MASTER pinned outside the horizontal scroll area
- [x] tracks.rs: drop the import-suppression dead code; side padding on the 4/8/12 scale; row well inset-m
- [x] app tests: MIDI overdub with edit_track != 0; paste carries per-track notes; clipboard after remove/move; export of a 2-track project matches the live render
Engine side (after the M11 engine merge, in tests/tracks.rs + engine_alloc.rs):
- [x] v6-style fixture with non-default insert params pinned bit-identical (`tests/fixtures/v8_layered_demo.json`, `v8_layered_project_renders_sample_identical`); sum-of-tracks test variant with lo-fi/chorus on track 2 only (`wet_insert_on_track_two_keeps_the_tracks_additive_and_isolated`)
- [x] assert!(handle.send(..)) in the queue-driven tests (tracks.rs, engine_alloc.rs, plugins.rs; a full queue now fails the test)
- [x] document or drop the params reset in set_track_active(off): kept (the app's remove_track and the existing `set_track_active_off_clears_its_notes_in_every_pattern` test rely on it); the ordering dependency is now in both doc comments and ARCHITECTURE.md "M10 as built (engine)"

## Visualizers (side branch `madi/visualizers-bc3bf5`, 2026-10-25)
- [x] engine: lock-free master-output tap (`scope.rs`: `ScopeTap` / `ScopeReader` /
  `ScopeWindow`) and a latched drum-hit bitmask (`EngineHandle::take_hits`), three lines in
  `render`; tests for round trip, overflow and hit bits
- [x] neu: `visualizer` module: scope, bars, radial, vector, waves, particles with
  `VisualInput` / `VisualStyle`, the 7-entry `PRESETS` table; kittest renders in both themes,
  frame-time sanity; a VISUALIZERS panel in the gallery example
- [x] app: `spectrum.rs` (hand-written 2048-point FFT, 64 log bands, smoothed), the VISUAL
  tab in the right column (preset stepper, FULL), the transport mini strip, the full overlay
  (Escape / CLOSE), persisted preset; tests for strip, panel fit, overlay, persistence, feed
- Shared files touched (for the merge): `engine/src/engine.rs` (tap + hits hooks),
  `engine/src/lib.rs`, `neu/src/lib.rs`, `app/src/app.rs`, `app/src/lib.rs`,
  `app/src/ui/{mod,controls,transport_bar}.rs`, `app/tests/ui_renders.rs`,
  `neu/examples/gallery.rs`, docs

## Later (ideas; README.md "Roadmap" has the full list, each needs a brief in docs/PLAN.md first)
- Audio clips + sampler, audio/mic recording, arrangement timeline, session view, automation, VST3/CLAP hosting, MIDI import/export, stem export (scope opened 2026-10-27, see DECISIONS.md)

## Final summary (M3 done, 2026-10-24)

### How to run
```bash
cargo run --release -p app
```
The window opens at 1280x1140 (minimum 1240x760, scrolls below that) in the OS theme; the
toggle at the top right switches light/dark and remembers a manual choice. `--smoke-test`
opens, renders 60 frames and exits 0. Full gate: `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
`cargo deny check licenses`, `cargo build --release`, `cargo run --release -p app -- --smoke-test`.

### Key map
- Piano: `A W S E D F T G Y H U J K O L P ;` = C C# D D# E F F# G G# A A# B C C# D D# E,
  starting at the octave shown in the KEYS panel (default 4, so `A` = C4, `H` = A4 = 440 Hz).
- `Z` / `X`: octave down / up (1..=7). OS key repeat is ignored; losing focus releases notes.
- Keys can also be clicked on screen. Velocity knob in the KEYS panel (default 80 %).
- Sequencer: click a step to toggle it; shift-click for accent. Bass lane: click a step to
  select it, then play a key to set its note; shift-click steps the accent; Delete/Backspace
  clears the selected step; Escape deselects. Record (REC) + Play: notes you play are
  quantised into the bass lane.

### What works
- Transport: Play / Stop / Record, BPM 60..=200, swing 0..=75 %, 16-step loop with
  sample-accurate scheduling in the audio thread; playhead shown on the grid.
- Synth: 2 PolyBLEP oscillators (sine/saw/square/triangle, detune, mix), TPT lowpass SVF
  with cutoff, resonance and envelope amount, amp ADSR, LFO (rate, depth, pitch/cutoff),
  glide, 16 voices with stealing; 8 presets (Bass, Sub, Lead, Pluck, Pad, Keys, Acid, Bell).
- FX: drive, BPM-synced delay (1/16, 1/8, 1/4, 1/2, feedback, mix), Freeverb reverb (size,
  damping, mix), master gain with a switchable soft limiter.
- Drums: procedural kick, snare, clap, closed hat, open hat (closed chokes open), per-row
  volume and tune; no sample files.
- Project: New / Open / Save / Save as (`.beatslopper.json`, version 2, lossless round trip);
  Export WAV of 1/2/4/8 bars plus a 2 s tail, 16-bit stereo, rendered offline on a worker thread.
- Every control is a neumorphic widget from the `neu` crate built from `design/WINNER.md`:
  panels, pills, circle buttons, knobs, concentric dials, segmented selectors, vertical sliders,
  step cells, piano keys, lamps, theme toggle.
- Tests: 303 across engine (offline renders), neu (egui_kittest) and app (kittest + unit).

### Known issues / notes
- egui's bundled font is wider than the mock's system font, so some rows are tighter than the
  mock (row-label column 64 instead of 80, rows 4 apart); see WINNER.md "As-built tweaks".
- The window may open smaller than requested at 150 % display scaling on some setups
  (observed with the widget gallery); the app scrolls in that case.
- Synth lane is monophonic; a repeated pitch under heavy swing retriggers legato.
- The bass lane has no tune knob; use the preset or the synth panel.
- If the audio stream fails after start the transport shows "AUDIO STREAM ERROR" (sticky flag);
  there is no device picker (default output only).
- MIDI hardware, piano roll, arrangement, recording audio, mixer and VST hosting are out of
  scope by design.

## Phase 2 summary (M4-M7 done, 2026-10-25)

### What was added
- **M4 Polish + MIDI:** MIDI keyboard input (midir) through its own lock-free queue with a port
  selector and lamp; undo/redo (Ctrl+Z / Ctrl+Y) for every edit with drag coalescing; shortcuts
  (Space play/stop, R record, Ctrl+S / Ctrl+Shift+S / Ctrl+O / Ctrl+E / Ctrl+N); unsaved marker
  in the title and project panel with an inline discard confirm; value tooltips on every knob,
  dial and slider; the brand wordmark tracking; the window re-requests its size on the first
  frame (a stale persisted size was the cause of small windows).
- **M5 Patterns + song mode:** eight patterns A..H of 1, 2 or 4 bars; pattern switch queued at
  the bar boundary while playing (blinking letter); bar selector and follow; copy/paste/clear;
  Pattern | Song mode; a chain editor (entries with repeats, add/remove/move) in the right
  column's Chain tab; song export; project format v3.
- **M6 Mixer:** six channel strips (level fader with meter, pan, delay and reverb sends, mute,
  solo) plus master gain, limiter and stereo meter, as an FX | Mixer tab; parallel send buses
  with return levels; block-peak meters over atomics with UI decay; project format v4.
- **M7 Piano roll:** a Grid | Roll view of the pattern; polyphonic notes with length, pitch and
  velocity (draw, move, resize, select, delete, shift-click accent, wheel scroll); the grid's
  bass lane is a view of the same notes; exact note-off scheduling; project format v5 with
  migration from every earlier version.

### Numbers
- Tests at the end of phase 2: see "Last gate result". Tags: m0..m7.

### Known issues / notes (phase 2)
- Double-clicking an empty piano-roll cell adds a note on the first click and removes it on the
  second (documented quirk of the one-edit-per-frame rule).
- Hard-panning a channel is +3 dB in that speaker (unity-at-centre pan law; the limiter covers
  the headroom).
- The MIDI port list refreshes every 2 s while the selector is visible; a port that failed to
  open is not retried until re-selected or next launch.
- At large audio buffers the meters update about 12 times a second.
- Keyboard note entry into a selected grid step only works in GRID view; in ROLL view the
  keyboard plays (record-arm overdub still writes length-1 notes).
- No MIDI device was connected on the build machine, so live MIDI input was verified only by
  parser tests and the engine queue test.

## Plugins: EQ + reverb pop-out windows (side branch `madi/plugins-eq-reverb-1bffdb`, 2026-10-25)
- [x] engine: 7-band parametric EQ on the master bus (`fx/eq.rs`), 12 presets, response curve
- [x] engine: reverb pre-delay / width / wet low+high cut (`ReverbState`), 10 presets, impulse envelope, RT60
- [x] engine: lock-free master spectrum tap + FFT analyser (`spectrum.rs`); project v6 (`eq`, `reverb`)
- [x] app: deferred-viewport windows (EQ graph with draggable nodes + analyser; reverb envelope + wet meter), preset lists with MODIFIED lamp, PLUGINS pills in the FX / MIXER head, geometry persisted, shortcuts forwarded
- [x] tests: 12 engine offline tests, 9 kittest / state tests, unit tests; gate green on the branch
- [x] reverb audit (branch `madi/reverb-audit-087b74`, 2026-10-25): the rack's SIZE / DAMPING / MIX and the window's `ReverbState` never disagree (engine + app, proven by 4 engine and 2 app tests); `set_reverb_state` snaps its smoothers so exports honour the pre-delay from the first hit; MORE pill in the rack's REVERB group, "shared with the FX rack" hint in the window, MIX = return level / REV = send notes (see DECISIONS.md)

## Phase 5: UI overhaul (design B "Deck"; docs/PLAN.md "Phase 5 brief", design/round3/WINNER.md)
- [x] Previews A / B / C built (design/round3), user picked B "Deck" (2026-10-25)
- [x] M12 neu: Deck restyle, every widget + new shell widgets, 146 neu tests (3dd6203)
- [x] M12 app: shell (toolbar, browser column, channel rack, F-key views, mixer bar, keyboard strip, hint bar, work-area window sizing), tests/shell_layout.rs; gate green, 900 tests, tag m12 (2f0d6ac)
- [x] QOL rack ↔ mixer link (branch `madi/rack-mixer-link`, 2026-10-27): one rack focus (`AppState::rack_focus`: a drum row whose NAME was clicked, else the edit track) on the accent wash; moving it selects its insert in the MIXER and scrolls the source dock to its strip once (`mixer::reveal_offset`), a re-focus keeps a Shift-built selection; any insert selected alone (chips, pickers, heads, Ctrl+L) and opening the mixer scroll the inserts dock to it; the focused source strip is marked in the mixer and its caps label focuses it in the rack; INS boxes light while their insert is selected; the mixer bar lights the highlighted insert's chip (adding it past 5) and a chip click opens the mixer on it; 908 tests
- [ ] M13 first: at 853 logical points (the user's 150 % case with a persisted 1280x1140 physical window) the toolbar overflows on the right and the main window sits behind the rack: the narrow layout (<= 1000) must engage from the persisted size too, and `initial_inner_size` should grow a too-narrow persisted window to the work-area rule; the TRACKS header's "VOL PAN" label overlaps the column labels (see design/round3/captures/m12-deck-shell.png)
- [ ] M13: real MIXER drawer (push-up, side column, Deck strips), SYNTH & FX Deck groups, roll Draw | Select + snap
- [ ] M14: SONG timeline, browser polish, plugin windows restyle, keyboard strip, retire the Soft Console aliases in neu, captures under design/round3/captures, tag

## Sound Pack + guitar level fix (branch `madi/synth-pack`, 2026-10-27)
- [x] engine: plucked-string bursts level-normalised and soft-clipped; Osc 1 mix is the string level in pluck mode; lofi guitars now at the pack's -18 dB (were -34 dB on average)
- [x] engine: 1 000-preset Sound Pack in ten new categories, generated by `examples/gen_preset_pack.rs` and checked by rendering (docs/PRESET_PACK.md); `LIBRARY` = core + pack
- [x] app: browser chips for the ten categories, off-screen rows skip tag and tooltip work
- [x] tests: per-category render/decay checks for the pack, pack shape, levelling, guitars-vs-keys loudness, pluck level consistency; engine optimised in dev/test builds

## Last gate result (Phase 5)
2026-10-25 M12: all green (fmt, clippy -D warnings, 900 tests, deny licenses ok, release build + smoke from the detached worktree), tag m12.
