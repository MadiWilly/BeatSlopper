# PROGRESS

**Current milestone:** M8 (Synthesis extensions) for an EDM + Joji-style lofi preset library (M9). M0-M7 tagged. Phase 3 brief: docs/PLAN.md. M0-M3 tagged. Phase 2 brief: docs/PLAN.md "Phase 2 brief" (M4-M7). design/WINNER.md is the token source of truth.

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
2026-10-25 M7 (final, after review fixes): all green (fmt, clippy -D warnings, 492 tests, deny licenses ok,
release build, smoke exit 0).

## Next action
M8: params subagent adds the ids; then engine-synth (osc extras, FM, pluck, filter ADSR, pitch env,
velocity) | engine-fx (lo-fi, chorus, ducker) | app (new panel groups) in parallel; gate, tag m8.
Then M9: presets subagent (80+ EDM/lofi presets) + app browser; gate, tag m9.

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

## M8: Synthesis extensions (EDM + lofi tools)
- [x] engine: new ParamIds (pulse, unison 1-7, sub, noise, FM, pluck mode, filter type + ADSR, key track, velocity, pitch env, lo-fi, chorus, sidechain); project v6 bump pending
- [ ] engine: oscillator/FM/noise/pluck DSP, filter types, filter ADSR, pitch env, velocity, unison
- [ ] engine: lo-fi insert, chorus, kick-triggered ducker on the synth channel
- [ ] app: synth panel and FX rack groups for the new parameters
- [ ] Tests per PLAN.md M8; gate green, tag m8

## M9: Preset library + browser (EDM + Joji-style lofi)
- [ ] engine: 80+ categorised presets with tags, search helpers, per-preset tests
- [ ] app: preset browser (category, list, search, prev/next, modified marker, persisted name)
- [ ] Tests per PLAN.md M9; gate green, tag m9, final summary updated

## Later (ideas beyond M9, do not start)
- MIDI hardware input (optional stretch only after M3 is green)

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
