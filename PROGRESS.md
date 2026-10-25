# PROGRESS

**Current milestone:** M3 (First beat). M0, M1, M2 tagged. design/WINNER.md is the token source of truth.

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

## M3: First beat
- [ ] Transport: play/stop, BPM 60-200, swing, 16-step loop, sample-accurate scheduling (test)
- [ ] Procedural drum kit: kick, snare, clap, closed hat, open hat; per-row volume and tune
- [ ] Step grid: 5 drum rows + 1 synth lane with per-step note
- [ ] Note entry: click step + press key; record-arm overdub quantized
- [ ] Accent/velocity per step; clear-row, clear-all
- [ ] Save/Load .beatslopper.json (round-trip test)
- [ ] Export loop to WAV (1/2/4/8 repeats) via offline renderer (test)
- [ ] UI tests (egui_kittest): theme toggle, step toggle, key press state, both themes render
- [ ] Reviewer pass: real-time rules + neumorphism fidelity vs WINNER.md
- [ ] Gate green, tag m3, final summary written here

## Last gate result
2026-10-24 M2: all green (fmt, clippy -D warnings, 203 tests, deny licenses ok, release build, smoke exit 0).

## Next action
M3 per docs/ARCHITECTURE.md "M3 additions": write pattern.rs + M3 messages + playhead atomics + step_cell
stub + rfd dep, then spawn engine-sequencer | engine-drums | engine-project+export | neu step_cell |
app transport+sequencer+save/load/export; integrate DrumKit into render; gate; final summary.

## Later (ideas beyond M3, do not start)
- MIDI hardware input (optional stretch only after M3 is green)
