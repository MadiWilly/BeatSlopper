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

### M2 as built (engine additions beyond the plan)
`ParamId::is_synth()`, `Engine::synth_params()`, `Engine::limiter_on()`, `Engine::fx()`,
`Engine::delay_samples()`, `Preset::value(id)`, `Wave::from_param`, `LfoTarget::from_param`.
Glide is the time to cover 95 % of a pitch jump. The test tone is summed in after the FX chain.

## M3 additions (first beat: transport, drums, sequencer, save/load, export)

### engine: pattern (`pattern.rs`, plain data, serde)
- `STEPS = 16`. `DrumRow { Kick, Snare, Clap, ClosedHat, OpenHat }` (exists in `drums`).
- `DrumStep { on: bool, accent: bool }`, `SynthStep { note: Option<u8>, accent: bool }`.
- `Pattern { drums: [[DrumStep; 16]; 5], synth: [SynthStep; 16], drum_volume: [f32; 5] (dB,
  -24..=6, default 0), drum_tune: [f32; 5] (semitones, -12..=12, default 0) }` with
  `Default`, `clear_row(row)`, `clear_synth()`, `clear_all()`, `is_empty()`, plus a
  `Pattern::demo()` (the mock's kick/snare/clap/hat pattern and a C2 bassline).
- Velocity: a normal step plays at 0.8, an accented step at 1.0.

### engine: transport and scheduler (`transport.rs`, `engine.rs`)
- `Transport { bpm (60..=200), swing (0..=0.75), playing }` exists; add the scheduler: a
  sample counter within the 16-step bar; step `n` starts at `n * samples_per_step` where
  `samples_per_step = sr * 60 / bpm / 4` (6000 at 120 BPM / 48 kHz, kept as f64 so the bar
  length is exact); swing delays every odd step by `swing * samples_per_step * 0.5`... exact
  definition: odd steps (1, 3, 5, ...) trigger at `(n + swing) * samples_per_step` with
  `swing` in 0..=0.75 (0.5 = classic triplet feel). Triggers are sample-accurate inside
  `render`: the loop checks, per sample, whether any step boundary falls on this sample and
  fires drum triggers / synth note on/off at that sample. Synth lane notes last one step
  (gate 90 % of a step, so repeated notes retrigger), with a `NoteOff` at the gate end.
- `Message` gains `Play`, `Stop` (also releases sequencer notes, resets position to 0),
  `SetSwing(f32)`, `SetDrumStep { row: DrumRow, step: u8, on: bool, accent: bool }`,
  `SetSynthStep { step: u8, note: Option<u8>, accent: bool }`, `ClearRow(DrumRow)`,
  `ClearSynth`, `ClearAll`, `SetDrumVolume { row, db }`, `SetDrumTune { row, semitones }`.
  Everything stays `Copy`. Whole-pattern loads (project load) go through
  `Engine::set_pattern(&Pattern)` directly before the stream starts, or as a burst of step
  messages from the UI (16 * 6 messages fit the 1024 queue).
- Playhead feedback to the UI without locks: `EngineHandle::position() -> Position { step:
  u8, frac: f32, playing: bool }` read from atomics (`AtomicU32` step + `AtomicU32` for the
  sample offset within the bar, `AtomicBool` playing) that `render` updates once per block;
  `EngineHandle` and `Engine` share them through an `Arc` created in `new_engine` (allocation
  happens there, never in `render`).
- Direct API mirrors the messages: `Engine::play()`, `stop()`, `set_swing()`, `set_pattern()`,
  `pattern() -> &Pattern`, `transport() -> &Transport`, `set_drum_step(..)`, `set_synth_step(..)`.
- Drums go through the FX chain together with the synth (one mono bus), per the mock.

### engine: drums (`drums/`)
- `DrumKit::new(sample_rate)`: five procedural voices, pre-allocated, `trigger(row, velocity)`
  (retrigger restarts the voice), `set_volume(row, db)`, `set_tune(row, semitones)`,
  `tick() -> f32` (mono sum), `is_active()`, `reset()`.
- Kick: sine with a fast exponential pitch sweep (about 150 Hz -> 50 Hz over 60 ms) plus a
  short click (noise burst 2 ms), amp decay about 350 ms. Snare: 180 Hz tone with decay 120
  ms plus band-passed noise decay 180 ms. Clap: 3-4 short noise bursts 10 ms apart through a
  band-pass, then a 150 ms tail. Closed hat: high-passed (6 kHz) metallic noise (sum of a few
  square oscillators or white noise), decay 60 ms. Open hat: same, decay 400 ms; triggering
  the closed hat chokes the open hat. Tune shifts the tonal parts by semitones (and the noise
  filter centre proportionally). Peak per drum about -6 dBFS at velocity 1.
- Noise source: a tiny xorshift PRNG inside the kit (no `rand` dependency).

### engine: project and export (`project.rs`, `export.rs`)
- `Project { version: u32 (1), bpm, swing, pattern: Pattern, params: Vec<(String, f32)>
  (ParamId label-independent names: use `ParamId::key()` -> `"osc1_wave"` style snake_case
  strings; unknown keys are ignored on load, missing ones take defaults), preset_index: usize,
  octave: i32, velocity: f32 }`. `Project::from_engine(&Engine, preset_index, octave,
  velocity)`, `apply_to(&mut Engine)`, `to_json_pretty()`, `from_json(&str)`,
  `save(path)`, `load(path)`; file extension `.beatslopper.json`. Round trip must be lossless.
- `export::render_loop_wav(project: &Project, repeats: u32 (1/2/4/8), sample_rate: f32,
  path) -> Result<ExportStats, ExportError>`: builds a fresh offline `Engine`, applies the
  project, plays exactly `repeats` bars (bar length = 16 * samples_per_step, rounded), renders
  in 512-frame blocks, then renders a 2 s tail with the transport stopped so delay/reverb
  decay, writes 16-bit stereo WAV with `hound`; `ExportStats { frames, peak, duration_seconds }`.
  A `render_loop(project, repeats, sr) -> Vec<f32>` helper does the same into memory for tests.

### neu: M3 widgets
- `step_cell(ui, &Theme, &StepCellSpec) -> Response`: 32 pt circle. `StepCellSpec { on,
  accent, current: bool (playhead column), label: Option<&str> (bass note name, 11/500), beat_start:
  bool }`. Off = raised-s surface; on = OnGlow accent (accent-hover fill when `accent` with a
  6 pt on-accent dot like the mock); `current` draws the playhead treatment (off: inset-s;
  on: accent-hover). Click toggles (the caller mutates). Shift-click reported via
  `response.clicked()` + `ui.input(|i| i.modifiers.shift)`.
- `playhead_dot(ui, &Theme, current: bool)`: 6 pt idle dot / 8 pt OnGlow dot (row under the
  grid in the mock). Also `beat_number` text via `neu::label`.

### app: M3
- Transport bar: Play / Stop / Record icon buttons (48), BPM `stepper` 60..=200 (bound to
  `SetBpm`), SWING knob Sm (0..=75 %), MASTER dial, theme toggle. Play toggles `Play/Stop`.
- `ui/sequencer.rs`: a "SEQUENCER" panel: for each drum row a label (caps), a Sm VOL knob
  (dB) and TUNE knob (st), 16 `step_cell`s in groups of 4 with beat numbers 1-4 above the
  first row and the playhead dots under the grid, a CLEAR pill per row and CLEAR ALL top-right.
  The BASS lane shows note names; click selects the step (highlight ring) and the next key
  pressed (computer or on-screen piano) sets its note; shift-click cycles accent; a selected
  step plus Delete/Backspace clears it.
- Record-arm overdub: while playing and armed, each NoteOn is quantised to the nearest step
  using `EngineHandle::position()` (step + frac, rounding frac >= 0.5 to the next step, wrapping
  at 16) and written to the synth lane as a `SetSynthStep` (accent if velocity >= 0.95).
- `AppState` keeps a `Pattern` mirror (source of truth for the UI) and sends step messages on
  edits. Playhead comes from `position()` each frame while playing (request_repaint).
- Save / Load / Export: pill buttons in the transport or a small "PROJECT" group: Save As and
  Open use the `rfd` crate (MIT) file dialogs with the `.beatslopper.json` filter; Export opens
  a save dialog for `.wav` after a `segmented` 1|2|4|8 repeats choice; export runs on a worker
  thread (`std::thread`) with a "Exporting..." lamp, never on the UI or audio thread. Errors
  show as a short status line in the panel (no modal).
- Persisted: last project path, repeats choice.

## M4 additions (polish + MIDI)

### engine
- A second SPSC queue for MIDI: `EngineHandle::take_midi_handle() -> Option<MidiHandle>` (once);
  `MidiHandle::send(Message) -> bool` is `Send` and lives on the MIDI input thread. The engine
  drains the UI queue, then the MIDI queue, at the start of every `render` (`apply_message`).

### app
- `midi.rs`: `MidiInput` built on `midir` 0.11: `list_ports() -> Vec<String>`, `connect(port
  name, MidiHandle, ui_tx: Sender<NoteEvent>) -> Result<MidiConnection, String>`, `disconnect`.
  The midir callback parses status bytes (note on/off on any channel, running status, note-on
  velocity 0 = off, everything else ignored), pushes `Message::NoteOn/NoteOff` to the
  `MidiHandle` (no allocation in the callback beyond midir's own buffer), and sends a
  `NoteEvent` copy to the UI through a `std::sync::mpsc` channel so the on-screen keys light,
  the bass-lane note entry and record-arm overdub see MIDI notes exactly like computer keys.
  `parse_midi(bytes, &mut RunningStatus) -> Option<NoteEvent>` is a pure function with tests.
- Transport/MIDI panel: a `segmented` or small list of port names with a "none" entry, a lamp
  "MIDI" lit while connected, persisted chosen port name (`midi_port`), reconnect at launch
  (silently skipped when the port is absent), all-notes-off on disconnect.
- Undo/redo (`undo.rs`): `Snapshot { pattern, params, bpm, swing, preset_index }`; a stack of
  up to 100 with a redo stack; every edit that changes the snapshot pushes (knob drags coalesce
  into one entry per drag via `drag_started/stopped`); Ctrl+Z / Ctrl+Y (also Ctrl+Shift+Z);
  restoring a snapshot resends everything to the engine.
- Shortcuts: Space play/stop, R record arm, Ctrl+S save, Ctrl+Shift+S save as, Ctrl+O open,
  Ctrl+E export, Ctrl+N new (never while a text field is focused).
- Unsaved marker: `dirty` flag set by any edit, cleared on save/open/new; project panel shows
  "Unsaved changes"; New/Open with `dirty` first show an inline "Discard changes?" pill pair.
- Window sizing: investigate why eframe 0.37 may open smaller than `with_inner_size` at
  non-100 % scaling; fix (e.g. request the size again on the first frame via
  `ViewportCommand::InnerSize`, or set `with_min_inner_size` to the default) and test at 1.5.

### neu
- `TextKind::Brand` (15 px, +0.16em tracking, weight 300) and a `brand(ui, theme, text)` helper.
- Every knob/dial/vslider shows its formatted value and label in a tooltip on hover (egui
  `on_hover_text` styled by `Theme::apply`: surface fill, raised shadow, no stroke).

## M5 additions (patterns + song mode)

### engine: data
- `MAX_STEPS = 64`, `PATTERN_COUNT = 8`, `MAX_CHAIN = 64`, `BAR_STEPS = 16`.
- `Pattern` gains `length: u8` (16, 32 or 64; `PatternLength::{Bar1, Bar2, Bar4}` helper) and
  its step storage grows to `MAX_STEPS` per row; rows beyond `length` are kept but not played.
  Serialisation stays a plain array per row (JSON lists); version-2 files with 16 entries load
  with `length = 16` (pad with empty steps), files with 32/64 entries load as is. No allocation
  on the audio thread: step storage is fixed-size (`[DrumStep; MAX_STEPS]`, serde via a small
  `serialize_with`/`deserialize_with` pair, no new crates).
- `Bank { patterns: [Pattern; PATTERN_COUNT], chain: [ChainEntry; MAX_CHAIN], chain_len: u8,
  mode: PlayMode, current: u8 }`, `ChainEntry { pattern: u8, repeats: u8 (1..=8) }`,
  `PlayMode { Pattern, Song }`. `Bank::default()` = demo in A, empty B..H, chain [A x1], mode
  Pattern, current 0. `Bank::demo()` additionally fills B with a variation and chain A A B A.
- `Project` version 3: `bank: Bank` replaces `pattern`; `from_json` migrates v2 (`pattern` ->
  patterns[0], chain [A x1]) and v1. `Project::default()` uses `Bank::default()`.

### engine: messages and scheduler
- Edits target the *edit* pattern: `SetEditPattern(u8)`; the existing step/row/volume/tune
  messages apply to it. New: `SetPatternLength { pattern: u8, steps: u8 }`, `CopyPattern {
  from: u8, to: u8 }`, `ClearPattern(u8)`, `SelectPattern(u8)` (pattern mode: the playing
  pattern switches at the next 16-step bar boundary while playing, immediately when stopped;
  also sets the edit pattern), `SetPlayMode(PlayMode)`, `SetChainEntry { index: u8, pattern:
  u8, repeats: u8 }`, `SetChainLen(u8)`. All `Copy`.
- Scheduler: the bar is always 16 steps; a pattern of length L plays L/16 bars then restarts
  (pattern mode) or advances the chain (song mode: after `repeats` plays of the entry, next
  entry, wrapping at `chain_len`; an empty chain plays the current pattern). A queued
  `SelectPattern` takes effect at the next bar boundary. Changing a playing pattern's length
  takes effect at its next bar boundary too. Play always starts at step 0 of the current
  pattern (song mode: chain index 0).
- `Position` gains `pattern: u8` (playing), `bar: u8` (0-based bar within the pattern),
  `song_index: u8`, `queued: Option<u8>` (pattern mode), published through more atomics.
- `Engine::set_bank(&Bank)`, `bank() -> &Bank`, `edit_pattern() -> u8`, plus direct setters
  mirroring the messages. `set_pattern(&Pattern)` keeps working on the edit pattern.

### engine: export
- `render_loop(project, repeats, sr)` renders the current pattern `repeats` times in pattern
  mode; `render_song(project, sr)` renders the whole chain once (plus tail); `export_frames`
  gains a `song: bool`; `render_loop_wav(.., song: bool, ..)` or a second entry point
  `render_song_wav`. Length = sum over chain of `repeats * length_steps * spp` (+ tail).

## M5 as built (engine)
`Bank { patterns: [Pattern; 8], chain: [ChainEntry; 64], chain_len, mode: PlayMode, current }`,
`Pattern.length` 16/32/64 with 64-slot rows, `Project` v3 with `bank` (`pattern()` compat
accessor), messages `SetEditPattern`, `SetPatternLength`, `CopyPattern`, `ClearPattern`,
`SelectPattern` (queued at the bar boundary while playing), `SetPlayMode`, `SetChainEntry`,
`SetChainLen`; `Position { pattern, bar, song_index, queued }`; `render_song`,
`render_song_wav`, `export_frames(.., song, ..)`. Step messages target the edit pattern.
`send_pattern` from the UI must send only the pattern's `length` steps.

## M6 additions (mixer)

### engine: channels (`mixer.rs`)
- `Channel { Kick, Snare, Clap, ClosedHat, OpenHat, Synth }` (`ALL`, `COUNT = 6`, `index`,
  `label`), `ChannelParam { Level (dB, -60..=6, default 0), Pan (-1..=1, 0), Mute (0/1),
  Solo (0/1), DelaySend (0..=1, default: Synth 1.0 else 0), ReverbSend (0..=1, default:
  Synth 1.0, Snare 0.3, Clap 0.3, else 0) }` with `range()`/`default_value()`/`key()`.
- `MixerState { strips: [Strip; 6] }`, `Strip { level_db, pan, mute, solo, delay_send,
  reverb_send }` (serde, `Default`), `MixerState::effective_gain(ch) -> f32` (mute/solo
  logic: when any solo is on, non-solo channels are silent), `pan_gains(pan) -> (l, r)`
  equal-power (-3 dB at centre).
- `Message::SetMixer { channel: Channel, param: ChannelParam, value: f32 }` (Copy);
  `Engine::set_mixer(channel, param, value)`, `mixer() -> &MixerState`, `set_mixer_state(&)`.
- Signal flow per sample: `DrumKit::tick_rows(&mut [f32; 5])` (per-row outputs; keep `tick()`
  as the sum for compatibility) and the synth mono; each channel: insert (synth only: drive)
  -> smoothed gain (level * mute/solo) -> pan -> stereo dry bus, plus post-fader sends into a
  stereo delay bus and a stereo reverb bus; `FxChain` gains `process_bus(dry: (l, r), delay_in:
  (l, r), reverb_in: (l, r)) -> (l, r)` where `DelayMix`/`ReverbMix` act as return levels
  (0 = no return) and `DriveAmount/DriveMix` stay the synth insert; `process(l, r)` keeps
  working as "everything through the synth channel" for existing tests. Then master gain,
  limiter, test tone.
- Meters: `Meters { channel: [f32; 6], master: [f32; 2] }` published per block as the block
  peak (linear) through `AtomicU32` bit patterns in `Shared`; `EngineHandle::meters()` reads
  them; the UI applies its own decay. Allocation-free.
- `Project` v4: `mixer: MixerState`; v3 files load with `MixerState::default()`.
- Export honours the mixer.

### neu
- `fader(ui, &Theme, &mut f32, &SliderSpec) -> Response`: a taller vertical slider (track
  6x140) with a meter strip beside it (`meter(ui, &Theme, level: f32, height)`: inset-s track,
  accent fill proportional to level, accent_hover above -6 dBFS). `pan_knob` = `knob` with
  `bipolar` (already available). Mute/solo = `toggle_button` "M" / "S" (square 28 variant:
  `toggle_button_sized`).

### app
- `ui/mixer.rs`: a MIXER panel toggled from the transport (pill "MIXER" / "FX", swapping the
  right-hand panel beside the synth): six strips (label, pan knob Sm, DELAY/REVERB send knobs
  Sm, fader + meter, M/S toggles) and a MASTER strip (gain dial Sm, stereo meters, LIMIT).
  All bound to `AppState.mixer: MixerState` with `set_mixer` sending `SetMixer`; undo-able;
  saved in the project; `resend_all` includes the mixer. Meters polled each frame from
  `engine.meters()` with a 300 ms decay.
