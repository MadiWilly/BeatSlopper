# Architecture and crate interfaces

Companion to `docs/PLAN.md`. This file records the interfaces the crates agree on so that
subagents can work in parallel on disjoint crates. Update it whenever a public interface changes.

## Workspace

```
Cargo.toml            workspace (resolver 3, edition 2024), shared [workspace.dependencies]
deny.toml             cargo-deny license allowlist (permissive only)
crates/engine         pure DSP + sequencing, offline-renderable, no UI/audio-device deps
crates/neu            neumorphic egui widget kit + theme tokens (from design/WINNER.md)
crates/app            eframe app, cpal output, keyboard input, UI panels
```

Dependency direction: `app -> {engine, neu}`. `engine` and `neu` never depend on each other.
Only the orchestrator edits the root `Cargo.toml` and `deny.toml`. A crate may add a dependency
in its own `Cargo.toml` (prefer `workspace = true` entries; if a new crate is needed, add it with
a version in the crate manifest and mention it in the hand-back so it can be promoted).

## Audio data format

- Sample rate is whatever the device reports (tests use 48 000 Hz).
- `Engine::render(out, frames)` writes **interleaved stereo f32** into `out` where
  `out.len() >= frames * 2`. Samples are nominally in -1.0..=1.0 after the master limiter.
- The audio callback owns the `Engine` and calls `render` once per buffer. It never allocates,
  locks, does I/O, logs or panics. If the device is not stereo, the callback renders into a
  pre-allocated stereo scratch buffer and copies/mixes into the device buffer.

## UI -> engine messaging

- `engine::new_engine(sample_rate) -> (EngineHandle, Engine)`.
- `EngineHandle::send(Message) -> bool` pushes onto an `rtrb` SPSC ring buffer (UI side).
- `Engine::process_messages()` drains it at the start of every `render` (audio side).
- `Message` is a `Copy` enum. M0 variants: `TestTone(bool)`, `MasterGain(f32)`.
  Later milestones add `NoteOn { note: u8, velocity: f32 }`, `NoteOff { note: u8 }`,
  `SetParam(ParamId, f32)`, transport and pattern messages. Patterns and presets are sent as
  small `Copy` payloads, never heap data.
- Direct setters (`Engine::set_test_tone`, `Engine::set_master_gain`, ...) exist for offline
  rendering in tests and the WAV exporter.

## neu widget kit (M0)

- `ThemeMode { Light, Dark }` with `from_egui(egui::Theme)` and `toggled()`.
- `Theme::light() / dark() / for_mode(mode)`: all tokens from `design/WINNER.md`
  (`crates/neu/src/theme.rs` is the transcription). `Theme::apply(&egui::Context)` sets egui's
  global visuals (fills and text colours) so plain egui widgets sit on the surface colour.
- Painting primitives (`neu::shadow`): a `Depth` enum `{ RaisedL, RaisedM, RaisedS, InsetM,
  InsetS, OnGlow }` and `paint_soft_rect(painter, rect, radius, depth, &Theme)` /
  `paint_soft_circle(painter, center, radius, depth, &Theme)`. Raised = two outer shadow shapes
  (highlight at -offset, dark at +offset) under a surface fill. Inset = surface fill plus a
  simulated inner shadow (clipped shadow shapes or a few feathered inner strokes). OnGlow =
  accent fill plus the glow shadow and the dark shadow. Never more than two shadows per element.
- Widgets (M0): `panel(ui, &Theme, |ui| ..)` (raised-l, radius 24, padding 20),
  `button(ui, &Theme, label) -> Response` (pill, raised-m, inset while pressed),
  `toggle_button(ui, &Theme, label, on: bool) -> Response` (OnGlow when on),
  `icon_button(ui, &Theme, Icon, diameter) -> Response` (circle, raised-m),
  `theme_toggle(ui, &Theme, mode: &mut ThemeMode) -> Response` (50x28 pill, 22 knob).
- Later milestones add `knob`, `dial` (concentric rings), `slider` (thin vertical),
  `segmented`, `stepper`, `step_cell`, `piano_keys`.

## app (M0)

- `--smoke-test`: open the window, render about 60 frames, close with exit code 0. Audio
  device failure must not fail the smoke test (log to stderr and continue without sound).
- Theme: on first launch follow the OS (`ctx.system_theme()`, falling back to dark). A manual
  toggle sets an override that is persisted through eframe storage. The OS theme is not
  persisted; only an explicit toggle is (see the bug noted in `design/WINNER.md`).
- Window: title "BeatSlopper", default inner size 1280x1080, min 1240x760, surface-colour clear.
- Modules: `main.rs` (args, eframe run), `app.rs` (state + `update`), `audio_io.rs`
  (cpal stream wrapping the `Engine`), `input.rs` (keyboard, M1), `ui/` (panels).

## M1 additions (keyboard piano)

### engine
- `Message` gains `NoteOn { note: u8, velocity: f32 }`, `NoteOff { note: u8 }`, `AllNotesOff`,
  and `SetParam(ParamId, f32)`.
- `ParamId` (in `params.rs`, `Copy`, `#[non_exhaustive]`-free so the UI can match): M1 variants
  `AmpAttack`, `AmpDecay`, `AmpSustain`, `AmpRelease` (seconds / seconds / 0..1 / seconds) and
  `MasterGain`. M2 adds oscillator, filter, LFO and FX ids. `ParamId::default_value()`,
  `ParamId::range()` and `ParamId::label()` live with the enum.
- Voices (`voice.rs`): `MAX_VOICES = 16`, each with a sine oscillator (M2 swaps in the PolyBLEP
  synth), velocity, and an amp ADSR (`params::Adsr`, linear attack, exponential-ish decay and
  release, with a 1 ms minimum so there are never clicks). Voice stealing: a note already held
  retriggers its own voice; otherwise the oldest releasing voice, else the oldest held voice.
  Allocation-free; voices live in a fixed array.
- Direct setters mirror the messages: `Engine::note_on(note, velocity)`, `note_off(note)`,
  `all_notes_off()`, `set_param(id, value)`. Note numbers are MIDI (69 = A4 = 440 Hz).
- Mixing: voices sum, then master gain (smoothed), then the limiter, as in M0.

### neu
- `piano_keys(ui, &Theme, &PianoModel) -> PianoOutput`:
  `PianoModel { base_note: u8 (MIDI of the leftmost C), white_keys: usize (14 = 2 octaves),
  held: &[bool; 128], labels: &[Option<char>; 128] (computer key cap per note, uppercase, ';'
  stays ';') }`. Geometry from `design/WINNER.md`: white 78x100 r12 raised-m, black 44x62 r10
  (`theme.key_black` fill), gap 6, black keys drawn on top; a held key is OnGlow with on-accent
  text. Each key shows its computer-key cap (12/500 tracked) and note name (11/400 secondary,
  e.g. "C4"). `PianoOutput { pressed: Option<u8>, released: Option<u8> }` reports mouse
  down / up on a key (one event per frame is enough).
- `stepper(ui, &Theme, label: &str, value: &mut i32, range: RangeInclusive<i32>) -> Response`:
  the reference's "(-) 3 (+)" control: two 28 pt stepper buttons around a 13/400 tabular value
  with the caps label above. Returns a response whose `changed()` is true when a button fired.

### app
- `input.rs`: `KeyMap` (egui::Key -> semitone offset 0..=16 for A W S E D F T G Y H U J K O L
  P ;), `HeldKeys` (which mapped keys are physically down), `octave: i32` (default 4; note =
  (octave + 1) * 12 + offset, so A = C4
  = MIDI 60; Z/X shift within 1..=7). `InputState::handle_events(&[egui::Event]) -> Vec<NoteEvent>`
  where `NoteEvent::{On { note, velocity }, Off { note }}`; events with `repeat == true` are
  ignored, so holding a key never retriggers; a key-up for a key that was not down is ignored;
  losing focus releases everything. Velocity is a fixed value from the UI (default 0.8).
- `ui/keyboard.rs`: bottom panel with `neu::piano_keys` (base note = (octave + 1) * 12, 14 white keys),
  the octave `stepper` labelled with Z / X, and a velocity slider. Mouse presses on keys send
  the same `NoteEvent`s as the computer keyboard.

## M2 additions (synth + effects)

### engine: parameters
`ParamId` gains (unit, range, default):
- `Osc1Wave` (index 0 Sine, 1 Saw, 2 Square, 3 Triangle; 0..=3; default 1), `Osc1Detune`
  (cents, -50..=50, 0), `Osc1Mix` (0..=1, 0.7); `Osc2Wave` (default 0), `Osc2Detune` (0),
  `Osc2Mix` (0.45).
- `FilterCutoff` (Hz, 20..=20000, log scale, 2400), `FilterResonance` (0..=1, 0.22),
  `FilterEnvAmount` (-1..=1, 0.4; maps to +-4 octaves of cutoff driven by the amp envelope).
- `AmpAttack/Decay/Sustain/Release` (M1).
- `LfoRate` (Hz, 0.1..=20, 4.5), `LfoDepth` (0..=1, 0.25), `LfoTarget` (index 0 Pitch,
  1 Cutoff; default 1), `Glide` (s, 0..=1, 0).
- `DriveAmount` (0..=1, 0.35), `DriveMix` (0..=1, 0.5), `DelayTime` (index 0 1/16, 1 1/8,
  2 1/4, 3 1/2; default 1), `DelayFeedback` (0..=0.95, 0.35), `DelayMix` (0..=1, 0.25),
  `ReverbSize` (0..=1, 0.6), `ReverbDamping` (0..=1, 0.4), `ReverbMix` (0..=1, 0.2),
  `MasterGain` (M1), `LimiterOn` (0/1, default 1).
`ParamId::is_fx()` tells FX ids apart. `ParamId::ALL` keeps every id; `index()` is the
position in the engine's `params` array.

### engine: synth (`synth/`, `voice.rs`)
- `synth::Wave { Sine, Saw, Square, Triangle }` with `from_index`. `synth::Oscillator`: PolyBLEP
  band-limited saw/square (triangle = integrated square or PolyBLEP triangle), sine; `set_freq`,
  `tick()`.
- `synth::Svf`: Chamberlin/TPT state-variable lowpass, `set(cutoff_hz, resonance)`, `tick(x)`;
  cutoff clamped below Nyquist * 0.45; denormal-safe.
- `synth::Lfo`: sine, `set_rate`, `tick() -> -1..=1`; one LFO per engine (shared), target
  pitch (+-1 semitone * depth) or cutoff (+-2 octaves * depth).
- Each `Voice`: two oscillators (wave, detune in cents, mix), SVF filter, amp `Adsr`; filter
  cutoff per sample = base cutoff * 2^(env * amount * 4) * lfo modulation. Glide slews the
  pitch when a new note starts while another is held (0 = off).
- `Engine::set_param` routes synth ids to all voices (and the shared LFO), FX ids to the
  `FxChain`, `MasterGain` to the smoother, `LimiterOn` to the master stage.
- `presets.rs`: `Preset { name: &'static str, values: &'static [(ParamId, f32)] }`,
  `presets::ALL` (8 presets: Bass, Sub, Lead, Pluck, Pad, Keys, Acid, Bell), `Preset::apply(&mut
  Engine)` (unlisted params go to defaults). Presets are plain data so the UI can also apply
  them through `SetParam` messages; the UI sends `Message::SetParam` per value.

### engine: FX (`fx/`)
- `fx::FxChain::new(sample_rate)`: pre-allocates a 4 s stereo delay line and Freeverb buffers.
  `set_bpm(bpm)`, `set_param(ParamId, f32)` (ignores non-FX ids), `process(l, r) -> (l, r)`,
  `reset()`, `delay_seconds()` (current synced delay time), `delay_samples()`.
- Order: drive (tanh waveshaper, drive gain 1..=24 from amount, dry/wet mix) -> delay (time =
  division * 60 / bpm, feedback, mix; smoothed time changes to avoid clicks or a crossfade)
  -> reverb (Freeverb: 8 combs + 4 allpasses per channel, size -> comb feedback 0.7..=0.98,
  damping, mix) -> back to the engine's master gain + limiter.
- `Message::SetBpm(f32)` and `Engine::set_bpm(bpm)` (60..=200) exist from M2 so the delay can
  sync; M3's transport reuses them.

### neu: M2 widgets
- `knob(ui, &Theme, &mut f32, &KnobSpec) -> Response`. `KnobSpec { label: &str, min, max,
  default, log: bool, bipolar: bool, size: KnobSize::{Md, Sm}, fmt: fn(f32) -> String }`.
  Geometry per `design/WINNER.md`: cap 40 raised-s, arc ring 50 (track shadow-dark, value arc
  accent, 270 degrees from 7:30; bipolar arcs start at 12 o'clock), pointer 2x9 accent; label
  above (caps 11/400) and value below (13/400 tabular), block width 52 (Sm: cap 26, arc 34).
  Vertical drag, 150 px for the full range (shift = fine, 4x slower), double-click resets to
  `default`, arrow keys step when focused, `changed()` when the value moved. Log scale maps
  the drag linearly in log space.
- `dial(ui, &Theme, &mut f32, &KnobSpec, DialSize::{Lg, Sm}) -> Response`: the concentric-ring
  hero control (Lg: arc 96, rings 84/64/44, pointer dot 6; Sm: arc 72, rings 62/46/32, dot
  5). Same interaction as `knob`.
- `segmented(ui, &Theme, &mut usize, &[&str]) -> Response`: inset-m pill track (height 30,
  item padding 0 8), selected item in text-primary 12/500, others text-secondary 12/400.
- `vslider(ui, &Theme, &mut f32, &SliderSpec) -> Response`: thin vertical slider, track 6x100
  inset-s, 14 pt accent thumb with on-glow, 4 pt accent end markers, label below, value above;
  `SliderSpec { label, min, max, default, fmt }`; double-click resets.
- `lamp(ui, &Theme, on: bool, label: &str)`: 10 pt indicator (inset-s off, OnGlow on).

### app: M2 panels
- `AppState.params: [f32; ParamId::COUNT]` mirrors the engine; a change sends
  `Message::SetParam(id, value)`; `apply_preset(index)` sets every value and sends them all.
  `preset_index: usize` (persisted with the theme override).
- `ui/synth_panel.rs` (left, flexible width): preset `segmented` (8 names) top-right; Osc 1 /
  Osc 2 rows (wave `segmented`, detune and mix knobs); Filter (cutoff `dial` Sm, res and env
  knobs); Amp (A D S R `vslider`s); LFO (rate, depth knobs, target `segmented`).
- `ui/fx_rack.rs` (right, 476 px): Drive (amount, mix), Reverb (size, damping, mix), Delay
  (time `segmented`, feedback, mix), Master (gain knob, limiter `toggle_button` + `lamp`).
- The transport bar's master control becomes a `dial` Lg bound to `MasterGain`.
