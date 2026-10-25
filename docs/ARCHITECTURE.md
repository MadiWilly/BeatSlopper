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
- Window: title "BeatSlopper", default inner size 1280x1140, min 1270x760, surface-colour clear.
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

## M5 as built (app)
The right column (260 wide) shows one panel at a time, PROJECT or SONG CHAIN, with a tab in
both heads; the sequencer shows one bar with a BAR selector and FOLLOW; `AppState::send`
spills into an outbox drained each frame so a dense bank never drops messages; the minimum
window is 1270x760.

## M7 additions (piano roll)

### engine: polyphonic notes
- `Note { start: u8 (step 0..length), length: u8 (1..=64 steps, clipped at the pattern end),
  pitch: u8 (0..=127), velocity: f32 (0.05..=1) }` (Copy, serde). `MAX_NOTES = 64` per pattern.
- `Pattern` gains `notes: [Note; MAX_NOTES]` + `note_count: u8` (helpers `notes() -> &[Note]`,
  `add_note(Note) -> Option<usize>` (rejects when full; sorts by start then pitch), `remove_note
  (index)`, `set_note(index, Note)`, `note_at(step, pitch) -> Option<usize>`, `clear_notes`).
  The step grid's bass lane (`synth: [SynthStep; 64]`) becomes a **derived view**: after every
  note edit `rebuild_lane()` sets `synth[s]` to the lowest-pitched note starting at step `s`
  (accent = velocity >= 0.95); `set_synth_step(step, note, accent)` upserts a length-1 note at
  that step (removing other notes that start there) so grid editing keeps working unchanged.
  Serde: `notes` are written; `synth` is `#[serde(skip)]` and rebuilt on load; version 5 reads
  v4 files by converting each `synth[s]` with a note into a length-1 `Note`.
- Scheduler: on each step boundary `s`, every note with `start == s` triggers `voices.note_on
  (pitch, velocity)` and schedules its note-off at `start + length` steps (exactly at that
  trigger sample minus 1 sample, so a note that ends where another of the same pitch starts
  retriggers cleanly); the pool is polyphonic so chords work. `seq_note` becomes a small
  fixed array of active sequencer notes (`[Option<(u8 pitch, u64 off_sample)>; MAX_VOICES]`).
  Pattern switches/stop release all active sequencer notes. Keyboard-held pitches are still
  never released by the sequencer.
- Messages: `SetNote { pattern: u8, index: u8, note: Note }` (index == note_count appends),
  `RemoveNote { pattern: u8, index: u8 }`, `ClearNotes(u8)`. `Message` stays `Copy`.
- `Project` version 5 (`FORMAT_VERSION = 5`), v4 migration as above.
- Export unchanged (goes through the scheduler).

### neu: piano roll widget
- `piano_roll(ui, &Theme, &mut PianoRollModel, &[RollNote]) -> PianoRollOutput`.
  `RollNote { start: u8, length: u8, pitch: u8, velocity: f32, selected: bool }`.
  `PianoRollModel { steps: usize (16/32/64), low_pitch: u8 (bottom visible), pitches: usize
  (visible rows, default 36), step_width: f32 (default 24), row_height: f32 (default 14),
  playhead_step: Option<f32> (fractional), selected: Option<usize> }`.
  Painting: inset-m panel background, white/black key shading on rows (surface vs a 6 %
  shadow-dark tint), bar lines (shadow-dark 1 pt) and beat lines (lighter), note names on a
  48 pt key column at the left (C only labelled, e.g. "C3"), notes as rounded bars (radius 6)
  in accent (OnGlow when selected, accent_hover when velocity >= 0.95), a 2 pt playhead line
  in accent. Interaction: drag on empty space draws a note (start = press cell, length = drag
  extent, min 1, snapped to steps); drag a note body moves it (pitch and start, snapped);
  drag a note's right 6 pt edge resizes; click selects; Delete/Backspace reported; shift-click
  toggles accent (velocity 1.0 vs 0.8); scroll wheel scrolls pitch (shift+wheel = horizontal
  for long patterns); double-click on a note deletes it.
  `PianoRollOutput { add: Option<RollNote>, moved: Option<(usize, RollNote)>, removed:
  Option<usize>, selected: Option<usize>, scrolled: bool }` (at most one edit per frame).
  All geometry helpers (`cell_at(pos)`, `note_rect(note)`) are public and unit-tested.

### app
- A PIANO ROLL panel toggled from the sequencer head ("GRID | ROLL" segmented): it replaces the
  sequencer grid area (same panel size) and edits the edit pattern's notes through
  `SetNote/RemoveNote` (undoable, dirty). The grid's bass lane keeps working via the derived
  view. Record-arm overdub and click-then-key entry write length-1 notes as before. Playhead
  from `Position` (bar * 16 + step + frac). Keyboard: Delete removes the selected note, Escape
  deselects. Persisted: roll/grid choice, `low_pitch`.

## M8 additions (synthesis extensions)

### engine: new `ParamId`s (unit, range, default, key)
- `Osc1Pulse` (0.05..=0.95, 0.5, `osc1_pulse`), `Osc2Pulse` (same, `osc2_pulse`): pulse
  width of the square wave.
- `SidechainAmount` (0..=1, 0, `sidechain_amount`), `SidechainRelease` (s, 0.05..=1, 0.25,
  `sidechain_release`): the synth channel ducks on every kick trigger (EDM pump); not a synth
  id, not an fx id: routed by the engine to the ducker (`is_mixer_like` not needed; handle in
  `Engine::set_param` directly).
- `Osc1Unison` (1..=7 voices, 1, discrete, `osc1_unison`; 7 is the classic supersaw), `Osc1Spread` (cents, 0..=50, 10,
  `osc1_spread`): unison copies of osc 1 detuned symmetrically.
- `SubLevel` (0..=1, 0, `sub_level`): sine one octave below osc 1.
- `NoiseLevel` (0..=1, 0, `noise_level`), `NoiseColor` (0 white / 1 pink, discrete, `noise_color`).
- `FmAmount` (0..=1 mapped to modulation index 0..=8, 0, `fm_amount`), `FmRatio` (0.5..=8, 1,
  `fm_ratio`): osc 2 (sine) at ratio x osc 1 frequency phase-modulates osc 1.
- `OscMode` (0 subtractive / 1 pluck, discrete, `osc_mode`), `PluckDamping` (0..=1, 0.5,
  `pluck_damping`), `PluckBright` (0..=1, 0.7, `pluck_bright`).
- `FilterType` (0 LP / 1 BP / 2 HP, discrete, `filter_type`), `FilterAttack` (s, 0.001..=2,
  0.005, `filter_attack`), `FilterDecay` (s, 0.001..=2, 0.25, `filter_decay`), `FilterSustain`
  (0..=1, 0.2, `filter_sustain`), `FilterRelease` (s, 0.001..=4, 0.3, `filter_release`),
  `FilterKeyTrack` (0..=1, 0.3, `filter_key_track`), `VelToCutoff` (0..=1, 0.3,
  `vel_to_cutoff`), `VelToAmp` (0..=1, 1.0, `vel_to_amp`).
- `PitchEnvAmount` (semitones, -24..=24, 0, `pitch_env_amount`), `PitchEnvDecay` (s,
  0.001..=2, 0.1, `pitch_env_decay`).
- `LofiBits` (4..=16, 16, `lofi_bits`), `LofiRate` (divisor 1..=16, 1, `lofi_rate`),
  `LofiWobble` (0..=1, 0, `lofi_wobble`), `LofiNoise` (0..=1, 0, `lofi_noise`), `LofiTone`
  (Hz, 1000..=20000, 20000, `lofi_tone`), `LofiMix` (0..=1, 0, `lofi_mix`).
- `ChorusRate` (Hz, 0.1..=5, 0.8, `chorus_rate`), `ChorusDepth` (0..=1, 0.3, `chorus_depth`),
  `ChorusMix` (0..=1, 0, `chorus_mix`).
`FilterEnvAmount` keeps its meaning (depth of the filter ADSR, +-4 octaves). `is_synth()`
covers everything up to the velocity ids; `is_fx()` covers lo-fi and chorus too.

### engine: DSP
- `synth/oscillator.rs`: pulse width for Square; `synth/fm.rs` sine modulator; `synth/noise.rs`
  white + pink (Paul Kellet); `synth/pluck.rs` Karplus-Strong (pre-allocated delay line per
  voice at construction sized for the lowest note, fractional read, one-pole damping,
  excitation = short noise burst filtered by brightness); `synth/svf.rs` returns LP/BP/HP.
- `voice.rs`: pitch env (one-pole decay from `amount` semitones to 0), filter ADSR (`Adsr`),
  key tracking (cutoff x 2^((note - 60)/12 x track)), velocity to amp/cutoff, unison (up to 4
  osc-1 copies (up to 7), summed / sqrt(n)), sub, noise, FM, pluck mode (replaces the oscillators).
- `fx/ducker.rs` (sidechain): on each kick trigger the gain drops to `1 - amount` within 2 ms
  and recovers exponentially over `release`; applied to the synth channel after chorus.
  Allocation-free; all per-voice state fixed-size.
- `fx/lofi.rs`: bit crusher (quantise to 2^bits), sample-and-hold rate reducer (hold every N
  samples), wobble (slow random LFO 0.5..=3 Hz into +-20 cents through a short modulated delay
  line, 20 ms max), vinyl noise (filtered noise + occasional crackle impulses), high cut
  (one-pole), dry/wet. Inserted on the synth channel after the drive. `fx/chorus.rs`: two
  modulated delay taps (5..=25 ms, LFO rate/depth) with stereo offset, mix; on the synth
  channel after lo-fi, before the sends and the dry bus (the synth channel becomes stereo).
- `Project` v6 (`FORMAT_VERSION = 6`); older files load with the new defaults.

### neu
Nothing new required (knobs, segmented, sliders cover the new groups).

### app
Synth panel groups: OSC 1 gains PULSE and UNISON/SPREAD; a MOD group (FM AMT, FM RATIO, SUB,
NOISE level + colour); MODE segmented (Synth | Pluck) with DAMP/BRIGHT knobs shown in pluck
mode; FILTER gains TYPE segmented + its own A D S R sliders + KEY TRACK; a PITCH ENV group
(AMOUNT, DECAY); VELOCITY group (AMP, CUTOFF). FX rack gains LO-FI (BITS, RATE, WOBBLE, NOISE,
TONE, MIX) and CHORUS (RATE, DEPTH, MIX). The panel may need a vertical scroll area or a
second row; keep the whole-sequencer visibility tests green.

## M9 additions (preset library)
- `presets.rs`: `Category { LofiKeys, LofiGuitars, LofiPads, EdmLeads, EdmBass, EdmPlucks, EdmPads, Risers }`
  (`ALL`, `label`), `Preset { name, category, tags: &[&str], values }`, `ALL` with at least 80
  entries grouped by category, `by_category(cat)`, `find(name)`, `index_of(name)`,
  `search(query)` (case-insensitive on name and tags), `Preset::is_drone()` (tag "drone").
  Keep the original 8 presets (renamed into categories where sensible).
- app: `ui/preset_browser.rs` (category segmented, scrollable name list, prev/next, search
  text field styled by the theme; shortcuts suppressed while it is focused), `preset_name`
  persisted, "modified" marker, Ctrl+Up/Down stepping.

## Plugins: EQ and Reverb (pop-out windows, branch `madi/plugins-eq-reverb-1bffdb`)

Two FL-style "plugin" windows, opened from the PLUGINS pills (`EQ` / `REVERB`) in the FX rack
/ MIXER head. Everything is additive: no `ParamId` was added, `fx/chain.rs` only gained
`reverb()` / `reverb_mut()` / `last_reverb_wet()`, and the new state lives in its own structs
and messages.

### engine: master EQ (`fx/eq.rs`)
- `EqKind { LowCut, LowShelf, Peak, HighShelf, HighCut }` (`ALL`, `index`, `from_index`,
  `from_value`, `label`, `has_gain`), `EqParam { On, Kind, Freq, Gain, Q }` (`ALL`, `range`,
  `label`), `EqBand { on, kind, freq (20..=20000 Hz), gain_db (-18..=18), q (0.1..=10) }`
  (serde, `#[serde(default)]`, `set` / `get` by `EqParam`, `normalise`, `response_db(hz, sr)`),
  `EqState { bands: [EqBand; 7], on, gain_db }` (serde; `Default` = `FLAT`: low cut 30 Hz off,
  low shelf 100, peaks 250 / 1k / 4k, high shelf 10k, high cut 18k off; `layout(gains, on)`,
  `set`, `set_gain_db`, `response_db(hz, sr)` = sum of the enabled bands + output gain, exactly
  `0` when off, `is_flat`). `PRESETS: &[EqPreset { name, state }]` (12: Flat, Bass Boost, Sub
  Cut, Lofi Telephone, Air, De-Mud, Presence, Radio, Warm Tape, Club Low End, Scooped, Vocal
  Clarity) and `preset_index_of(&EqState)`.
- DSP: `Coeffs::compute(kind, freq, gain_db, q, sr)` (RBJ cookbook, `f64`, freq clamped below
  `0.49 sr`) with `response_db`; `BiquadState::tick` (transposed direct form II,
  denormal-flushed); `ParametricEq::new(sr)` with `set(band, param, value)`, `set_on`,
  `set_gain_db`, `set_state` (snap), `reset`, `state()`, `process(l, r)`. Per band the
  frequency, gain and an on/off wet amount are `Smoother`s (10 ms); coefficients are recomputed
  only while a band's smoothers move (or on kind / Q / on changes). The master on/off and the
  output gain are smoothed too. Off and flat states are bit-transparent.
- Engine: the EQ runs after `FxChain::process_bus` and before the test tone, master gain and
  limiter. `Message::SetEq { band: u8, param: EqParam, value }`, `SetEqOn(bool)`,
  `SetEqGain(f32)`; `Engine::{eq, set_eq, set_eq_on, set_eq_gain, set_eq_state}`.

### engine: reverb extensions (`fx/reverb.rs`)
- `Reverb` gains a pre-allocated pre-delay line (`0..=200 ms`, linearly interpolated, the read
  position glides over 50 ms), `width` (mid/side blend of the two wet channels, smoothed; `0`
  = L == R), and wet `low_cut` / `high_cut` second-order filters (`fx::eq::Coeffs`), bypassed
  exactly at 20 Hz / 20 kHz, so the default state renders bit-identically to M2. Setters:
  `set_pre_delay_ms`, `set_width`, `set_low_cut_hz`, `set_high_cut_hz`, `apply_state`,
  `state()`, getters; `reset` also snaps the new smoothers.
- `ReverbParam { Size, Damping, PreDelay, Width, LowCut, HighCut, Mix }` (`ALL`, `range`,
  `default_value`, `label`, `is_log`, `clamp`), `ReverbState { size, damping, pre_delay_ms,
  width, low_cut_hz, high_cut_hz, mix }` (serde, `DEFAULT` = the M2 sound, `new`, `set` /
  `get`, `normalise`, `comb_feedback`, `decay_time_60db()` (analytic: longest comb loop /
  loss per pass, a little shorter with damping, capped at 15 s, monotonic in size),
  `impulse_envelope(sr, seconds) -> Vec<f32>` (RMS per 10 ms bin of a throwaway wet-only
  reverb; UI / tests only). `PRESETS: &[ReverbPreset]` (10: Small Room, Studio, Plate, Hall,
  Cathedral, Lofi Spring, Dark Ambient, Bright Chamber, Gated, Infinite Wash),
  `preset_index_of`.
- Engine: `Message::SetReverb { param: ReverbParam, value }`; `Engine::{reverb, set_reverb,
  set_reverb_state}`. `ReverbState` is the source of truth: `set_reverb(Size | Damping | Mix)`
  goes through `set_param(ParamId::Reverb*)`, and `set_param(ParamId::Reverb*)` writes the
  state, so both views always agree. The pre-delay / width / cuts reach the DSP through
  `FxChain::reverb_mut()`. `set_reverb_state` (project load) also snaps the chain's size /
  damping / mix smoothers and the reverb's pre-delay / width glides (`FxChain::snap_reverb`,
  `Reverb::snap`; delay lines and tail untouched), like `set_mixer_state` / `set_eq_state`, so
  an offline render honours the loaded room from its first hit. A synth `Preset::apply` resets
  the three shared values and leaves the extras alone (reverb audit, DECISIONS.md 2026-10-25).
- Reverb level meter: `FxChain::last_reverb_wet()` (the return after its level) is peak-held
  per block and published through one more atomic; `EngineHandle::reverb_level() -> f32`.

### engine: spectrum (`spectrum.rs`)
`SPECTRUM_SIZE = 2048`, `SPECTRUM_BINS = 1025`. `spectrum_pair() -> (SpectrumTap,
SpectrumReader)`: the tap (audio side, created in `new_engine`) `push`es every mono output
sample `(l + r) / 2` into a ring of `AtomicU32` bit patterns and `publish`es the write index
once per block; the reader (UI side, `EngineHandle::spectrum()`, `Clone`) `snapshot`s the ring
in time order. `fft_radix2(re, im)` is a hand-written iterative Cooley-Tukey (no new crate);
`Analyzer::analyse(&samples, dt) -> &[f32]` Hann-windows, FFTs, converts to dB (full-scale
sine = 0 dB, floor -90 dB) and applies attack / 40 dB/s decay smoothing per bin;
`level_at(hz, sr)` interpolates; `bin_hz(bin, sr)`.

### engine: project
`Project` version 6 adds `eq: EqState` and `reverb: ReverbState` (both `#[serde(default)]`).
`apply_to` sets the params first, then `set_reverb_state` and `set_eq_state`;
`from_json` builds the reverb from `reverb_size` / `reverb_damping` / `reverb_mix` when a
file has no `reverb` (version <= 5) and always copies the state's size / damping / mix back
into `params` (`sync_reverb_params`), so the state wins over a stale param. Exports honour
both: `render_project` goes through `apply_to`, whose `set_reverb_state` snaps the room
(test `export_honours_the_reverb_pre_delay_from_the_first_hit`).

### app
- `plugins.rs`: `PluginKind { Eq, Reverb }` (`ALL`, `index`, `title`, `pill_label`,
  `default_size`, `viewport_id`), `WindowGeom { open, pos, size }`, `PluginWindows { eq,
  reverb }` (persisted under `plugin_windows`), `PluginEdit { EqBand { band, param, value,
  drag_started }, EqOn, EqGain { db, drag_started }, EqPreset(i), Reverb { param, value,
  drag_started }, ReverbPreset(i), Close(kind) }`, `PluginShared` (theme, sample rate, the EQ /
  reverb mirrors and preset indices, the selected band, the graph rect, the `SpectrumReader` +
  `Analyzer`, the reverb level with its decayed meter value, the `EnvelopeCache` and its
  debounced worker thread (`ENVELOPE_DEBOUNCE` 100 ms, rendered at 24 kHz over
  `max(RT60, 1 s)` capped at 10 s), the queued edits and shortcuts, the window geometry) and
  `PluginHost { shared: Arc<Mutex<PluginShared>> }` with `restore`, `take_edits`, `sync(..)`,
  `show_viewports(ctx, open)`, `geometry(open)`.
- Deferred viewports: `show_viewports` calls `Context::show_viewport_deferred` once per open
  window every main frame (the callback is `Send + Sync + 'static`, hence the `Arc<Mutex>`; the
  lock is only ever taken on UI threads). `ui::plugins::viewport_ui` paints the surface,
  records the window geometry, turns `close_requested` into `PluginEdit::Close`, forwards
  `shortcuts::take_shortcuts` from the window's input, and requests a repaint of the root
  viewport whenever it queued something. Under `egui_kittest` (`embed_viewports`) no viewport
  is shown; the window bodies are tested as plain panels.
- `AppState` gains `eq`, `eq_preset`, `reverb`, `reverb_preset`, `plugin_open: [bool; 2]`
  and the setters `set_eq_band(band, param, value, drag_started)`, `set_eq_on`,
  `set_eq_gain(db, drag_started)`, `set_eq_state`, `apply_eq_preset`, `send_eq`,
  `set_reverb(param, value, drag_started)`, `set_reverb_state`, `apply_reverb_preset`,
  `send_reverb`, `apply_plugin_edit(PluginEdit)`, `is_plugin_open` / `toggle_plugin`; `set_param`
  on `ReverbSize / Damping / Mix` mirrors into `reverb`, `apply_preset` too. `EditKey` gains
  `Eq(band)` (one key per band: a node drag that moves frequency and gain is one undo step),
  `EqGain`, `Reverb(param)`. `Snapshot` (undo) and the project glue carry `eq` and `reverb`;
  `resend_all` sends `eq_messages` (37) and `reverb_messages` (7). `BeatSlopperApp::sync_plugins`
  runs in `tick`.
- `ui/plugins/`: `eq_graph.rs` (geometry helpers `freq_to_x` / `x_to_freq` / `db_to_y` /
  `y_to_db` / `spectrum_to_y` / `node_pos` / `hit_node` / `plot_rect`; painting: inset-m
  graph, log grid 20 Hz..20 kHz with labels, dB grid -18..+18, the spectrum as a mesh in
  accent at 20 % alpha, the response curve 2 pt accent, 12 pt nodes (OnGlow when selected,
  raised-s with an accent ring otherwise, a faint hollow dot for off bands); interaction: drag
  = freq / gain, wheel = Q, double-click = on / off, right-click = select + cycle kind, click =
  select; hover tooltip), `eq_window.rs` (head with `EQ On`, graph, `BAND n` controls: TYPE
  segmented, ON, FREQ / GAIN / Q knobs, OUTPUT knob; PRESETS column with the MODIFIED lamp and
  a scrollable pill list), `reverb_window.rs` (RT60 readout, the envelope plot on a log dB
  scale with time ticks and the RT60 marker, the wet meter, the seven knobs, PRESETS column;
  the head carries the caps hint `SHARED_HINT` "Size · damp · mix shared with the FX rack" and
  the MIX knob's tooltip a second line, `MIX_NOTE`: mix is the return level, the mixer's REV
  sends feed the bus).
- Reverb audit (branch `madi/reverb-audit-087b74`): the FX rack's REVERB group head has a
  `More` pill (`fx_rack::REVERB_MORE_LABEL`, lit while the window is open, toggles
  `plugin_open[Reverb]`; not "Open" because the PROJECT panel's file button owns that
  accessible name), the REVERB label and the rack's MIX knob explain sends vs return level
  (`REVERB_TOOLTIP`, `REVERB_MIX_NOTE`, via `Knob::note` -> `neu::knob_with`), and the mixer's
  PAN / DLY / REV legends have `LEGEND_TOOLTIPS`. `neu::knob_noted` / `knob_with` add a second
  tooltip line to a knob (one tooltip, not two stacked).
- Tests: `crates/engine/tests/plugins.rs` (12), unit tests in `fx/eq.rs`, `fx/reverb.rs`,
  `spectrum.rs`, `plugins.rs`, `eq_graph.rs`; `crates/app/tests/plugins_ui.rs` (both windows
  render in both themes, preset pills apply through the app state, a node drag moves the band
  and is one undo step, double-click / wheel, the pills open / close the windows and the host
  routes edits).
## Visualizers (branch `madi/visualizers-bc3bf5`)

### engine: master-output tap (`scope.rs`, additive)
- `scope::channel() -> (ScopeTap, ScopeReader)`: a pre-allocated `rtrb` ring of
  `SCOPE_CAPACITY_FRAMES` (8192) interleaved stereo frames. `new_engine` creates it; the
  `Engine` owns the `ScopeTap`, the handle hands out the reader once through
  `EngineHandle::take_scope_reader()`. `render` ends with `scope.push_block(body)` (the final
  output after the limiter, whole frames only, as many as fit; when the UI stalls the rest of
  the block is skipped and counted in `ScopeTap::skipped()`; nothing blocks or allocates).
- `ScopeWindow` (UI side): the latest `SCOPE_WINDOW_FRAMES` (2048) frames as contiguous,
  time-ordered `left()`, `right()` and `mono()` slices (mirrored ring, no copy on read),
  `push` / `push_block`, `take_peak`, `is_silent`. `ScopeReader::drain_into(&mut window)`
  moves every waiting frame, oldest first.
- Drum hits: `Shared.hits: AtomicU32`; `fire_step` ORs `1 << DrumRow::index` into the engine's
  `hit_bits` for every row that triggers and `render` publishes them with `fetch_or` at the
  end of the block; `EngineHandle::take_hits() -> u8` swaps them out (latched until read, so a
  UI frame slower than an audio block still sees every hit), `hits()` peeks.
- Tests (`tests/scope.rs`): the tap delivers exactly what `render` produced; overflow skips
  without panicking and recovers after a drain; the kick step sets bit 0, snare + closed hat
  set their bits together and stay latched until taken.

### neu: `visualizer` module
- `VisualInput { samples (left / mono window), samples_r: Option (right), spectrum (64 log
  bins 0..=1), peak: (l, r), hits: u8, playhead: 0..1, bpm, time }`, `VisualInput::SILENT`.
- `VisualStyle { glow, grid, mirror, thickness, fade, palette: Palette::{Accent, Hover,
  Blend} }`, `VisualStyle::DEFAULT`.
- `Visualizer::{Scope, Bars, Radial, Vector, Waves, Particles}` (`ALL`, `label`,
  `widget_label` = accessible name such as `"Scope visualizer"`, `show`) and the free
  functions `scope`, `bars`, `radial`, `vector`, `waves`, `particles`, each
  `fn(ui, &Theme, &VisualInput, &VisualStyle) -> Response`, filling
  `ui.available_size_before_wrap()` (at least 40x20) with an inset-m backdrop (optional faint
  shadow-dark grid) and the drawing. Glow is always a wider low-alpha second stroke; no
  textures, shaders or blur. Animation state (peak holds, kick-ring start times, the wave
  history, the particle pool with a xorshift PRNG) lives in egui temp memory under the
  response id, so each call site animates independently. Helpers with unit tests:
  `trigger_index`, `bar_values`, `correlation`, `wave_envelope`.
- `VisualPreset { name, visualizer: Option<Visualizer> (None = Off), style }`, `PRESETS`
  (Soft Scope, Club Bars, Orbit, Vector, Tide, Fireworks, Off), `OFF_PRESET`,
  `preset(index)`, `preset_index(name)`, `preset_names()`.
- Kittest (`tests/visualizer.rs`): every visualizer renders in both themes with silence, a
  sine and noise (with hits) at strip and panel sizes; every preset renders and Off draws
  nothing; particles spawn on hits and fade out; 60 frames of each visualizer take under 1 s
  in debug; NaN / infinite inputs do not panic.

### app
- `spectrum.rs`: `Spectrum::new(sample_rate)`, `analyse(samples, dt)` (2048-point Hann
  radix-2 FFT, hand written, allocation-free after construction; 64 log bands 30 Hz..16 kHz,
  dB scale -60..0 -> 0..1 with a 0 dBFS sine = 1; instant attack, 0.2 s exponential
  release), `bins()`, `raw()`, `peak_bin()`, `is_active()`, `band_of(hz)`,
  `band_edges_hz(k)`.
- `AppState`: `visual_preset: usize` (persisted under `VISUAL_PRESET_KEY`, default 0 = Soft
  Scope; `set_visual_preset`, `visual_preset()`), `visual_full` (`toggle_visual_full`,
  `set_visual_full`; Escape closes), `scope: Option<ScopeReader>` (taken in `from_parts`),
  `scope_window`, `spectrum` (at the device rate), `visual_hits`, `top_row_rect` (the
  SYNTH | FX row, measured by `ui::main_area`). `update_visuals()` runs every tick after the
  meters: drains the tap, takes the hits, analyses the window while the preset is on.
  `visuals_active()` keeps the repaint going while audio runs or decays, the spectrum is
  still falling, the window is not silent or a hit is less than 1.5 s old; always false when
  Off. `visual_input(time)` builds the `VisualInput` (left / right windows, bands, the
  decayed master meters as the peak, hits, playhead within the bar).
- `ui/visualizer.rs`: `show` (the right column's VISUAL tab: the shared tab row, a PRESET
  stepper `(-) Soft Scope (+)` that wraps, the FULL toggle, the drawing filling the rest),
  `strip(ui, state, theme, max_width)` (the transport bar's mini visualizer, up to 220x40,
  shrinking to the room left before the right-hand group and hiding below 100 pt; about 144
  pt at the default 1280 window), `overlay` (a foreground `Area` over `top_row_rect` with the
  preset pills, CLOSE and the drawing), `preset_pills`, `draw`.
- Shared-file changes: `RightTab::Visual`; `controls::RIGHT_TABS` is
  `Project | Chain | Visual` and `panel_head` is now that tab row alone (the selected item,
  12/500 text-primary, is the panel's title: a title beside three tabs did not fit 220 pt and
  a second row pushed the PROJECT status line into the KEYS panel); the transport bar's Test
  tone pill moved under the MIDI selector and the bar's group gap tightens 24 -> 16 while the
  strip is on (the row had no free width otherwise); `ui::show` draws the overlay last.
- Tests (`tests/ui_renders.rs`): the strip renders in both themes without changing the bar's
  height and the right-hand group stays inside the bar; the VISUAL panel fits the column
  above the KEYS panel; FULL opens the overlay over the top row and Escape / CLOSE shut it;
  the overlay pills choose a preset and Off hides every drawing; the preset survives
  `save` -> `load_visual_preset`; offline-rendered audio reaches the window and the analyser.
## QOL pass (2026-10-25, branch `madi/qol-polish-6ccb28`)

Small usability work a producer hits in the first ten minutes. Everything is additive: new
files where possible (`neu/src/value_edit.rs`, `app/src/{tempo,recent,drag_paint,qol}.rs`,
`app/src/ui/{tempo,recent_panel}.rs`), small local edits elsewhere, no `ParamId` or project
format change.

### neu: typed value entry and the wheel (`value_edit`)
- `readout(ui, theme, id, pos, anchor, text, font, color, name) -> Option<String>`: a
  clickable numeric readout. A click swaps the text for an inline `egui::TextEdit` styled by
  the theme (surface fill, no outline, inset-s pill, accent caret, the old text selected).
  Enter and focus loss commit (the typed string is returned for the caller to parse), Escape
  cancels. `begin_edit` / `show_edit` / `floating_edit` / `is_editing` / `cancel_edit` are the
  pieces; an editor opened in the frame of a click (a right-click) starts its text field the
  next frame, because egui drops the focus of a widget created in the frame of a click
  elsewhere. Readouts are accessible as buttons named `"<label> value"` whose accesskit value
  is the shown text (`node.value()` in tests).
- Parsing: `parse_number(text)` gives the display number of a typed or formatted string
  (units tolerated: `128 bpm`, `-6 dB`, `70 %`, `2.4k` / `2.4 kHz` = 2400, `1.5 s` = 1500 ms,
  pan `L 30` = -30 / `C` = 0, decimal comma). `value_for_display(fmt, min, max, log, target)`
  maps a display number back to the control value through the control's own `fmt`: an affine
  fit through probes at the ends and the middle (every percent / ms / Hz / cents / semitone /
  dB-level / pan formatter, so `70 %` is exactly `0.7`), else a bisection over the fraction
  (the dB-of-gain formatter). `parse_value` combines both and clamps; `parse_int` is the
  integer variant. Garbage parses to `None` and leaves the value alone.
- `knob` / `dial` / `vslider` / `fader`: the value text is a readout; a right-click anywhere
  on the control opens the editor too (floating over the cap for unlabelled small knobs, the
  only way to type into the sequencer row and mixer strip knobs). `stepper`: the value cell
  is a readout (`parse_int`).
- `wheel_notches(ui, response) -> f32`: wheel notches over a control this frame from the raw
  `MouseWheel` events (positive = up; shift = a quarter notch); when non-zero, egui's smoothed
  scroll delta is cleared so the panel's scroll area stays put. `wheel_steps` accumulates
  whole steps for integer controls. Knobs, dials and sliders nudge by one key step (1/50 of
  the range) per notch; steppers by one.

### app
- Shortcuts gain `OpenRecent` (Ctrl+Shift+O), `BpmUp` (`+`, `=`, Ctrl+Up) and `BpmDown`
  (`-`, Ctrl+Down). Home (rewind while playing: a `Play` message restarts at step 0), Escape
  and the note nudges live in `AppState::handle_keyboard` because they depend on focus and
  selection: Escape cancels a pending DISCARD / KEEP, drops the bass and roll selections and
  stops the test tone; a second Escape within 400 ms is the panic (`AllNotesOff`, test tone
  off, arm cleared, status line). With a bass step selected in the grid, Up / Down move its
  note by a semitone and PageUp / PageDown by an octave (keys consumed, so a focused knob does
  not see them). Shortcuts and the piano stay quiet while any text edit has focus
  (`ctx.text_edit_focused()` also covers the kit's inline editors).
- Tempo (`tempo.rs`, `ui/tempo.rs`): `fmt_bpm` (whole number, else one decimal),
  `nudge_bpm`, `TapTempo` (times in seconds on the UI clock; the mean interval of the last
  four taps, a 2 s gap starts a new series, the pill flashes 250 ms). The transport's BPM
  group is `(-) <readout in the 22/300 display type> (+)` with the TAP pill centred under
  it (beside the stepper it crowded out the visualizer strip); the wheel over it nudges.
- Sequencer (`drag_paint.rs`, `qol.rs`, `ui/sequencer.rs`): cells act on the press.
  `PaintStroke { row, on, painted }` starts on a press (`begin_paint`: one undo step), and
  while the button is down every cell of that row under the pointer is painted to `on` once
  (`paint_step`); the stroke ends when no button is down; pressing an "on" cell starts an
  erase stroke. Drum rows only (the bass lane keeps click-to-select). A right-click clears the
  cell (`test-feedback`) and is checked before the press branch. Ctrl-click = shift-click
  (accent). Double-click a
  row label to mute its mixer channel (`row_channel`: drum rows map to their channel, the bass
  lane to `Synth`), shift + double-click to solo; muted labels dim to 45 %, soloed ones get
  an accent dot. LENGTH is `/2 · <n bars> · x2`: `double_pattern_length` copies the loop
  into the new half when that half is empty, `halve_pattern_length` keeps the hidden steps.
  VOL / TUNE legends sit over the row knobs; the hat rows read `C.Hat` / `O.Hat`.
- Projects (`recent.rs`, `ui/recent_panel.rs`): `RecentList` (last 8 paths, most recent
  first, de-duplicated; `next_after` cycles; persisted under `recent_projects`). Every save
  and open pushes. The PROJECT panel's clock button opens the RECENT view in the right
  column (BACK returns); a click opens an entry, with the DISCARD / KEEP confirm when dirty
  (`PendingAction::OpenRecent`, the path parked in `pending_recent`); Ctrl+Shift+O opens the
  next entry after the current project. Missing files are dropped with a status line.
- Window title: `BeatSlopper – file * · 128 BPM · A` (`window_title_full`).
- Clean-up: the brand sub-label no longer names a milestone.
