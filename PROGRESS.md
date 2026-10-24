# PROGRESS

**Current milestone:** Phase 0 (repo + design previews). Waiting on design pick.

## Phase 0: Repo + design previews
- [x] git init, README.md, .gitignore, LICENSE
- [x] Private GitHub repo created and pushed
- [x] State files: PROGRESS.md, DECISIONS.md, BLOCKERS.md, docs/PLAN.md
- [ ] design/preview-a.html
- [ ] design/preview-b.html
- [ ] design/preview-c.html
- [ ] design/index.html
- [ ] Commit and push "Phase 0: design previews"
- [ ] User picks winner -> design/WINNER.md (gate for any Rust code)

## M0: Skeleton
- [ ] Cargo workspace (engine, neu, app) builds
- [ ] App window opens in winning theme, OS light/dark detection + remembered toggle
- [ ] engine: Engine::render offline; test tone
- [ ] app: cpal output stream plays test tone via button
- [ ] neu: theme tokens from WINNER.md, raised/inset dual-shadow painter, panel, button
- [ ] --smoke-test CLI flag (about 60 frames, exit 0)
- [ ] deny.toml license allowlist, cargo-deny passes
- [ ] Gate green, tag m0

## M1: Keyboard piano
- [ ] Key map A W S E D F T G Y H U J K O L P ; (about 1.5 octaves), Z/X octave shift
- [ ] Fixed velocity, adjustable in UI
- [ ] Held-key tracking ignores OS key repeat (test)
- [ ] Polyphonic 8-16 voices, voice stealing, ADSR (tests)
- [ ] On-screen piano keys light with accent while held, clickable
- [ ] Gate green, tag m1

## M2: Synth + effects
- [ ] 2 band-limited oscillators (PolyBLEP: sine/saw/square/triangle), detune, mix
- [ ] Lowpass SVF (cutoff, resonance), filter env amount
- [ ] Amp ADSR
- [ ] LFO (rate, depth, target pitch/cutoff)
- [ ] FX chain: drive -> BPM-synced delay -> Freeverb-style reverb -> master gain + soft limiter
- [ ] Neumorphic knobs for all params, double-click reset
- [ ] 6-10 presets in dropdown
- [ ] Pitch, NaN/peak, delay-sync tests
- [ ] Gate green, tag m2

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
Not run yet (no Rust code exists in Phase 0).

## Next action
Spawn 3 design-preview subagents, write design/index.html, commit, push, stop and wait for
the user's pick.

## Later (ideas beyond M3, do not start)
- MIDI hardware input (optional stretch only after M3 is green)
