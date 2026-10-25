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

## M10 additions (instrument tracks)

### engine
- `track.rs`: `MAX_TRACKS = 8`, `TrackId = u8` (`0..MAX_TRACKS`). `Track { active: bool,
  pool: VoicePool, insert: SynthInsert (drive -> lo-fi -> chorus -> ducker, stereo out),
  params: [f32; ParamId::COUNT] (only per-track ids are meaningful) }`; `TrackState { name:
  String (<= 16), colour: u8 (0..8), active: bool, preset: String, params: BTreeMap<String,
  f32> }` is the serde form. `ParamId::is_track()` = `is_synth() || is_synth_insert()`
  (drive, lo-fi, chorus, sidechain); everything else is global.
- `pattern.rs`: `NoteTrack { notes: [Note; MAX_NOTES], note_count: u8 }`; `Pattern.tracks:
  [NoteTrack; MAX_TRACKS]` replaces `notes` / `note_count`; `Pattern.synth` (the lane) is the
  derived view of the *edit* track and is rebuilt when the edit track changes. The note
  helpers take a `track: TrackId`.
- `mixer.rs`: `Channel` grows to 5 drum rows + 8 tracks (`Channel::Track(u8)`), `Channel::COUNT
  = 13`; `MixerState.strips` follows; `effective_gain` unchanged; `Meters.channel` follows.
- `engine.rs`: `tracks: [Track; MAX_TRACKS]`, `edit_track: TrackId`. Render per sample: drums
  -> rows; each active track: `pool.tick()` -> `insert.process(x) -> (l, r)` -> its mixer
  channel (stereo inputs: `mix_frame` takes `[(f32, f32); Channel::COUNT]`); the kick trigger
  calls every active track's ducker. Messages: `SetEditTrack(u8)`, `SetTrackParam { track, id,
  value }`, `SetNote { track, .. }`, `ClearNotes { pattern, track }`, `SetTrackActive { track,
  on }`, `CopyTrack { from, to }`, `SwapTracks(u8, u8)`; `NoteOn/NoteOff/SetParam` and the
  older note messages act on the edit track. `seq_active` becomes per track.
- `project.rs`: v7 (`tracks: Vec<TrackState>`, patterns carry `tracks`); the v6 loader maps
  `params` (track ids) + `notes` + the Synth strip into track 0.

### app
- `AppState.tracks: Vec<TrackUi { name, colour, preset_name, modified }>`, `edit_track`,
  `params` becomes per track (`track_params: [[f32; COUNT]; MAX_TRACKS]`, the global ids live
  in `params` still); `set_param` sends `SetTrackParam` for track ids.
- `ui/tracks.rs`: the TRACKS panel (right-column tab `Tracks`); `ui/sequencer.rs` head shows
  the edit track; `neu::piano_roll` gains a `ghosts: &[(Note, colour)]` input.

### neu: M10 as built
- Ghost notes live in `crates/neu/src/piano_roll.rs`. The existing
  `neu::piano_roll(ui, theme, &mut model, notes)` is unchanged (it is the no-ghost case);
  the app calls the sibling
  `neu::piano_roll_with_ghosts(ui: &mut Ui, theme: &Theme, model: &mut PianoRollModel,
  notes: &[RollNote], ghosts: &[GhostNote]) -> PianoRollOutput`
  with the other tracks' notes. `neu::GhostNote { start: u8, length: u8, pitch: u8, colour:
  Color32 }` (`GhostNote::from_note(&RollNote, Color32)`, `.as_roll_note()`); build the slice
  per frame from every track except `edit_track`, colour = `theme.track_colour(track.colour)`.
- Ghosts are painted after the grid lines and before the editable notes: the colour at
  `neu::GHOST_OPACITY` (0.35, `neu::ghost_fill(colour)`), `neu::GHOST_RADIUS` (3 pt) corners
  and a 1 pt outline at `neu::GHOST_OUTLINE_OPACITY` (0.6, `neu::ghost_outline(colour)`).
  They are never hit-tested or selected and never reach `PianoRollOutput`: a press on a ghost
  cell draws a note there like an empty cell. Ghosts off the visible rows (hidden black keys
  included) or starting at or past `model.steps` are not drawn; one running past the grid end
  is clipped. `neu::ghost_rects(&PianoRollModel, grid: Rect, &[GhostNote]) ->
  Vec<(Rect, Color32)>` is the pure rule (unit-tested).
- Track colours: `neu::TRACK_COLOURS = 8` and `Theme::track_colour(&self, index: u8) ->
  Color32` (index wraps mod 8; swatch 0 is the accent) in `crates/neu/src/theme.rs`. Dark:
  `FF7A1A FFC83C 4ADE80 2DD4BF 38BDF8 818CF8 F472B6 F87171`; light: `1E5BFF B45309 15803D
  0F766E 0284C7 6D28D9 BE185D DC2626`. Tested pairwise distinct and >= 1.5:1 against the
  surface in both themes (`neu::contrast_ratio`); the 35 % ghost composite is >= 1.2:1.
- The gallery's PIANO ROLL panel shows two ghosts (tracks 1 and 2); the hint line ends in
  "dim = other tracks".

## M11 additions (FL-style mixer)
- `inserts.rs`: `INSERT_COUNT = 16`, `Insert { level, pan, mute, solo, sends, eq3: Eq3,
  slots: [Slot; 3], route: Route::{Master, Insert(u8 > self)} }`, `Slot::{Off, Compressor(..),
  Drive(..), Lofi(..), Chorus(..), Eq3(..)}` with fixed-size parameter arrays; sources carry
  `insert: u8` (`0` = master). `fx/compressor.rs`, `fx/eq3.rs`. Render: sources -> their
  track inserts -> insert buses (summed, processed in ascending number order so a route to a
  higher insert is already available) -> master. Project v8.

## M10 as built (engine)

Everything below lives in `crates/engine`; the app and neu crates are untouched by this pass
(the app does not compile against it until adapted). The v6 demo project renders bit for bit
as before the refactor (`tests/tracks.rs`,
`v6_project_renders_sample_identical_after_the_track_refactor`: FNV-1a hash and per-100 ms
RMS of a 1 s render captured pre-refactor from `tests/fixtures/v6_demo.json`).

### `track.rs` (new; re-exported at the crate root)
- `pub const MAX_TRACKS: usize = 8`, `pub type TrackId = u8`, `pub const MAX_NAME_LEN: usize
  = 16`, `pub const COLOUR_COUNT: u8 = 8`, `pub const fn track_index(TrackId) -> usize`
  (wraps), `pub fn default_name(slot) -> String` (`"Synth"` for slot 0, `"Track N"` else),
  `pub fn default_track_params() -> BTreeMap<String, f32>`.
- `pub struct SynthInsert` (`drive -> lo-fi -> chorus -> x ducker`, mono in / stereo out; the
  drive amount/mix smoothers live here and tick once per `process`, *after* the drive used
  them, which is what keeps the pre-M10 sample sequence): `new(sample_rate)`,
  `set_param(ParamId, f32) -> bool` (only `is_synth_insert` ids; `SidechainAmount` /
  `SidechainRelease` reach the ducker at once), `param(ParamId) -> f32`,
  `process(x: f32) -> (f32, f32)`, `trigger_ducker()`, `reset()`, `snap()`, `sample_rate()`,
  `drive()` / `lofi()` / `chorus()` / `ducker()` accessors.
- `pub struct Track { pub active: bool, pub pool: VoicePool, pub insert: SynthInsert, pub
  params: [f32; ParamId::COUNT] }`: `new(sample_rate)` (inactive, defaults),
  `set_param(id, v) -> f32` (clamps, stores, routes to pool or insert; global ids ignored),
  `param(id)`, `synth_params() -> &SynthParams`, `reset_params()`,
  `copy_settings_from(&Track)`, `is_sounding()`.
- `pub struct TrackState { pub slot: u8, pub name: String, pub colour: u8, pub active: bool,
  pub preset: String, pub params: BTreeMap<String, f32> }` (serde, `#[serde(default)]`;
  `Default` = `TrackState::new(0)`): `new(slot)` (active, default name, colour = slot % 8,
  preset `""`, every `is_track` id at its default), `from_track(slot, &Track)`,
  `param(id)`, `set_param(id, v)`, `apply_to(&mut Track)`, `normalise()` (slot/colour wrap,
  name truncated to 16 chars, only track ids kept, missing ones filled).

### `params.rs`
- `ParamId::is_synth_insert()` = DriveAmount, DriveMix, Lofi*, Chorus*, SidechainAmount,
  SidechainRelease. `ParamId::is_track()` = `is_synth() || is_synth_insert()`.
  `ParamId::is_global()` = `!is_track()` (delay, reverb, master gain, limiter).
- `ParamId::is_fx()` now means "what `FxChain` processes": drive (its M2 serial path), delay
  and reverb. Lo-fi / chorus / sidechain are **not** `is_fx` any more. The app's FX rack
  should group by `is_synth_insert()` (per track, says the track name) versus `is_global()`
  (shared).

### `fx/chain.rs`
- `FxChain` keeps drive (serial `process` only), delay, reverb and `process_bus`. Removed:
  `process_synth_insert`, `set_ducker`, `ducker_trigger`, `lofi()`, `chorus()`, `ducker()`.
  Use the per-track `SynthInsert` instead (`engine.track(t).insert`).

### `pattern.rs`
- `pub struct NoteTrack { pub notes: [Note; MAX_NOTES], pub note_count: u8 }` (`Copy`,
  `NoteTrack::EMPTY`): `notes() -> &[Note]`, `is_empty()`, `note_at(step, pitch)`,
  `clear()`, `normalise()`, `insert(Note) -> Option<usize>` (no pattern-length check),
  `remove(index) -> bool`, `remove_starting_at(step)`. `Note::EMPTY_SLOT` is the unused-slot
  value.
- `Pattern` fields: `length`, `drums`, `synth` (the lane), **`lane_track: TrackId`** (which
  track `synth` mirrors; not serialised, 0 after a load), **`tracks: [NoteTrack;
  MAX_TRACKS]`** (replaces `notes` / `note_count`), `drum_volume`, `drum_tune`. `PartialEq`
  is manual and ignores `synth` and `lane_track` (they are views of the content).
- Note helpers take the track first: `track(t) -> &NoteTrack`, `notes(t) -> &[Note]`,
  `note_count() -> usize` (over all tracks), `add_note(t, note)`, `remove_note(t, index)`,
  `set_note(t, index, note)`, `note_at(t, step, pitch)`, `clear_notes(t)`,
  `clear_all_notes()`, `swap_tracks(a, b)`, `set_synth_step(t, step, note, accent)`,
  `rebuild_lane(t)` (sets `lane_track = t` and rebuilds; every helper rebuilds the lane for
  the current lane track). `clear_rows()` (drums only) is new; `clear_all()` = rows plus
  every track's notes; `clear_synth()` is gone. `is_empty()` / `used_steps()` consider every
  track. The app should call `pattern.rebuild_lane(edit_track)` on its own copies.
- Serde: a pattern writes `"tracks": [{ "track": n, "notes": [...] }, ...]` for non-empty
  tracks only (no `tracks` key when there are none; `notes` is never written). On load
  `tracks` wins; else a v5/v6 `notes` array lands on track 0; else a v4 `synth` lane is
  converted onto track 0. A track index >= 8, or more than 64 notes in one track, is an
  error; duplicate entries for a track merge.

### `mixer.rs`
- `pub enum Channel { Kick, Snare, Clap, ClosedHat, OpenHat, Track(u8) }`,
  `Channel::COUNT = 13`, `Channel::ALL` (drums, then `Track(0)..Track(7)`), `index()` (track
  `t` -> `5 + t`, wraps), `from_index`, `from_row`, `drum_row()`, **`from_track(TrackId)`**,
  **`track() -> Option<TrackId>`**, `label()` (`"T1"`..`"T8"`), `key()`
  (`"track_1"`..`"track_8"`). `#[allow(non_upper_case_globals)] pub const Synth: Channel =
  Channel::Track(0)` keeps old call sites compiling (as a `const` it does not make a `match`
  exhaustive, so update matches to `Channel::Track(_)`). Serde of `Channel::Track(n)` is
  externally tagged: `{"track": n}`.
- `ChannelParam::default_value`: every `Track(_)` gets delay send 1.0 and reverb send 1.0
  (what `Synth` had).
- `MixerState.strips: [Strip; 13]`; a custom `Deserialize` pads a shorter `strips` array
  with the missing channels' defaults (a 6-strip v6 mixer maps its synth strip onto
  `Track(0)`); more than 13 is an error. New `MixerState::swap(a, b)`.

### `engine.rs`
- New `Message` variants (all `Copy`, `size_of::<Message>() <= 16`): `SetEditTrack(u8)`,
  `SetTrackParam { track: u8, id: ParamId, value: f32 }`, `SetTrackActive { track: u8, on:
  bool }`, `CopyTrack { from: u8, to: u8 }`, `SwapTracks(u8, u8)`. Changed: `SetNote {
  pattern, track, index, note }`, `RemoveNote { pattern, track, index }`, `ClearNotes {
  pattern, track }` (a struct variant now). Unchanged in shape but **edit-track scoped**:
  `NoteOn`, `NoteOff`, `AllNotesOff` (every track), `SetParam` (per-track id -> edit track;
  global id -> as before), `SetSynthStep`, `ClearSynth`, `ClearAll` (rows plus the edit
  track's notes only). MIDI-queue notes go to the edit track too.
- Engine state: `tracks: Box<[Track; MAX_TRACKS]>` (boxed once in `new_engine`; `Engine`
  stays under 256 KiB), `edit_track`, per-track `seq_active` / `kbd_held`, a `draining`
  flag per track (a switched-off track renders until its voices fade, then costs nothing).
  `new_engine` starts with track 0 active, the rest inactive, edit track 0.
- Render per sample: drums -> rows; every active (or draining) track: `pool.tick()` ->
  `insert.process(x)` -> `inputs[Channel::Track(t).index()]`; a kick hit calls
  `insert.trigger_ducker()` on every active track. The sequencer fires each active track's
  notes of the playing pattern (per-track note-off bookkeeping, the M7 timing rules).
- Direct setters / accessors (mirror the messages): `edit_track() -> TrackId`,
  `set_edit_track(t)` (rebuilds every pattern's lane for `t`), `tracks() -> &[Track;
  MAX_TRACKS]`, `track(t) -> &Track`, `track_active(t) -> bool`, `active_track_count()`,
  `track_params(t) -> &[f32; ParamId::COUNT]`, `track_param(t, id) -> f32`,
  `set_track_param(t, id, v) -> f32`, `set_track_active(t, on)`, `copy_track(from, to)`,
  `swap_tracks(a, b)` (the edit track follows its content: `a` <-> `b`),
  `track_note_on(t, note, vel)`, `track_voices(t) -> &VoicePool`, `total_active_voices()`,
  `set_note(pattern, track, index, note)`, `remove_note(pattern, track, index)`,
  `clear_notes(pattern, track)`, `clear_synth()`, `clear_all()`. `param(id)` returns the
  edit track's value for a per-track id; `voices()`, `active_voices()`, `synth_params()`
  read the edit track. `note_on` on an inactive edit track is ignored; `note_off` releases
  the note on the edit track and on any track still holding it from the keyboard.
  `set_bank` / `set_pattern` rebuild the lanes for the edit track.
- `set_track_active(t, false)` also resets the slot's parameters to the defaults (kept on
  purpose: the app's remove-track mirrors it, and `Project::apply_to` orders its work
  "listed slots' params, then the bank, then the off switches" because off wipes params and
  notes; reordering either side changes what a loaded project sounds like).
- `Preset::apply_to_track(&self, engine, track)` (per-track ids only); `Preset::apply`
  still resets every id and lands the per-track ids on the edit track.

### `project.rs` (format version 7)
- `FORMAT_VERSION = 7`. `Project` fields: `version`, `bpm`, `swing`, `bank`, `params`
  (**global ids only**), `mixer` (13 strips), `eq`, `reverb`, **`tracks: Vec<TrackState>`**
  (`#[serde(default = one "Synth" track)]`; an explicit `[]` means no track; sorted by
  slot, one entry per slot after a load), `octave`, `velocity`. **`preset_index` is gone**
  (the preset name lives in `TrackState.preset`).
- `Project::from_engine(engine, octave, velocity)` (no preset index): writes every active
  track with `TrackState::from_track` defaults for name / colour / preset; the app decorates
  them through `track_mut(slot)` before saving (or builds the `Project` itself, as
  `build_project` does today). `apply_to` pushes globals, mixer, reverb, EQ, the active
  tracks' params, then the bank, then switches every unlisted slot off (which also drops
  notes the file held on an inactive slot) and moves the edit track to the first active
  slot if it ended up inactive.
- Helpers: `track(slot) -> Option<&TrackState>`, `track_mut(slot)`, `track_or_insert(slot)`,
  `track_param(slot, id)`, `set_track_param(slot, id, v)`. `param(id)` / `set_param(id, v)`
  route a per-track id to slot 0 (the pre-M10 synth) as a compatibility convenience.
- Loading v6 and older: the file's track-id `params` plus `preset_index` (mapped to
  `presets::ALL[i].name`, else `""`) become `tracks = [TrackState { slot: 0, name: "Synth",
  colour: 0, active: true, .. }]`; the patterns' `notes` land on track 0 (pattern
  deserializer) and the 6-strip mixer pads (mixer deserializer). The "refuse newer" probe is
  version 8 now.
- On-disk shape of a track entry: `{ "slot": 0, "name": "Synth", "colour": 0, "active":
  true, "preset": "Bass", "params": { "amp_attack": 0.01, ... per-track ids only } }`.

### Tests
- `tests/tracks.rs` (12, plus 1 ignored): the v6 regression, the v8 layered regression
  (`tests/fixtures/v8_layered_demo.json`: Keys through lo-fi bits 6 / rate 3 / mix 0.8 and
  a chorus, Lead with drive 0.9 / mix 0.7 and sidechain 0.7 under a kick; FNV-1a hash and
  per-100 ms RMS pinned, so a refactor of the insert ordering is caught), sum-of-tracks
  linearity, insert isolation (track 2 wet through lo-fi + chorus, track 1 dry: still
  additive within 1e-5, and muting the wet track leaves the dry one bit-identical), mute
  isolation, note isolation, `SetEditTrack` routing, `SetTrackActive off`, `CopyTrack`,
  `SwapTracks`, the 8-track v7 round trip, a finite long render under every lifecycle
  message, and `#[ignore] eight_tracks_render_faster_than_real_time` (1 s of 8-track audio
  in about 0.06 s in release; gate `< 0.5 s`). Every queue-driven test asserts
  `handle.send(..)` (a full queue fails the test instead of passing vacuously).
- `tests/engine_alloc.rs`: counting global allocator; `render` plus every track message with
  8 active tracks is allocation-free (every send asserted).
- `tests/fx_m8.rs` insert tests now drive a `SynthInsert`; `tests/fixtures/v6_demo.json` is
  the pre-M10 `Project::default()`.

## M10 as built (app, part 1)

The app compiles against the M10 engine again and exposes the track model in the existing
panels; the TRACKS panel (`crates/app/src/ui/tracks.rs`, the right-column `Tracks` tab) is
part 2 and is not here. Everything below lives in `crates/app`.

### State (`app.rs`)
- `pub struct TrackUi { pub name: String, pub colour: u8, pub active: bool, pub preset:
  usize, pub modified: bool }` (`Clone`, `PartialEq`, `Eq`, `Debug`): `TrackUi::inactive(slot)`,
  `TrackUi::fresh(slot)` (active, `engine::track::default_name`, colour = slot % 8, preset 0),
  `preset_name() -> &'static str`.
- `AppState.tracks: [TrackUi; MAX_TRACKS]`, `AppState.track_params: [[f32; ParamId::COUNT];
  MAX_TRACKS]` (only the `ParamId::is_track()` entries are meaningful; the global entries of
  a track's array stay at their defaults), `AppState.edit_track: TrackId` (always an active
  slot). `AppState.params` keeps the **global** ids only (its per-track entries stay at the
  defaults). Helpers: `default_tracks()`, `default_track_params()`, `preset_params(index)`
  (defaults + preset), `preset_track_params(index)` (defaults + the preset's per-track ids),
  `track_differs_from_preset(&params, index) -> bool`, `clean_track_name(&str) -> String`
  (trim, <= `MAX_NAME_LEN` = 16 chars).
- Reads: `param(id)` (the edit track's value for a per-track id, else the global),
  `track_param(slot, id)`, `track(slot) -> &TrackUi`, `edit_track_ui() -> &TrackUi`,
  `active_tracks() -> impl Iterator<Item = TrackId>` (slot order), `active_track_count()`,
  `track_active(slot)`, `free_track_slot() -> Option<TrackId>`,
  `nearest_active_track(slot) -> Option<TrackId>`, `edit_notes() -> &[Note]` (the edit
  track's notes of the edit pattern), `track_notes(slot) -> &[Note]`, `preset_index()` (the
  edit track's preset), `preset_name()`, `preset_modified()`.
- Writes: `set_param(id, v)` sends `SetTrackParam { track: edit_track, .. }` for a per-track
  id (a real change sets `tracks[edit].modified`) and `SetParam` for a global id;
  `apply_preset(index)` applies to the edit track (its per-track ids = defaults + preset, its
  `preset` set, `modified` cleared) and resets the globals as before M10 (presets carry
  reverb / delay values), sending one message per id. `resend_track_params(slot)`;
  `resend_all_params()` = every global `SetParam` + every active track's `SetTrackParam`s;
  `resend_all()` sends `SetTrackActive` for all eight slots first (off drops that slot's
  notes in the engine, so this precedes the bank), then the params, `SetEditTrack`, bpm,
  swing, mixer (78 values), EQ, reverb, bank. `track_messages(&tracks, &track_params,
  edit_track) -> Vec<Message>` is the pure form (tests count with it).
- Track operations (the TRACKS panel calls these; all but `select_track` are one undo step
  and mark the project dirty):
  - `add_track() -> Option<TrackId>`: first unused slot, `TrackUi::fresh`, params =
    `preset_track_params(0)`, `SetTrackActive` on + its params, then selected. `None` with a
    status line when all eight slots are used.
  - `duplicate_track(from) -> Option<TrackId>`: settings, preset and modified marker into the
    first unused slot as `"<name> copy"` (cut to 16 chars), colour = slot, `SetTrackActive`
    on + `CopyTrack`, selected; notes are not cloned. `None` for an unused source or no room.
  - `remove_track(slot) -> bool`: refuses the last active track (status line); clears the
    slot's notes in every pattern mirror and the clipboard, resets its params and (M10 review)
    its mixer strip on both sides (`Strip::for_channel` + six `SetMixer`s), sends
    `SetTrackActive` off; if it was the edit track the nearest active slot is selected.
  - `rename_track(slot, &str) -> bool` (trimmed, <= 16 chars; empty / same / unused = no
    edit), `set_track_colour(slot, colour) -> bool` (wraps mod 8).
  - `select_track(slot) -> bool` (view state, no undo step): releases held keys
    (`release_held_notes`), drops the bass and roll selections, `rebuild_lanes()`, sends
    `SetEditTrack`. `select_adjacent_track(delta: i32) -> bool` walks the active tracks in
    slot order and wraps (Alt+Up = -1, Alt+Down = 1).
  - `move_track(slot, up: bool) -> bool`: swaps with the nearest *active* slot in that
    direction (mirrors, params, notes in every pattern and the clipboard, mixer strips),
    the selection follows the moved track, sends `SwapTracks`.
  - `rebuild_lanes()` (every pattern's and the clipboard's lane for the edit track),
    `release_held_notes()`.
- Notes: every note edit (`add_note`, `set_note`, `remove_note`, `clear_notes`, `move_notes`,
  `remove_notes`, `add_chord`, `set_synth_step`, `clear_synth`, drag-paint, overdub, the
  click-then-key entry) addresses `edit_track`; `note_messages(pattern, index, track)`
  replays one track (`ClearNotes { pattern, track }` + `SetNote { track, .. }`), and
  `pattern_messages` replays every track that holds notes after `ClearPattern` (which
  empties all tracks in the engine). `paste_pattern` / `set_pattern` rebuild the lane.
- Undo: `Snapshot` (`undo.rs`) gained `tracks`, `track_params` and `edit_track`, and its
  `bank` is boxed (`Box<Bank>`, 40 KiB); `restore` puts the mirrors back, moves the
  selection to the snapshot's edit track (or the first active slot), rebuilds the lanes and
  `resend_all`s. `apply_project` releases held keys and rebuilds the lanes too.
- Project (`project.rs`): `build_project` writes the global ids into `params` and one
  `TrackState` per active slot (`track_state(slot)`: name, colour, the preset *name* even
  when modified, every per-track id); `apply_project` loads the globals, every listed active
  slot (preset found by name, else by matching the parameters against the presets, else 0;
  `modified` = the per-track ids differ from that preset's), switches unlisted slots off
  (and drops their notes from the bank mirror), keeps one active track at least, and selects
  the first active slot. The eframe-persisted preset (`preset_index`, then the M9 name) is
  no longer re-applied at startup (M10 review): the project carries one preset per track.
- Shortcuts (`shortcuts.rs`): `Shortcut::AddTrack` (Ctrl+T), `DuplicateTrack`
  (Ctrl+Shift+D), `PrevTrack` (Alt+Up), `NextTrack` (Alt+Down); `BeatSlopperApp::run_shortcut`
  dispatches them to `add_track`, `duplicate_track(edit_track)`, `select_adjacent_track`.
  Suppressed while a text field has focus like every other shortcut.
- Stack: `app::new_engine(sample_rate)` builds the engine on a helper thread with
  `ENGINE_BUILD_STACK` (16 MiB): `engine::new_engine` builds its `[Track; 8]` on the stack
  before boxing it, which the debug build's main thread (1 MiB, already deep in the event
  loop) could not hold once the bank grew to 40 KiB. `BeatSlopperApp.state` is boxed, the
  audio `Renderer` boxes its engine, `ExportJob::spawn` takes a `Box<Project>` and the export
  thread gets the same stack size. `--smoke-test` passes in debug again.

### Panels
- SYNTH (`ui/synth_panel.rs`): the head is the edit track's colour chip
  (`controls::track_chip`, 10 pt, accessible as `"<name> colour"`) and the title
  `head_title(state)` = `"Synth · <name>"`; flush right the preset selector (the edit track's
  preset; a click applies to it) and the `MODIFIED_LABEL` (`"Modified"`) lamp
  (`preset_modified()`).
- FX rack (`ui/fx_rack.rs`): DRIVE, LO-FI, CHORUS and SIDECHAIN (the `is_synth_insert` ids)
  use `track_group_head`: the caps label plus `controls::track_tag` (chip + name, tooltip
  "Edits track <name> ..."); DELAY, REVERB and MASTER are unchanged.
- MIXER (`ui/mixer.rs`): `visible_channels(state)` = the five drum channels then the active
  tracks' channels in slot order; strips are laid out inside a horizontal `ScrollArea`
  (`strips_width(count, strip_width)`, scrolls once wider than the panel; the default six
  strips plus master fill it exactly). A track strip's fader label and accessible names use
  the track's name (`channel_name(state, channel)`, `strip_label`, `control_name(state,
  channel, param)`), and its colour chip sits in the M / S column above the fader
  (`CHIP_RISE`). `DRUM_STRIP_LABELS` / `DRUM_CHANNEL_NAMES` replace the old 6-entry arrays.
- SEQUENCER (`ui/sequencer.rs`): the head shows `controls::track_tag` after the `GRID | ROLL`
  tab; the bass lane and the roll edit `edit_notes()`; `roll_ghosts(state, theme) ->
  Vec<GhostNote>` builds every other active track's notes in their colours per frame and
  both roll sites call `neu::piano_roll_with_ghosts`. `qol::row_channel(row, edit_track)`
  maps the bass lane to `Channel::from_track(edit_track)` for the label's mute / solo.
- `controls.rs`: `TRACK_CHIP` (10), `TRACK_CHIP_GAP` (6), `track_chip_name(name)`,
  `track_chip(ui, theme, colour, name)`, `track_tag(ui, state, theme)`.

### Tests
- `app.rs`: routing of `set_param` to the edit track, note edits per track (overdub included),
  select releases held keys, duplicate copies settings only, remove + undo restores the notes
  on both sides, rename / colour / move with undo, all eight slots, `pattern_messages` per
  track, the preset helpers. `project.rs`: a two-track project through build / apply, a file
  and undo, plus a v6 file into track 0. `shortcuts.rs`: the four new bindings.
  `ui/mixer.rs`: names and visible channels.
- `tests/ui_renders.rs`: the SYNTH head follows add / select and the preset row edits the
  edit track (MODIFIED lamp state via accesskit), the FX rack's four chips, the mixer's one
  strip per active track (overflow scrolls, removal hides), ghosts in both themes (a click on
  a ghost cell draws on the edit track), the track shortcuts through the app harness, and
  the 1280 x 1140 whole-sequencer visibility with three tracks in the mixer.
- Gate: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` (app 258, engine 379 + 1 ignored, neu 118), `--smoke-test` ok.


## M9 as built (engine presets)

Everything lives in `crates/engine/src/presets.rs` (data only, no new DSP) and
`crates/engine/tests/presets.rs`; the app and neu crates are untouched by this pass (the M9
app agent replaces the synth panel's 8-item row with a browser over `LIBRARY`).

### API (`engine::presets`)
- `pub enum Category { LofiKeys, LofiGuitars, LofiPads, EdmLeads, EdmBass, EdmPlucks,
  EdmPads, Risers }` (`Copy`, `Eq`, `Hash`): `Category::ALL: [Category; 8]` (browser order),
  `label() -> &'static str` ("Lofi Keys", "Lofi Guitars", "Lofi Pads", "EDM Leads", "EDM
  Bass", "EDM Plucks", "EDM Pads", "Risers & FX"), `index() -> usize` (position in `ALL`),
  `from_index(usize) -> Category` (wraps).
- `pub struct Preset { name: &'static str, category: Category, tags: &'static [&'static
  str], values: &'static [(ParamId, f32)] }`. Unchanged: `apply(&mut Engine)` (resets every
  parameter, global ones included, then applies; track ids land on the edit track),
  `apply_to_track(&mut Engine, TrackId)` (track ids only, globals untouched), `value(ParamId)
  -> f32` (listed value clamped, else the default). New: `has_tag(&str) -> bool` (ASCII
  case-insensitive), `is_drone() -> bool` (tag `"drone"`), `has_vinyl_noise() -> bool`
  (`LofiNoise > 0 && LofiMix > 0`: the track hisses even with no note), `lists(ParamId) ->
  bool`.
- `pub const ALL: &[Preset]`: the original eight (Bass, Sub, Lead, Pluck, Pad, Keys, Acid,
  Bell) with their M2 names and values, now carrying a category and tags; **not** part of
  `LIBRARY` (the app's `presets::ALL.len() == 8` assertion still holds).
- `pub static LIBRARY: &[Preset]` (91 entries, grouped contiguously in `Category::ALL`
  order), `pub const LIBRARY_COUNT: usize` (91).
- `by_category(Category) -> impl Iterator<Item = &'static Preset>` (library order),
  `find(&str) -> Option<&'static Preset>` (ASCII case-insensitive; `LIBRARY` first, then
  `ALL`), `index_of(&str) -> Option<usize>` (index into `LIBRARY`; `None` for the original
  eight), `search(&str) -> Vec<&'static Preset>` (case-insensitive substring on the name and
  every tag, trimmed; empty query = the whole library; library order).
- Tags in use: mood (`warm`, `dark`, `bright`, `soft`, `dreamy`, `aggressive`), genre
  (`lofi`, `edm`, `trap`, `house`, `trance`, `dubstep`, `dnb`, `future bass`, `synthwave`,
  `joji`), type (`ep`, `piano`, `keys`, `guitar`, `pad`, `lead`, `bass`, `sub`, `808`,
  `pluck`, `riser`, `fx`, `drone`, `noise`, `vinyl`, `sidechain`).

### Library (91 presets: 11 per category, 12 in Lofi Guitars, EDM Bass and Risers & FX)
- **Lofi Keys:** Dusty Rhodes, Bedroom Keys, Tape Wurli, Lofi Piano, Cassette Keys, Rainy
  Day EP, Glass EP, Toy Piano, Organ Dust, Music Box, Vapor Keys.
- **Lofi Guitars:** Nylon Guitar, Bedroom Strat, Muted Pluck Gtr, Jazz Hollowbody, Tape
  Telecaster, Ukulele Lofi, Harp Dust, Koto Lofi, Slow Swell Gtr, Bass Guitar Lofi, Dream
  Clean, Tremolo Dirt (12).
- **Lofi Pads:** Night Drive Pad, Warm Tape Pad, VHS Strings, Underwater Choir, Dusty Organ
  Pad, Bedroom Strings, Hollow Moon, Lofi Flute Pad, Grainy Pad, Sunset Drive, Wobble Pad.
- **EDM Leads:** Festival Saw, Supersaw Lead, Laser Lead, Hoover, Future Bass Chord, Trance
  Lead, Square Lead, FM Bell Lead, Screech Lead, Pluck Lead, Big Room Lead.
- **EDM Bass:** Trap 808, 808 Sub, Reese Bass, Neuro Growl, Wobble Bass, Deep House Sub,
  Future House Bass, Fat Mono, FM Bass, Dub Sub, Tape 303, Distorted 808 (12).
- **EDM Plucks:** Glass Pluck, Progressive Pluck, Trance Pluck, Pizzicato, Kalimba, Marimba,
  House Stab, Water Drop, 8-Bit Pluck, Harp Pluck, Rain Pluck.
- **EDM Pads:** Big Pad, Anthem Pad, Analog Strings, Deep Pad, Trance Gate, Hollow Pad,
  Future Pad, Sine Bed, Choir Pad, Dark Pad, Shimmer Pad.
- **Risers & FX:** White Riser, Pink Riser, Sweep Up, Downlifter, Impact Sub Drop, Dub Echo
  (drone), Infinite Hall (drone), Alarm Trill, Wind, Vinyl Crackle (drone), Laser Zap,
  Thunder Boom (12).

Every library preset lists every insert id (drive, lo-fi, chorus, sidechain) and the sends
(delay/reverb; globals, applied by `apply` but ignored by `apply_to_track`). Data is written
with the file-local `values![Osc1Wave: 1.0, ...]` macro (expands to `&[(P::Osc1Wave, 1.0),
...]`; rustfmt leaves it alone). Design rules used: amp release ends the voice at -80 dB
after `AmpRelease` s; a synced delay at 120 BPM must repeat 64/32/16/8 times in 8 s, so
feedback stays `<= 0.7 / 0.6 / 0.5 / 0.3` for delay times 0/1/2/3; the reverb comb feedback
is `0.7 + 0.28 x size`, so sizes `<= 0.9` die within 8 s and `1.0` does not. The three
drones are the presets whose *global* delay/reverb (or the vinyl bed) outlast 8 s.

### Tests (`tests/presets.rs`, 20; `cargo test -p engine` = 401)
- Per category (8 tests, parallel): >= 10 presets; each applies, a 1 s C4 note renders
  finite and non-silent (RMS > 1e-3), peak <= 1.0 after the limiter, and after release a
  full 1 s window drops below `max(1e-4, 2 x noise bed)` within 8 s, where the noise bed is
  the preset's own no-note render (the lo-fi insert's vinyl hiss is not gated by the voices,
  so presets with `has_vinyl_noise()` decay to their bed, not to zero); a `drone` must still
  be above 1e-4 after 8 s.
- Library shape: >= 80 entries grouped by category; names unique across `LIBRARY` + `ALL`
  (`ALL` keeps its eight names in order); every value finite, in range, listed once, never
  `MasterGain`/`LimiterOn`; every preset lists `DriveMix`, `LofiMix`, `ChorusMix`,
  `SidechainAmount`; pairwise >= 4 differing `is_track()` values (`LIBRARY` + `ALL`).
- Helpers: `find`/`index_of` agree (case-insensitive); `search("piano")` finds "Lofi Piano"
  and "Toy Piano" and only piano-named/tagged presets; empty query = everything; order kept;
  `apply_to_track(.., 2)` sets every track id to `preset.value(id)` and leaves the globals
  and track 0 alone.
- Character: "Nylon Guitar" is pluck mode and its tail (<= 1.5 s) is shorter than "Night
  Drive Pad"'s; "Trap 808" at C2 has < 5 % of its energy above 200 Hz (naive Hann DFT);
  "Festival Saw"/"Supersaw Lead" are seven-voice saws and the lead is > 2x brighter than
  the lofi pad (`brightness`); every Lofi Keys preset has `LofiMix > 0`, `LofiBits < 16`
  and vinyl noise, and bypassing the lo-fi changes "Dusty Rhodes"; "White Riser" is noise
  that swells (late RMS > 2x early) and brightens; the three drones keep sounding.

## M10 as built (app, part 2: TRACKS panel)

The "easy list" of instrument tracks, modelled on FL Studio's Channel Rack and Ableton's
track list: `crates/app/src/ui/tracks.rs`, shown in the right-hand column under
`RightTab::Tracks` (`ui/mod.rs` dispatches it like the other three).

### The head: four short tabs
- `controls::RIGHT_TABS` is now `["Proj", "Chain", "Vis", "Tracks"]` (the full words,
  `Project | Chain | Visual | Tracks`, are about 236 pt and overflow the column's 220 pt
  inner width; the short set is about 200). The same `controls::panel_head` serves all four
  panels, so every head reads the same; its accessible name is the joined items
  (`"Proj / Chain / Vis / Tracks"`, a radio group whose items are `"Proj"` ... `"Tracks"`).
  Song mode still flips the tab to `Chain` and pattern mode back to `Proj`; the tab is not
  persisted (it never was: the play mode decides it at load).
- `Ctrl+T` (add) and `Ctrl+Shift+D` (duplicate) switch the column to the TRACKS tab when
  they succeed (`BeatSlopperApp::run_shortcut`), so the new row is visible.

### The list (`tracks::show`)
- One row per active track in slot order, `tracks::ROW_HEIGHT` (34) tall, inside a vertical
  `ScrollArea` (`"tracks_list"`) that takes whatever height is left over the footer. At the
  1280 x 1140 design size in GRID view the panel is as tall as the SEQUENCER beside it
  (about 338), which leaves room for `VISIBLE_ROWS_AT_DESIGN_SIZE` (5) rows: with six to
  eight tracks the list scrolls; the ROLL view's taller panel shows more.
- Row anatomy, left to right (fixed widths, the name block takes the rest, 76 pt at the
  column width): the colour chip (10 pt circle in `theme.track_colour`, a click cycles the
  eight swatches through `set_track_colour`; a hover ring in the swatch colour); the name
  (13 pt, text-primary when selected or hovered, else secondary) over the preset name
  (11 pt secondary), both cut with an ellipsis (`…`) to the block (the full text is the
  tooltip); a painted pencil (14 pt hit box; the kit has no pencil icon, the glyph is three
  line segments); the 22 pt `M` / `S` squares (`neu::toggle_button_sized`, bound to the
  track's strip `Channel::from_track(slot)` through `toggle_mute` / `toggle_solo`); the
  40 x 22 `Roll` pill (lit while the track is selected and the lane view is ROLL; a click
  does `select_track` then `set_lane_view(Roll)`).
- The name block (not the whole row) is the row's button: a click selects
  (`select_track`, no undo step), a double-click opens the rename editor. The selected row
  sits on an inset-s well (`paint_soft_rect`, radius 10), the segmented control's inset
  track in the row's shape. The row allocation itself only senses hover (for the name's
  hover colour via `contains_pointer`), so a click anywhere lands on a control, never on
  dead space.
- Inline rename: the pencil (or the double-click) calls `neu::begin_edit` under
  `Id::NULL.with(("tracks_panel_rename", slot))`; `neu::show_edit` draws the editor over the
  name's rect (inset-s pill, 13 pt, 16-char limit = `MAX_NAME_LEN`). `Committed(text)` goes
  to `rename_track` (trimmed; empty or unchanged = no edit), `Cancelled` (Escape) changes
  nothing. While the editor has focus `ctx.text_edit_focused()` is true, so
  `take_shortcuts` returns nothing and `handle_keyboard` releases the piano, as for every
  text field. `tracks::is_renaming(ui, slot)` reports an open editor.
- Footer, replaced by the confirm while one is pending: row 1 `+ Add` (the primary pill: a
  `toggle_button` in its ON look, accessible as `"Add track"`; `add_track`, which refuses a
  ninth with a status line), `Dup` (disabled without a free slot; `duplicate_track(edit)`,
  "<name> copy"), `Del` (disabled with one active track; a click stores the edit track in
  `ui.data` under `"tracks_panel_pending_delete"`); row 2 the `▲` / `▼` move buttons (22 pt
  rounded squares with a painted triangle, raised-s / inset-s while pressed, dimmed when the
  selected track is already first / last; `move_track(edit, up)`) beside the hint
  `"Keys play the selected track · Ctrl+T adds"` (11 pt secondary, wraps to two lines).
  The confirm is the question `"Delete <name>?"` over `Yes` (`remove_track`, then the state
  selects the nearest track) | `No`; it is dropped by itself if the track goes away (undo,
  a load). Every control has a tooltip.
- `ui/sequencer.rs` head (refined here): `head_track_tag` replaces the plain
  `controls::track_tag`. The chip always shows; the caps name is laid out into the room
  left beside the right group (its width from the previous frame, the one
  `controls::right_aligned` stores under `"sequencer_head"`), cut with an ellipsis, and
  dropped below 24 pt of room (`TAG_NAME_MIN_WIDTH`). At 1280 wide the head has 22 pt to
  spare, so only the chip shows and the name is its tooltip; before this the tag made the
  head about 50 pt wider than its slot and the whole right-hand column hung off the window.

### Accessible names (`tracks::*`, for tests)
`row_name(index, name)` = `"Track row 1: Synth"` (a button, `toggled` = selected);
`swatch_name` = `"Synth swatch"`; `rename_name` = `"Rename Synth"`; `mute_name` /
`solo_name` = `"Mute track Synth"` / `"Solo track Synth"` (buttons, `toggled` = state; the
mixer's strips stay `"Mute Synth"`); `roll_name` = `"Roll: Synth"` (`toggled` = selected +
ROLL view); `ADD_LABEL` `"+ Add"` with `ADD_NAME` `"Add track"`; `DUP_LABEL` `"Dup"`;
`DEL_LABEL` `"Del"`; `delete_question(name)` = `"Delete Synth?"`; `YES_LABEL` / `NO_LABEL`;
`MOVE_UP_NAME` / `MOVE_DOWN_NAME` = `"Move track up"` / `"Move track down"`; `HINT`.

### Tests
- `tests/tracks_panel.rs` (8, kittest): the panel alone lists one row per active track in
  both themes without editing (names, every control, the selected row, the tab fits the
  column); a row click selects and the SYNTH head follows; ADD / DUP / DEL with the confirm
  (NO keeps, YES removes and selects the nearest, DEL disabled with one track, undo brings
  the row back); rename through the pencil (Space does not play and no piano key sounds
  while the field has focus, Enter commits, Escape cancels, Space plays again afterwards);
  M / S set the track's strip without selecting, ROLL selects and switches the view; the
  arrows reorder with the selection following and the chip cycles the colour; Ctrl+T shows
  the tab; and at 1280 x 1140 with eight tracks the whole sequencer stays visible, the
  pills and hint sit under the list, the ROLL pill is inside the page padding and a ninth
  ADD is refused. `ui/tracks.rs` unit tests: the names and the row budget.
- `tests/ui_renders.rs`: the two `radio("Project")` lookups read `RIGHT_TABS[0]`.
- Gate: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` (app 173 unit + 1 + 12 plugins + 8 tracks + 74 renders),
  `--smoke-test` ok.

## M9 as built (app browser)

The SYNTH panel's 8-item preset `segmented` is gone; `crates/app/src/ui/preset_browser.rs`
puts a preset strip in the panel head and a browser over the panel body. Everything is in
`crates/app`; the engine's `presets.rs` is untouched (its `ALL` / `LIBRARY` split stays).

### The unified list (`crates/app/src/presets.rs`)
- The app addresses presets by one index into `CLASSIC ++ LIBRARY`: the eight M2 classics
  (`engine::presets::ALL`, re-exported as `presets::CLASSIC`) first, so preset 0 is still
  "Bass" (a fresh track's sound, no vinyl bed) and every index the pre-M9 code, tests and
  v6 files used keeps its meaning, then the 91 library entries in library order (99 in
  all). Helpers: `count()`, `all()`, `get(i)`, `name(i)`, `index_of(name)` (ASCII
  case-insensitive, whole list), `matches(preset, query)` (name or tag substring, like the
  engine's `search`), `by_category(Option<Category>)` (unified indices; `None` = all),
  `filtered(category, query)` (the browser's list) and `step(filter, current, delta)`
  (wraps; a `current` outside the filter steps to its first / last entry).
- Folding the classics into the engine `LIBRARY` was considered and dropped: `ALL` is used
  by a dozen engine tests and the v6 project migration, the library's per-category render
  / decay tests would then judge the M2 sounds, and the classics are reachable through the
  same unified index anyway. A category filter therefore shows the engine's `by_category`
  entries plus the classics of that category (Lofi Keys +1 "Keys", EDM Leads +1 "Lead",
  EDM Bass +3 "Bass" / "Sub" / "Acid", EDM Plucks +2 "Pluck" / "Bell", EDM Pads +1 "Pad").

### State (`app.rs`)
- `TrackUi.preset: usize` is now an index into the unified list (`preset_name()` =
  `presets::name`); `preset_params` / `preset_track_params` / `track_differs_from_preset`
  and `apply_preset(index)` clamp to `presets::count()`.
- New: `apply_preset_named(&str) -> bool` (unknown name = no change),
  `preset_filter() -> Vec<usize>` (the category filter only), `preset_matches()` (filter +
  search text), `step_preset(delta) -> bool` (previous / next within the filter, wrapping,
  one undo step like `apply_preset`), `open_preset_browser(focus_search)`,
  `close_preset_browser()`.
- Browser view state (not undoable, not persisted): `preset_browser_open`,
  `preset_category: Option<Category>`, `preset_query: String`, `preset_search_focus`
  (a one-shot focus request the search field consumes). `handle_keyboard` closes the
  browser on Escape when no text field has the keyboard (the browser itself closes on
  Escape from inside the search field, see below).
- Persistence: none for the preset since the M10 review (the `PRESET_NAME_KEY` /
  `PRESET_INDEX_KEY` start-up re-apply to track 0 is gone, with the keys: the project file
  carries the preset per track and a fresh launch starts from preset 0; the browser's
  category and search text were never persisted).
- `project.rs`: `track_ui_from_state` finds the preset by name in the unified list
  (case-insensitive), else by matching the per-track parameters against every preset,
  else 0 with `modified` set, as before. `TrackState.preset` already carried the name, so
  the file format is unchanged.

### Layout (`ui/preset_browser.rs`, `ui/synth_panel.rs`)
- Head strip (`head_strip`, flush right before the MODIFIED lamp): `<` pill
  (`PREV_LABEL`), the name well (`NAME_WIDTH` = 128, inset-s pill, 13 pt, ellipsised, the
  full name and category as tooltip; a click opens the browser), `>` pill (`NEXT_LABEL`),
  the `BROWSE` toggle pill (`BROWSE_LABEL`, lit while open). `<` / `>` call
  `step_preset`, so they walk the category filter and wrap.
- The browser (`show`) replaces the panel *body* (the oscillator groups) while open, inside
  the same `neu::panel`, so the panel keeps the size the page gives it and nothing below
  moves (an `egui::Area` popup would have had to fight the main area's scroll and the
  window's bottom edge; an overlay over the whole SYNTH | FX row would have hidden the FX
  rack for no reason). Top to bottom with the group gap: the category `segmented`
  (`CATEGORY_LABELS` = `All · Lo Keys · Lo Gtr · Lo Pads · Leads · Bass · Plucks · EDM Pads
  · Risers`, about 500 pt, fits the 704 pt panel at the minimum window width; "FX" was
  avoided because the FX rack's `FX | Mixer` tab is a radio item of that name); the search
  row: the search field (`SEARCH_WIDTH` = 220, an `egui::TextEdit` on the kit's inset-s
  editor pill with the accent caret and the `SEARCH_HINT` placeholder, `SEARCH_CHAR_LIMIT`
  = 32), the match count (`count_text`: `"12 of 99"`) and the `CLOSE` pill flush right;
  then the list in a vertical `ScrollArea` (`"preset_browser_list"`), `columns_for(width)`
  columns of at least `COLUMN_MIN_WIDTH` (200) pt (three at 1280 wide), `ROW_HEIGHT` (26)
  rows: name left (13 pt, primary when selected or hovered, else secondary), tags right
  (11 pt secondary: `vinyl` where `has_vinyl_noise()`, `drone` where `is_drone()`, the
  category label in the `All` view; `entry_tags`), the selected entry on an inset-s well
  (`ROW_RADIUS` 8) with a 6 pt accent dot. A click applies the preset to the selected
  track and keeps the browser open. `NO_MATCH` shows for an empty list.
- Closing: Escape (checked by the browser itself, so it also works while the search field
  has the keyboard: egui drops the field's focus on Escape and `handle_keyboard` skips
  Escape while a field is focused), `CLOSE`, `BROWSE` again. The category and the search
  text are kept for the next time.
- Focus: while the search field has the keyboard `ctx.text_edit_focused()` is true, so
  `take_shortcuts` returns nothing (Space, R, the preset steps, every Ctrl combination)
  and `handle_keyboard` releases the piano, exactly like the TRACKS rename field.

### Shortcuts (`shortcuts.rs`)
- `Shortcut::PrevPreset` = Ctrl+PageUp, `NextPreset` = Ctrl+PageDown (`step_preset(-1 /
  1)`: the browser's category filter, wrapping; the search text does not narrow the
  stepping), `FindPreset` = Ctrl+Shift+P (`open_preset_browser(true)`: opens the browser
  with the search field focused; the old text stays and typing appends). The plan's Ctrl+Up
  / Ctrl+Down were taken by the QOL tempo nudge first (DECISIONS.md), so the preset steps
  use the page keys; plain PageUp / PageDown remain the selected note's octave nudge. Every
  earlier binding is unchanged.

### Accessible names
`head_name(preset)` = `"Preset: Lofi Piano"` (the name well, a button); `PREV_LABEL` `"<"`
/ `NEXT_LABEL` `">"`; `BROWSE_LABEL` `"Browse"` (`toggled` = open); `CLOSE_LABEL`
`"Close"`; the category items are radio buttons named by `CATEGORY_LABELS`; the search
field is the panel's only `TextInput`; every list entry is a button named after the preset
(`toggled` = selected). Note for tests: the sequencer's bass lane also has a button named
"Bass", so the whole-app tests scan entries inside `top_row_rect`.

### Tests
- `crates/app/src/presets.rs` (4): the unified order and lookups, category counts =
  engine `by_category` + classics, search like the engine's, `step` wrap semantics.
  `ui/preset_browser.rs` (3): category items round-trip and stay short, `entry_tags`,
  `columns_for`. `app.rs`: `every_preset_sends_all_params_and_stays_in_range` now runs
  over all 99; `apply_preset_named` / `step_preset` / filter / view state. `project.rs`:
  the two-track round trip uses "Nylon Guitar" by name; unknown names fall back (by
  parameters, else 0 + modified) and known names match case-insensitively.
  `shortcuts.rs`: the three new bindings beside the tempo nudge.
- `tests/preset_browser.rs` (6, kittest): the list shows all 99 in unified order and each
  category exactly (both themes); an entry applies to the selected track, the head follows,
  the other track keeps its preset, select shows the other track's preset, CLOSE / BROWSE /
  the name well open and close; search narrows (`piano`, a category on top, `808` by tag,
  nothing) and Escape from the field closes keeping the text; `<` / `>` wrap over the
  whole list and inside Lofi Guitars, the MODIFIED lamp lights on a knob move and clears
  on `>`; through the whole app Ctrl+PageDown / PageUp step and wrap, the tempo nudge is
  untouched, Ctrl+Shift+P opens with the field focused, typed letters and Space neither
  play nor start the loop, the preset step shortcut is quiet while typing, Escape closes
  and Space plays again; at 1280 x 1140 the whole sequencer stays visible with the browser
  open and closed, the top row keeps its height and gets no wider (the closed panel's LFO
  DEPTH readout already overhangs by 5 pt on `main`, an M8 artefact), the category row,
  field, CLOSE and the entries sit inside the SYNTH panel in more than one column.
- `tests/ui_renders.rs`: `synth_head_names_the_edit_track_and_follows_add_and_select`
  picks "Keys" through the browser and steps back with `<`. `tests/plugins_ui.rs` reads
  the "Pad" preset with `presets::get(4)`.
- Gate: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` (app 183 unit + 1 + 12 plugins + 6 browser + 8 tracks + 74
  renders; engine 185 + 1 ignored and its integration suites; neu 55 + 37), `--smoke-test`
  ok.

## M10 review fixes (app, 2026-10-25)

The app-side items of PROGRESS.md "M10 review follow-ups"; the engine is untouched (its
items wait for the M11 mixer merge).

- Track operations (`app.rs`): `put_strip(channel, Option<Strip>)` stores a strip in the
  mirror and sends its six `SetMixer`s; `remove_track` resets the slot's strip to
  `Strip::for_channel` (the engine keeps a strip across `SetTrackActive`, so a deleted
  soloed track used to keep every other channel silent), `add_track` starts from the
  default strip, `duplicate_track` copies the source's whole strip (level, pan, sends, mute,
  solo: a duplicate is the track as it was, notes aside). `prune_clipboard` drops the
  clipboard's notes on unused slots; `restore` (undo / redo) and `apply_project` call it, so
  a paste after a load or an undo never writes notes to an inactive track.
- The eframe `preset_name` / `preset_index` start-up re-apply is gone with its keys: the
  project carries one preset per track and nothing else read the stored name.
- TRACKS row (`ui/tracks.rs`), left to right: 8 pt padding (the selected row's 3 x 16 pt
  on-glow accent bar sits in it, 2 pt from the edge, on the straight part of the well's
  8 pt-radius inset-m well), chip, name block (52 pt: `"Track 8"` fits, longer names get the
  ellipsis and the tooltip), pencil, 20 pt `M` / `S`, the `Sm` LEVEL knob (34 pt,
  unlabelled, `ChannelParam::Level` through `edit_mixer`, accessible as `level_name` =
  `"Level track <name>"`, a slider), the 32 pt `Roll` pill, 4 pt padding: 168 pt of fixed
  width in the 220 pt column. The `TextKind` / `Align2` import-suppression lines are gone.
- SEQUENCER head (`ui/sequencer.rs`): the chip *and* the name show at 1280 in both views
  (`tag_name` = `"Track: <name>"`, a label): the head gap is 8 (`HEAD_ITEM_GAP`), the A..H
  tabs use `neu::segmented_padded` with `PATTERN_TAB_PADDING` 6 (a kit addition: the
  regular `segmented` on a 6 pt padding), and LENGTH is `(-) 1 bar (+)` on the kit's 28 pt
  `stepper_button`s (accessible as `HALVE_LABEL` / `DOUBLE_LABEL` = `"Halve length"` /
  `"Double length"`), which freed 68 pt; the tag's name gets about 82 pt beside the chip
  (`"SIXTEEN CHARS…"`). `HEAD_RIGHT_GROUP_SEED` (416) stands in for the right group's width
  on the first frame. The row CLEAR pills (73 pt) are the kit's 28 pt `Icon::Clear` circles
  (still "Clear" / "Clear pattern"): the grid rows were 947 pt in a 914 pt slot, which
  pushed the right-hand column 33 pt past the page at 1280 x 1140 (the ROLL view did not,
  so the column jumped between views); the rows are 902 pt now and the page's right edge is
  exactly 1252 (= 1280 - 20 - egui's 8 pt scroll gutter) in every state the tests cover.
- FX rack tags (`controls::track_tag`): the name sits in a fixed `TRACK_TAG_NAME_WIDTH`
  (96) slot (`caps_label_truncated`, ellipsis, full name in the tooltip). The rack's right
  column is pushed to the edge with the width it had *last* frame (`right_aligned`), so a
  head that grew with a longer track name overflowed the rack for one frame, which widened
  the page's scroll content for the next frame and moved everything below by the growth (a
  kittest click on a pill measured on that frame missed it).
- MIXER (`ui/mixer.rs`): the master column is pinned at the panel's right edge outside the
  strips' horizontal `ScrollArea` (which takes the width left of it); the strips scroll
  under the master once wider than that room.
- SYNTH panel: `COL_GAP_MIN` is 4 (was 8): the first row's four columns are 679 pt since
  the design cleanup's 12 pt A D S R gap and need 6.2 pt gaps to sit inside the panel's 698
  pt at 1280; the 8 pt minimum put the LFO column (its DEPTH readout) 5 pt past the panel
  and the FX rack with it. The browser layout test now asserts the top row's edge exactly.
- Tests: `app.rs` (strip reset frees a deleted solo, measured on the master meter through
  the offline engine; ADD / DUP strips; MIDI overdub into track 2; paste carries every
  track's notes; the clipboard after move / remove / load / undo; the export of a two-track
  project equals the live render of the same state sample for sample, see DECISIONS.md for
  why the test keeps the smoothed settings at their defaults); `tests/tracks_panel.rs` (the
  LEVEL knob per row, keyboard edit, row inside the column); `tests/ui_renders.rs` (chip +
  name in both views, first frame included, the seed; MASTER pinned with eight tracks);
  `tests/preset_browser.rs` (top row equality, the LFO readouts inside the panel);
  `neu/tests/controls.rs` (`segmented_padded`).
## M11 as built (engine)

Everything below lives in `crates/engine`; the app and neu crates are untouched by this pass
(the app does not compile against it until adapted). `tests/fixtures/v7_demo.json` (the
pre-M11 `Project::default()` as a v7 file) renders bit for bit as before
(`tests/inserts.rs`, `v7_project_migrates_and_renders_sample_identical`: FNV-1a hash and
per-100 ms RMS captured pre-M11; they equal the v6 reference in `tests/tracks.rs`).

### The model (FL Studio's)

`source -> source strip (level, pan, mute, solo; FL's channel-rack knobs) -> insert bus
(mixer.routes[channel], 1..=16, or 0 = straight to the master sum) -> insert (slot 1 -> slot 2
-> slot 3 -> built-in EQ3 -> fader (level x mute/solo among the inserts, smoothed) -> pan ->
post-fader delay / reverb sends) -> insert.route (0 = master, else a HIGHER insert number)`.
Inserts are processed in ascending number order, so a route to a higher insert lands on a bus
that has not been processed yet: groups and sub-groups work, feedback is impossible by
construction. The shared delay / reverb returns, master EQ, master gain and limiter follow as
before. An insert at its defaults (slots off, flat EQ, 0 dB, centre) is bit-transparent.

### `inserts.rs` (new; re-exported at the crate root)
- `pub const INSERT_COUNT: usize = 16`, `SLOT_COUNT: usize = 3`, `SLOT_PARAMS: usize = 6`,
  `MASTER: u8 = 0`, `MAX_INSERT: u8 = 16`, `DEFAULT_ROUTES: [u8; Channel::COUNT]` (drums
  1..=5, tracks 6..=13).
- Insert numbers are **1-based** in every API (`insert: u8`). `pub const fn insert_index(insert:
  u8) -> usize` wraps: `(insert + 15) % 16`, so `0` and `16` both address the last insert and
  `17` the first (the master has no insert state); `insert_number(index: usize) -> u8` is the
  inverse; `slot_index(slot: u8) -> usize` wraps into `0..3`. An unchecked message index can
  therefore never panic.
- `pub struct SlotParam { pub key: &'static str, pub label: &'static str, pub unit: &'static
  str, pub min: f32, pub max: f32, pub default: f32 }` with `range()`, `clamp(value)`
  (non-finite = default). The tables, in `params` index order:
  - `COMPRESSOR_PARAMS: [SlotParam; 6]`: `threshold` "Thresh" dB -60..=0 (-18), `ratio`
    "Ratio" ":1" 1..=20 (4), `attack` "Attack" ms 0.1..=100 (10), `release` "Release" ms
    10..=1000 (100), `makeup` "Makeup" dB 0..=24 (0), `mix` "Mix" 0..=1 (1).
  - `DRIVE_PARAMS: [SlotParam; 2]`: `amount` "Drive" 0..=1 (0.35), `mix` "Mix" 0..=1 (1).
  - `LOFI_PARAMS: [SlotParam; 4]`: `bits` "Bits" 4..=16 (8), `rate` "Rate" "x" 1..=16 (2),
    `tone` "Tone" Hz 1000..=20000 (10000), `mix` "Mix" 0..=1 (1). (Wobble and vinyl noise
    stay at zero in a slot.)
  - `CHORUS_PARAMS: [SlotParam; 3]`: `rate` "Rate" Hz 0.1..=5 (0.8), `depth` "Depth" 0..=1
    (0.3), `mix` "Mix" 0..=1 (1).
  - `EQ3_PARAMS: [SlotParam; 4]`: `low` "Low" dB -15..=15 (0), `mid` "Mid" dB -15..=15 (0),
    `mid_freq` "Mid Hz" Hz 100..=10000 (1000), `high` "High" dB -15..=15 (0).
- `pub enum SlotKind { Off, Compressor, Drive, Lofi, Chorus, Eq3 }` (serde snake_case,
  `Default` = `Off`): `ALL: [SlotKind; 6]`, `index()`, `from_index(usize)` (clamps),
  `from_value(f32)` (rounds; non-finite = Off), `label()` ("Off", "Comp", "Drive", "Lo-fi",
  "Chorus", "EQ3"), `key()`, `params() -> &'static [SlotParam]` (empty for Off),
  `param_count()`, `default_params() -> [f32; 6]`.
- `pub struct SlotState { pub kind: SlotKind, pub params: [f32; SLOT_PARAMS] }` (serde,
  `#[serde(default)]`, `Copy`, `Default` = `SlotState::OFF`): `new(kind)` (the kind's
  defaults), `set_kind(kind)` (resets the params to the new kind's defaults), `param(index)
  -> Option<&'static SlotParam>`, `get(index) -> f32` (`0.0` for an unused index), `set(index,
  value) -> f32` (clamped; an unused index changes nothing and returns `0.0`), `normalise()`,
  `is_off()`.
- `pub enum InsertParam { Level, Pan, Mute, Solo, DelaySend, ReverbSend, Eq3Low, Eq3Mid,
  Eq3MidFreq, Eq3High, Route }` (serde snake_case): `ALL: [InsertParam; 11]`, `range()`
  (`Level` -60..=6 dB, `Pan` -1..=1, toggles and sends 0..=1, `Eq3*` gains -15..=15 dB,
  `Eq3MidFreq` 100..=10000 Hz, `Route` 0..=16), `default_value(insert: u8)` (sends from
  `default_sends`, mid 1000 Hz, else 0), `is_toggle()`, `key()` (`"level_db"`, `"pan"`,
  `"mute"`, `"solo"`, `"delay_send"`, `"reverb_send"`, `"eq3_low"`, `"eq3_mid"`,
  `"eq3_mid_freq"`, `"eq3_high"`, `"route"`), `label()` ("Level", "Pan", "M", "S", "Delay",
  "Reverb", "Low", "Mid", "Mid Hz", "High", "To"), `clamp(insert, value)` (toggles snap to
  0/1, `Route` rounds and applies `InsertState::valid_route`, non-finite = default).
- `pub fn default_sends(insert: u8) -> (f32, f32)`: inserts 1..=13 are the default homes of
  `Channel::ALL` in order, so inserts 2 and 3 (snare, clap) send `(0.0, DRUM_REVERB_SEND)`,
  6..=13 (the tracks) `(1.0, 1.0)`, everything else `(0.0, 0.0)`.
- `pub struct InsertState { pub level_db: f32, pub pan: f32, pub mute: bool, pub solo: bool,
  pub delay_send: f32, pub reverb_send: f32, pub eq3: Eq3State, pub slots: [SlotState; 3],
  pub route: u8 }` (serde, `#[serde(default)]`, `Copy`, `PartialEq`; `Default` =
  `InsertState::NEUTRAL`: 0 dB, centre, no mute/solo, no sends, flat EQ, slots off, route
  0): `default_for(insert)` (neutral plus `default_sends`), `const fn valid_route(insert,
  route) -> u8` (`route` when `insert < route <= 16`, else `MASTER`), `get(param)`,
  `set(insert, param, value) -> f32` (stored value; a refused route returns `0.0`),
  `set_route(insert, route) -> u8`, `normalise(insert)`, `level_gain()`, `slot(slot) ->
  &SlotState` / `slot_mut(slot)` (wrap), `is_neutral()` (bit-transparent apart from sends and
  route).

### `fx/compressor.rs` (new; `fx::Compressor`, re-exported at the crate root)
- Feed-forward peak compressor, stereo-linked (`max(|l|, |r|)`): peak detector with instant
  attack and exponential release, gain computer in dB with a quadratic `KNEE_DB = 6` soft
  knee, the applied reduction attacks towards the computed one with the attack time constant
  and follows it straight down, then `wet = x * db_to_gain(-reduction) * makeup`, `x + (wet -
  x) * mix`. Bit-transparent under the knee with make-up 0 dB and mix 1.
- `Compressor::new(sample_rate)` (`Copy`), `set_threshold_db` (-60..=0), `set_ratio`
  (1..=20), `set_attack_ms` (0.1..=100), `set_release_ms` (10..=1000), `set_makeup_db`
  (0..=24), `set_mix` (0..=1), the matching getters, `envelope()`, `gain_reduction_db() ->
  f32` (`>= 0`, read after `process` for the meter), `reduction_db(level_db)` (the static
  curve), `reset()`, `process(l, r) -> (l, r)`. Constants `MIN_/MAX_*`, `DEFAULT_*`,
  `KNEE_DB`. A -3 dBFS sine over a -18 dB threshold at 4:1 settles at -14.25 dBFS (+-0.5).

### `fx/eq3.rs` (new; `fx::{Eq3, Eq3State}`, re-exported at the crate root)
- Low shelf at `LOW_SHELF_HZ = 200`, peaking mid (`MID_Q = 0.7`) at the given frequency,
  high shelf at `HIGH_SHELF_HZ = 4000`, reusing the RBJ biquads of `fx::eq` (`Coeffs`,
  `BiquadState`). Gains `-15..=15` dB (`MIN_/MAX_GAIN_DB`), mid `100..=10000` Hz
  (`MIN_/MAX_MID_HZ`, `DEFAULT_MID_HZ = 1000`); `clamp_gain_db`, `clamp_mid_hz`.
- `pub struct Eq3State { pub low_gain_db, pub mid_gain_db, pub mid_freq_hz, pub high_gain_db
  }` (serde `#[serde(default)]`, `Copy`, `Default` = `Eq3State::FLAT`): `new(low, mid,
  mid_hz, high)`, `normalise()`, `is_flat()`, `response_db(freq_hz, sample_rate)` (the sum of
  the three bands, for a UI curve).
- `pub struct Eq3` (`Copy`): `new(sample_rate)`, `state()`, `retarget(&Eq3State)` (values
  glide over `EQ3_SMOOTH_MS` = 10 ms, coefficients recomputed only while moving, click-free),
  `snap(&Eq3State)` (project load; clears the memories), `reset()`, `is_transparent()`,
  `process(l, r)`. A flat state bypasses the biquads entirely (bit-transparent).

### `mixer.rs`
- `MixerState { pub strips: [Strip; 13], pub inserts: [InsertState; 16], pub routes: [u8;
  13] }` (still `Copy`, serde `{ "strips": [..], "inserts": [..], "routes": [..] }`; shorter
  arrays pad with the defaults, longer ones are an error). `inserts[i]` is insert number
  `i + 1`; `routes[Channel::index]` is the source's insert number (`0` = master).
- **Defaults changed**: every source strip is neutral now (`ChannelParam::default_value` is
  `0.0` for everything; the `channel` argument stays for API stability). The sends of a mix
  live on the inserts: `MixerState::default()` has `inserts[i] = InsertState::default_for(i +
  1)` (track inserts 6..=13 sent 1.0 / 1.0, inserts 2 / 3 reverb 0.3) and `routes =
  DEFAULT_ROUTES`. Strip sends still work (post strip, pre insert, into the same buses; the
  M6 tests use them) but the app should expose the insert sends.
- New: `valid_source_route(insert) -> u8` (`<= 16` else 0), `insert(insert) -> &InsertState`
  / `insert_mut(insert)` (wrap), `get_insert(insert, param)`, `set_insert(insert, param,
  value) -> f32`, `set_slot_kind(insert, slot, kind) -> SlotKind` (the kind's defaults),
  `set_slot_param(insert, slot, index: u8, value) -> f32`, `route(channel) -> u8`,
  `set_route(channel, insert) -> u8`, `any_insert_solo()`, `insert_audible(insert)`,
  `insert_effective_gain(insert)`, `insert_pan_gains(insert)`, `migrate_pre_m11(listed_strips:
  usize)`. `normalise()` also clamps the inserts and refuses bad routes; `swap(a, b)` now
  swaps the two channels' routes as well (a track's insert number travels with it on
  `SwapTracks`).
- Solo logic: the source strips solo among themselves (`effective_gain`) and the inserts
  among themselves (`insert_effective_gain`: when any insert is soloed the non-soloed inserts
  are silent); both layers apply, mute beats solo on each.

### `engine.rs`
- New `Message` variants (all `Copy`, `size_of::<Message>() <= 16` unchanged): `SetInsert {
  insert: u8, param: InsertParam, value: f32 }`, `SetSlotKind { insert: u8, slot: u8, kind:
  SlotKind }`, `SetSlotParam { insert: u8, slot: u8, index: u8, value: f32 }`, `SetRoute {
  channel: Channel, insert: u8 }`. `SetMixer` is unchanged (source strips).
- Direct setters (mirror the messages): `set_insert(insert, param, value) -> f32` (level,
  pan, mute, solo, sends glide over `MIXER_SMOOTH_MS`; a solo change re-targets every insert;
  the EQ3 values glide in the EQ; `Route` applies at the next sample and returns `0.0` when
  refused), `set_slot_kind(insert, slot, kind) -> SlotKind` (the effect starts fresh: a hard
  cut, like loading a plugin), `set_slot_param(insert, slot, index, value) -> f32` (`0.0` and
  no change for an unused index; drive amount/mix glide, lo-fi / chorus / EQ3 smooth
  internally, compressor values apply at once), `set_route(channel, insert) -> u8`.
  `set_mixer_state(&MixerState)` now also normalises the inserts / routes, snaps every
  insert's smoothers and restarts every slot effect from the loaded state. `mixer()` exposes
  everything (`mixer().insert(n)`, `mixer().route(ch)`).
- `Meters` gains `pub insert: [f32; 16]` (post-fader, pre-pan block peak of insert number
  `i + 1`) and `pub insert_gr: [f32; 16]` (largest compressor gain reduction in dB among the
  insert's slots over the block, `0` = none), published through the same atomics;
  `of_insert(insert) -> f32`, `gr_of_insert(insert) -> f32` (wrap). `Meters::SILENT` and
  `EngineHandle::meters()` cover them. The per-source `channel` meters stay (post-strip).
- State: `inserts: Box<[InsertDsp; 16]>` (private): per insert three `SlotDsp` (every kind
  pre-allocated: `Compressor`, `Drive` + two smoothers, `[Lofi; 2]`, `[Chorus; 2]`, `Eq3`;
  only the chosen kind runs), the built-in `Eq3` and a `StripDsp` fader stage (the same five
  smoothers as a source strip). `Engine` stays under 256 KiB inline; the slot delay lines
  (two 20 ms lo-fi + two 20 ms chorus lines per slot, 48 slots) are about 0.75 MiB of heap,
  allocated once in `new_engine`. Stereo handling per slot: the compressor and EQ3 are
  stereo; the drive is memoryless (one instance, both channels); lo-fi and chorus keep a left
  and a right instance (identical seeds, so wobble / noise stay centred; the left chorus
  instance's sine tap feeds the left channel and the right instance's cosine tap the right,
  which for a mono input equals the track inserts' single chorus).
- Render per sample: drums / tracks -> `StripDsp::apply` per source (gain, clean, pan,
  strip sends, source meter) -> `insert_in[route - 1]` or the master sum -> for `n` in
  `0..16`: slots -> EQ3 -> fader stage (insert meter, sends) -> `insert_in[route - 1]` when
  `route > n + 1 && route <= 16`, else the master sum (the render re-checks the rule, so a
  bad value can never feed back) -> `FxChain::process_bus(master, delay, reverb)` -> master
  EQ -> tone -> master gain -> limiter. No allocation, locks, I/O, logs or panics; every `u8`
  from a message is wrapped or refused.

### `project.rs` (format version 8)
- `FORMAT_VERSION = 8`. The mixer block gains `inserts` and `routes` (see the module docs'
  JSON). Loading a version-7 (and older) file runs `MixerState::migrate_pre_m11(listed)` with
  `listed` = the number of strips the file wrote: strip `i` **moves** onto insert `i + 1`
  (level, pan, mute, solo, delay / reverb sends), the strip is reset to neutral, the source is
  routed to that insert, the insert's slots are empty, its EQ flat and its route the master,
  so the file sounds exactly as it did and the mix shows up on the inserts; channels the file
  did not write (the padded tracks of a pre-M10 file) get `InsertState::default_for`. The
  "refuse newer" probe is version 9 now. `Project::from_engine` / `apply_to` carry the inserts
  through `mixer`.

### Tests
- `tests/inserts.rs` (9, plus 1 ignored): two sources on one insert sum there (bit for bit)
  and a muted insert silences both; route groups `kick + snare -> 14 -> 16 -> master` and
  `14 -> 15 -> 16` are bit-transparent, the group fader scales the bus, the feedback rule
  refuses lower-or-equal and out-of-range routes (setter and `set_mixer_state`); the
  compressor slot lands a 15 dB-over tone at `threshold + 3.75 dB` (+-1 dB), leaves a quiet
  tone bit-identical and meters about 11.25 dB; the built-in and slot EQ3 shelves cut 98 Hz /
  7.9 kHz by the analytic response (+-0.5 dB) and leave 2 kHz / 523 Hz within 1 dB; slot
  order matters; neutral inserts are bit-transparent for every routing of the demo; the v7
  fixture migrates and renders sample-identical; insert solo / mute / source solo; post-fader
  insert sends; the four messages (with index wrapping) and the published insert meters; and
  `#[ignore] eight_inserts_with_compressors_render_faster_than_real_time` (1 s of eight
  tracks through compressor + EQ3 + chorus slots grouped onto a compressed insert 16 in about
  0.1 s in release; gate `< 0.5 s`).
- `tests/engine_alloc.rs`: the render stays allocation-free with a compressor, a second slot
  of a rotating kind and an EQ3 on every insert, the drums grouped `14 -> 16`, and the four
  insert messages in flight.
- Unit tests in `inserts.rs`, `fx/compressor.rs`, `fx/eq3.rs`, `mixer.rs` (insert setters,
  solo logic, routes, serde padding, the pre-M11 migration), `project.rs` (v6 strips land on
  the inserts). `tests/mixer.rs` pulls the synth's sends on insert 6 now; `tests/tracks.rs`
  expects `"version": 8`.
- Gate: `cargo fmt -p engine`, `cargo clippy -p engine --all-targets -- -D warnings`,
  `cargo test -p engine` (422: 197 unit + 225 integration, up from 401; the 2 ignored release
  perf gates both ok).

## M11 as built (app)

The app side of the FL-style mixer: the MIXER panel's FL layout over the M11 engine, the
insert number boxes in the TRACKS panel and the sequencer rows, LINK / Ctrl+L, and the
app-side insert names and colours. Everything below lives in `crates/app` except the one
kit addition (`neu::fader_sized`). The engine is untouched.

### State (`app.rs`)
- `pub struct InsertUi { pub name: String, pub colour: u8 }` (`Clone`, `PartialEq`, serde):
  `InsertUi::fresh(insert)` = `"Insert N"` in colour `(N - 1) % 8` (the track swatches,
  `Theme::track_colour`); `default_insert_name`, `default_insert_ui() -> [InsertUi; 16]`
  (indexed by `insert_index`, insert 1 first), `clean_insert_name` (trim, at most
  `MAX_INSERT_NAME_LEN` = 12 chars).
- `AppState.insert_ui: [InsertUi; 16]`; view state `insert_selection: Vec<u8>` (insert
  numbers, the primary first; empty = nothing explicitly selected) and `open_slot:
  Option<(u8, u8)>` (the slot whose knobs the strip grid shows). `AppState.mixer` is the
  engine's `MixerState`, so it already carries `inserts` and `routes`.
- Setters, all through the mirror then the message, dirty, undo (discrete for mute / solo /
  route / slot kind / rename / colour, coalesced per drag for the rest, exactly like
  `set_mixer`): `set_insert(n, InsertParam, v) -> f32` and `edit_insert(.., drag_started)`
  (`EditKey::Insert(n, param)`), `toggle_insert_mute(n)` / `toggle_insert_solo(n)`,
  `set_slot_kind(n, slot, kind) -> SlotKind` (the kind already loaded is not an edit; a
  slot switched off closes its knob view), `set_slot_param(n, slot, index, v) -> f32` and
  `edit_slot_param` (`EditKey::Slot(n, slot, index)`; an unused index changes nothing),
  `set_route(channel, n) -> u8` (`MixerState::valid_source_route`; the same route is not an
  edit), `rename_insert(n, &str) -> bool`, `set_insert_colour(n, colour) -> bool`.
- Reads: `insert_ui(n)`, `route_name(n)` (`"MASTER"` for 0 or out of range, else the
  name), `insert_sources(n) -> Vec<Channel>` (the drums and the *active* tracks routed
  there), `insert_is_idle(n)` (no sources, every slot off: drawn dimmed), `free_insert()
  -> Option<u8>` (the first number no active source is routed to; thirteen sources can
  never fill sixteen inserts), `highlighted_insert()` (the primary selection, else the edit
  track's route), `is_insert_selected(n)`.
- Selection and group moves: `select_insert(n, extend)` (plain = alone, `extend` =
  Shift-click toggles membership, `0` clears), `move_insert_fader(n, value, drag_started)
  -> f32` (when `n` is selected with others, every selected insert moves by the same dB
  delta, clamped each, under one coalesced `EditKey::InsertGroup` step; else
  `edit_insert(.., Level, ..)`), `toggle_open_slot(n, slot)` (an empty slot never opens),
  `clamp_open_slot()` (drops the view when its slot is off: after undo, load, a kind
  change).
- Link: `link_to_free_insert(channel) -> Option<u8>` (`set_route` + `select_insert`; a
  status line when nothing is free), `link_track_to_free_insert(slot)` (the edit track for
  `Shortcut::LinkInsert`, Ctrl+L, which also shows the MIXER tab).
- Track operations: `put_strip(channel, None)` also puts the channel's `DEFAULT_ROUTES`
  entry back through `put_route` (a removed track forgets its route, a fresh one starts at
  its home insert); `duplicate_track` copies the source's route (FL's clone keeps the mixer
  track); `move_track` already swaps routes through `MixerState::swap`.
- Messages: `insert_messages(&MixerState) -> Vec<Message>`: per insert one `SetInsert` per
  `InsertParam::ALL` entry, then per slot `SetSlotKind` plus one `SetSlotParam` per
  parameter of that kind, then one `SetRoute` per channel (a default mixer: `16 * 14 + 13`
  = 237). `resend_inserts()` sends them; `resend_all()` calls it after `resend_mixer()` (a
  full resend is about 570 messages now; the outbox absorbs two back to back).
- Meters: `decay_meters` / `meters_active` cover `Meters::insert` and `insert_gr` (the
  gain reduction in dB decays like a level: a 20 dB/s scaling of the bar).
- Undo: `Snapshot.insert_ui`; `restore` puts the names back and clamps the open slot.

### Project file (`project.rs`)
- The engine's `Project` has no field for the names, so the app splices a sidecar: the
  file is `project_text()` = `with_insert_ui(&build_project().to_json_pretty(),
  &insert_ui)`, the engine JSON with a top-level `"insert_ui": [{ "name", "colour" } x 16]`
  key (`INSERT_UI_KEY`); `Project::from_json` ignores unknown keys, so engine-written files
  and the engine's own loader are untouched. `insert_ui_of(text) -> Option<[InsertUi; 16]>`
  reads it back (names cleaned, colours wrapped, missing entries the defaults; `None`
  without the key or when it is not an array). `apply_project_text(&str)` =
  `parse_project` + `apply_project_with_inserts(&project, inserts)`; `apply_project(&Project)`
  is the same with the default names (an engine project carries none). `save_project_to`
  writes `project_text()`, `load_project_from` reads the text and applies it; both keep
  their status / recent / dirty behaviour. A load clears the insert selection and the open
  slot.
- `parse_project(text) -> Result<Box<Project>, ProjectError>` runs `Project::from_json` on
  a helper thread with `ENGINE_BUILD_STACK` (16 MiB): the deserialiser's chain under
  `from_json` needs about 1.6 MiB of stack in a debug build (measured: the `serde_json`
  `Value` to `Project` descent over a 40 KiB bank), the test threads have 2 MiB and the main
  thread 1 MiB, and with the M11 inserts the project tests that load a file overflowed. The
  old `mixer_round_trips_through_the_project_and_v3_files_load_defaults` test is now two
  tests for the same reason (three states, engines and projects in one frame).

### MIXER panel (`ui/mixer.rs`)
Three docks in the 436 pt content width (`CONTENT_WIDTH`), laid out by hand from one
origin in the ~303 pt of body the top row leaves at 1280 x 1140; checked by
`the_docks_fit_the_panel_width` (widths) and `the_strip_blocks_leave_the_fader_a_track`
(heights) plus four `const` assertions:
- **Sources** (`SOURCE_DOCK_WIDTH` = 4 x `SOURCE_STRIP_WIDTH` 36 = 144, horizontal
  `ScrollArea` `mixer_sources`): one strip per `visible_channels` entry: the track's colour
  chip row, the caps label (`strip_label`, truncated, full name in the tooltip), the
  **number box** (`BOX_SIZE` 28 x 16, `box_text`: the insert number or `M`; a click selects
  that insert and opens the picker, the wheel steps the number), LEVEL and PAN `Sm` knobs,
  the 28 pt `M` / `S`. No sends (they live on the inserts) and no meter.
- **Inserts** (`INSERTS_DOCK_WIDTH` = 436 - 144 - 4 - 52 = 236, `ScrollArea`
  `mixer_inserts`; all sixteen strips are laid out, two are in view): `INSERT_STRIP_WIDTH`
  108 = the 3 x 2 knob grid (`GRID_WIDTH` 106) plus a point each side. Top to bottom with
  `BLOCK_GAP` 2: the head row (`ROW_H` 16: accent bar when highlighted, inset-s well when
  selected, colour chip (click cycles), `N · name` caps label truncated (click selects,
  Shift-click extends, double-click renames through `neu::begin_edit` / `show_edit`, id
  `rename_id(n)`, `is_renaming`), pencil); three slot rows (`SLOT_PILL_WIDTH` 88 x 16 pill
  reading `EMPTY_SLOT_LABEL` `- - -` or the kind's label, lit while its knobs are open; a
  click opens an `egui::Popup::menu` of the six kinds; a loaded slot adds the 16 pt
  `SLOT_EDIT` square with a knob glyph that toggles `open_slot`); the grid (`GRID_HEIGHT`
  90: two rows of three `Sm` knobs with 9 pt caps labels under them, `GRID_LABELS` LOW /
  MID / HIGH / FREQ / DLY / REV editing `GRID_PARAMS`, or the open slot's `SlotKind::params()`
  with their `label`s, `slot_fmt` picking the formatter by unit); the ROUTE pill
  (`route_text`: `To MASTER` / `To N`; the picker offers `insert_route_choices(n)` = the
  master and every higher insert only); the fader row (the rest of the height: `M` / `S`
  column, `neu::fader_sized` with its track fitted to the room and no caps label, the
  post-fader `neu::meter` in the fader column's margin, and a `GR_WIDTH` 3 pt
  gain-reduction bar (`gr_meter`, filled from the top over `GR_FULL_DB` 24 dB) when a
  compressor is loaded). A fader change goes through `move_insert_fader`. Idle inserts draw
  their name and chip at `DIMMED_OPACITY`.
- **Master** (`MASTER_WIDTH` 52, pinned right of both scroll areas): the MASTER gain knob
  level with the faders' value row, LIMIT, the stereo meter to the faders' bottom.
- `route_picker(theme, anchor, state, current, choices) -> Option<u8>` (pub; the TRACKS and
  sequencer boxes reuse it): an `egui::Popup::menu` under the anchor with a `Master` pill
  (when offered) and the inserts as rows of `PICK_COLUMNS` 4 `PICK_SQUARE` 28 pt number
  squares (the insert's name is the tooltip), the current one lit. A grid, not a list: the
  17-entry list needed a scroll area whose far entries a test click could not reach.
- Accessible names: sources `"<name> level"` / `"<name> pan"` / `"Mute <name>"` / `"Solo
  <name>"` (unchanged), `source_box_name` `"Insert of Kick"`; inserts `insert_head_name`
  `"Insert 6: Drums"`, `insert_swatch_name` `"Insert 6 swatch"`, `insert_rename_name`
  `"Rename insert 6"`, `slot_name` `"Insert 6 slot 1"`, `slot_edit_name` `"Edit insert 6
  slot 1"`, `kind_pick_name` `"Slot kind Comp"`, `insert_control_name` (`"Insert 6 low"`,
  `"Insert 6 mid"`, `"Insert 6 mid hz"`, `"Insert 6 high"`, `"Insert 6 delay send"`,
  `"Insert 6 reverb send"`, `"Insert 6 level"`, `"Mute insert 6"`, `"Solo insert 6"`),
  `slot_control_name` `"Insert 6 slot 1 thresh"`, `route_name` `"Route of insert 6"`,
  `pick_name` `"Pick MASTER"` / `"Pick insert 7"`, the meters `"Level"` (16) and
  `GR_METER_NAME` `"Gain reduction"`, `"Master"`, `"Limit"`, `"Stereo level"`.

### TRACKS panel and sequencer rows
- `ui/tracks.rs`: the preset line starts with the insert number box (`BOX_SIZE` 22 x 13,
  `insert_box_name` `"Insert box Synth"`, tooltip "Mixer insert: ..."; added after the name
  block so it is on top of it, a click never selects or renames); the preset name is
  truncated to the room left. The move row gains the `Link` button (`LINK_LABEL`,
  `LINK_NAME` `"Link to free insert"`, 36 x 20): `link_track_to_free_insert(edit_track)`
  and the MIXER tab. `HINT` is `"Keys play the selected track"` (Ctrl+T stays in the ADD
  tooltip) so it fits beside the three buttons in two 11 pt lines.
- `ui/sequencer.rs`: every row's name cell ends in a `ROW_BOX_SIZE` 20 x 14 number box
  (`row_box_name` `"Row insert Kick"` / `"Row insert Bass"`; the bass row's channel is the
  edit track through `qol::row_channel`); the solo dot stops short of it.
- Shortcut `LinkInsert` = Ctrl+L (`shortcuts.rs`, checked last; plain L stays a piano key).

### Kit (`neu`)
- `neu::fader_sized(ui, theme, value, spec, track_height, label)` and `FADER_MIN_TRACK`
  (28): the fader on a track of the given height (clamped), optionally without the caps
  label under it; the shared `vertical_slider` body takes the track size and the label
  flag. Kittest in `neu/tests/mixer_widgets.rs`.

### Tests
- `app.rs` `insert_tests` (8): defaults and name cleaning; `set_insert` clamps, stores,
  reaches the engine with the strips' undo granularity; slot kind / params mirror the
  engine and undo restores a slot change (the open view closes with it); routes follow
  the engine rule and travel with duplicate / move / remove / add; link finds the first free
  insert; Shift-selected faders move together in one undo step; rename / colour undo and a
  `resend_all` into a fresh engine; insert meters decay. `project.rs` `insert_file_tests`
  (3): inserts, slots, routes and names through the project text and a file (engine mirror
  after the load, undo), an engine-only file loads the default names, the sidecar is
  cleaned and tolerates bad input. `ui/mixer.rs` (6): names, the width budget, the height
  budget, route choices, slot formatters. `ui/tracks.rs`: the move row budget.
  `shortcuts.rs`: Ctrl+L.
- `tests/mixer_panel.rs` (10, kittest): the number box routes a source and the engine
  mirror follows; a slot pill loads an effect whose knobs replace the grid (count =
  `params().len()`), the edit square toggles back, undo takes it back; a slot knob edit
  reaches the engine; the ROUTE pill offers only the master and higher inserts (and the
  state refuses a lower one); head clicks select, Shift extends, selected faders move
  together; the pencil renames inline and the name survives the project text (Escape
  cancels); meters and the GR bar render in both themes; Ctrl+L links the edit track and
  shows the mixer (Ctrl+Z undoes); the TRACKS row and sequencer row boxes route, and LINK
  works from the footer; the mixer stays inside its slot at 1280 x 1140 with four tracks.
  `tests/ui_renders.rs`: the three mixer tests updated to the docks (sixteen `Level`
  meters, the pinned MASTER, the scrolling source dock); the whole-sequencer tests with the
  mixer shown are unchanged and green. `tests/plugins_ui.rs`: the resend count includes
  `insert_messages`.
- Gate: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` (863 passed, 2 ignored: app 321 = 207 unit + 1 + 10 + 12 + 6 +
  9 + 76, engine 422, neu 120), `cargo run -p app -- --smoke-test` ok.

## M12 as built (neu: the Deck widget kit)

The kit is restyled to `design/round3/WINNER.md` (direction B "Deck"): flat panel tones, 1 px
borders, 4 / 3 / 2 pt radii, one amber accent, eight track colours, 28 pt controls on an 8 pt
rhythm, always-visible scrollbars. Every `pub` name the app used in M11 still exists with its
signature; only what it draws changed. `crates/app` compiles and passes clippy `-D warnings`
unchanged; its geometry tests fail where they measured the old sizes (listed under Tests).

### neu: M12 as built

**`theme.rs`.** `Theme` carries the WINNER tokens with their exact hex values: `bg, panel,
panel_2, panel_3, panel_4, border, border_2, text, text_2, text_3, accent, accent_hover,
accent_press, on_accent, accent_wash` (accent @ 16 % dark / 14 % light, premultiplied), `rec,
ok, clip` (= rec), `grid_beat, grid_bar, step_off, step_off_alt, on_track`, `tracks:
[Color32; 8]`, `shadow: epaint::Shadow { offset: [0, 8], blur: 24, color }` (the one
floating shadow), `knob_face: [top, bottom]`, `key_black_face`, `key_black_border`,
`key_black_text`, `key_black_keycap: [face, border, text]`, the radii `radius_control` 4 /
`radius_small` 3 / `radius_chip` 2, the spacing scale `space_1..space_8` and the rhythm
`control_height` 28, `row_pitch` 32, `header_height` 28, `toolbar_height` 40,
`window_toolbar_height` 36, `tab_strip_height` 28, `gutter` 4, `gutter_hit` 8. The Soft
Console fields stay as aliases so the app compiles: `surface` = panel, `text_primary` = text,
`text_secondary` = text_2, `shadow_highlight` = border, `shadow_dark` = border_2, `glow` =
accent_wash, `key_black` = the black key's top colour, `radius_panel` / `radius_key_*` /
`radius_pill` = 4, `panel_padding` = 8; the `ShadowPair`s are zero (retired in M13).
Helpers: `track_colour(i)` (the new tables), `Theme::note_outline / note_handle /
note_step_border` (60 / 50 / 70 % towards black), `led_on_border`, `led_rec_border`,
`theme::towards_black`, `theme::with_alpha`, `DISABLED_ALPHA` (0.45), `SCROLLBAR_WIDTH` 10 /
`SCROLLBAR_THUMB` 6, `POPUP_RADIUS` 4. `Theme::apply` sets egui's style per WINNER section 7:
panel / window fill = panel, `extreme_bg_color` (fields, scroll tracks) = panel_2, widget
fills per state (inactive panel_3 / hovered panel_4 / active panel_2 / open panel_2, strokes
border_2 or border, `fg_stroke` text_3 / text_2 / text so the scroll thumb is text-3, hover
text-2, drag text), selection = accent_wash with an accent stroke, hyperlink = accent, text
cursor = text, window stroke 1 px border and no window shadow, popup shadow = `shadow`,
popup / menu corner radius 4, item spacing 8 / 8, button padding 10 / 0, interact size
28 x 28, `Theme::scroll_style()` (solid, never hidden, bar 10, thumb 6, handle min 24, every
opacity 1), tooltip delay 0.3, and `Theme::text_styles()` (Small 11, Body 13, Button 12,
Heading 12, Monospace 13 plus `TextStyle::Name` entries `title label button value body
display small hint section micro brand`). Deck's "full-length faint thumb when nothing
scrolls" needs `ScrollBarVisibility::AlwaysVisible` on the area: use `neu::scroll_vertical /
scroll_horizontal / scroll_both(id_salt)`. The 1 px inner-edge border of the scrollbar track
is not reproducible through egui's style and is dropped.

**`text.rs`.** `TextKind` is the Deck scale: `Title` 12/600 caps +0.06em, `Label` 12/500 caps
+0.04em, `Button` 12/600 caps +0.04em, `Value` 13, `Body` 13, `Display` 18/600, `Small` 11/500,
`Hint` 10/600 caps +0.02em, `Section` 11/600 caps +0.06em, `Micro` 10/600, `Brand` = the 13/600
project name (the wordmark is retired). `weight()`, `faux_offset()` (500 paints twice 0.35
apart, 600 0.5 apart), `style_name()`, `text_style()`, `colour(theme)`, `ALL`. Widgets:
`label title value body display small hint section micro brand text_widget`.

**`flat.rs` (new).** `Tone { Bg, Panel, Panel2, Panel3, Panel4 }`, `paint_panel(painter,
rect, tone, radius, theme)` (fill + 1 px `border`), `paint_control(painter, rect, radius,
fill, border)`, `flat_rect_shape`, `paint_focus_ring` / `paint_focus_circle` (2 px accent, 1 px
outside; drawn while a control has focus and is not pointer-pressed), `paint_shadow_floating`
(the only shadow painter), `is_shadow_shape`, `vertical_gradient_rect / _corners /
_circle` (tessellated fans for the knob face, thumbs and keys), `paint_gradient_control`,
`paint_separator` (1 x 20 border_2), `paint_bottom_hairline`. **`shadow.rs`** keeps `Depth`
and every `paint_soft_*` / `soft_*_shapes` name but draws flat: RaisedL = panel + border,
RaisedM / S = panel_3 + border_2, InsetM / S = panel_2 + border_2 (Deck wells are panel-2),
OnGlow = accent + accent_press; `paint_focus_rect / circle` delegate to `flat`. Retired in M13.
**`panel.rs`**: `panel` is a flat frame (panel, 1 px border, radius 4, padding 8);
`panel_toned(ui, theme, Tone, ..)`.

**`button.rs`.** `ButtonKind { Secondary, Primary, Ghost, Toggle(bool), Solo(bool), Rec(bool),
GhostToggle(bool) }`, `ButtonSize { Regular 28, Small 22, Xs 24 }`, `FaceState`,
`paint_button_face(painter, rect, radius, kind, state, theme) -> text colour` (the state table
of WINNER section 4; disabled = `control_painter` at 45 % alpha). `button` = secondary 28,
`primary_button`, `ghost_button`, `small_button`, `toggle_button` = Toggle, `solo_button`,
`rec_button(ui, theme, on)` (28 square, record glyph), `toggle_button_sized` (radius 4;
under 24 tall = small type), `button_kind(ui, theme, label, kind, size)`, `button_wide(..,
width)`, `icon_button(ui, theme, icon, side)` (a square, glyph 16 at 28 / 12 below),
`icon_button_kind(ui, theme, icon, size, kind)`, `header_button` (24 ghost, glyph 12),
`rail_button(ui, theme, icon, on, tooltip) -> Response` (28 ghost-toggle, glyph 14, the
tooltip is its name; the icon's name when empty), `stepper_button` (28 square secondary, 12
glyph), `toolbar_separator`, `switch(ui, theme, &mut bool, name)` (40 x 22, radius 11) and
`switch_small` (34 x 20), `theme_toggle` = the "Dark theme" switch. `Icon` gains `Undo Redo
Menu ChevronDown ChevronUp ChevronLeft ChevronRight Search Link Copy Paste Dots Folder File
Roll Mixer Synth Song Browser Fullscreen Check`; `paint_icon` draws in a 16-unit box, stroke
1.5 round caps, play / stop / rec / dots filled. `PILL_HEIGHT` = 28, `TOGGLE_SIZED_RADIUS` = 4.

**`chrome.rs` (new).** `keycap(ui, theme, "F5")` (16 h, padding 0 4, panel_2, border_2,
radius 3, hint type) + `paint_keycap` / `paint_keycap_coloured` / `keycap_size`; `LedKind { Ok,
Accent, Rec }`, `led(ui, theme, on, kind)` / `led_named(.., name)` (10 circle, 1 px border;
check box role) + `paint_led`; `TagKind { Plain, Vinyl, Drone, Colour(c) }`, `tag(ui, theme,
text, kind)` (16 h, border at 50 % for the coloured kinds); `chip(ui, theme, colour)` 10 x 10;
`paint_colour_bar`; `number_box(ui, theme, &mut u8, RangeInclusive<u8>) -> Response` /
`number_box_named` (22 x 22, radius 3, 11/600; vertical drag 4 px per step, wheel, click to
type; the box is a button named "Insert" whose value is the number, the readout under it is
"Insert value"); `name_button(ui, theme, text, colour: Option<Color32>, selected) ->
Response` (76 x 24, 4 px colour bar, text_3 bar when `None`, ellipsised 12/600, selected =
accent_wash + accent) / `name_button_sized(.., muted, size)`; `ellipsise`;
`pattern_tile(ui, theme, letter, has_notes, current)` (24 x 24, named "Pattern A");
`text_field(ui, theme, &mut String, placeholder, icon: Option<Icon>, width) -> Response`
(28 h, panel_2, border_2, leading 14 icon, placeholder text_3, focus = accent border + 1 px
accent ring; named by its placeholder).

**`shell.rs` (new).** `window_frame(ui, theme, title, right: FnOnce(&mut Ui), body:
FnOnce(&mut Ui) -> R) -> R` fills the available rect; `window_frame_sized(ui, theme, size,
title, sub: Option<&str>, right, body)`: panel fill, 1 px border, radius 4, a 28 h panel_2
header (title in title type at x 8, sub 12 text_2 after it, `right` laid out right-to-left
with 4 padding and an 8 gap) with a border hairline, the body clipped under it; a 28 tall
window is the collapsed header (no hairline, body hidden). `toolbar(ui, theme, |ui| ..)`
(36 h, panel_2, padding 0 8, gap 8, hairline below) and `app_toolbar` (40 h, panel, border,
radius 4, padding 0 6, gap 5). `tab_strip(ui, theme, &mut usize, &[(&str, Option<&str>)]) ->
Response` (28 h panel_2 strip with a hairline, items padding 0 12 in button type, text_2 /
hover text / active text + a 2 px accent underline inset 6, keycap 6 after the text; click
and Left / Right select; `changed()`; items are tabs named by their label) /
`tab_strip_padded(.., padding)`. `hint_bar(ui, theme, &[(key, label)])` (28 h, keycap + 12
text_2 label pairs, clipped; one label named "Space Play / Stop · Ctrl+L Link").
`drawer_handle(ui, theme) -> Response` (a full-width 4 pt border bar with an 8 pt drag hit,
border_2 on hover, accent while dragged; Splitter named "Drawer handle"; read
`drag_delta()`). `list_row(ui, theme, selected, |ui| ..)` / `list_row_named(.., name, ..)`
(28 h, padding 0 8 0 12, hover panel_3, selected accent_wash + 2 px accent bar; ListItem).
`MenuItem { Entry { label, key, enabled }, Separator }` with `item / keyed / disabled`
constructors; `menu(ui, theme, &[MenuItem]) -> Option<usize>` paints the one menu style
inline (min 180 w, panel_2, border_2, radius 4, the floating shadow, padding 4 0, 28 h
items with the keycap right-aligned, hover panel_4, disabled text_3, 9 pt separators) and
returns the index of the entry clicked; `menu_popup(ui, theme, &anchor, items)` floats it
under a clicked anchor through `egui::Popup::menu`; `dropdown(ui, theme, name, value,
&[&str], width, small, colour: Option<Color32>) -> Option<usize>` (28 h / 22 h field with
the chevron and the optional 3 px track bar; ComboBox named `name` whose value is `value`;
opens the menu below). `tooltip_with_keycap(response, theme, text, key) -> Response` (panel_4
face, border_2, 12 text + keycap after 8; the label reads "text (key)"). `scroll_vertical /
scroll_horizontal / scroll_both(id_salt) -> ScrollArea` (always-visible bars, no auto
shrink). `group(ui, theme, title, head_right, body)` (the SYNTH & FX group: 24 h head,
panel, border, padding 6 8 8). `well(ui, theme, size) -> (inner Rect, Response)` (panel_2,
border_2). `paint_playhead`, `paint_hairline_frame`, `popup_id_of`.

**Value controls.** `knob.rs`: `KnobSize { Md 32, Sm 22, Lg 40 }` with `side()`,
`arc_radius()` (13 / 8 / 17), `arc_stroke()` (2.5 / 2), `cap()` (face 18 / 8 / 26),
`pointer_width()` (2 / 1.5), `block_width()` (56, mini 22); the track arc is border_2 with
butt caps, the value arc accent, the face a gradient circle with a 1 px border_2 ring, the
pointer a `text` line from max(r - 8, 3) to r; `paint_knob(painter, theme, centre, size,
spec, v, dimmed)`; `knob_dimmed` = 45 % alpha with a text_2 pointer; `dial` draws the 40 knob
(`DialSize::Lg` text block left of it, `Sm` label above / value below; `rings()` / `dot()`
kept). `slider.rs`: `vslider` 28 x 72 (track 6, 4 wide accent fill, 20 x 12 thumb with a
text_2 line, travel `SLIDER_TRAVEL` 60), `fader` 24 x 120 (`FADER_WIDTH` 24, `FADER_TRACK`
8 x 120, cap 24 x 12 with a text line 5 from its top, unity mark at 75 %, travel
`FADER_TRAVEL` 108; the response is the 24 wide column), `fader_sized` (`FADER_MIN_TRACK`
24), `hslider(ui, theme, value, spec)` 72 x 28 / `hslider_sized(.., size)` (travel 60,
`hslider_fraction_at`), `lamp` = the ok LED with a label / `lamp_kind(.., LedKind)`;
`SLIDER_THUMB` is the 12 pt thumb height, the markers are retired (0). `segmented.rs`: 28 h
panel_2 box with border_2 and radius 4, padding 2, items 24 h; the selected item is a
panel_4 chip (`chip_colour`) with a 2 px accent underline inset 4; `segmented_small` (22 h,
18 items, padding 6, 11 pt). `stepper.rs`: 28 square buttons, value 28 h, `STEPPER_GAP` 6.
`meter.rs`: `METER_TRACK` 5 x 120, `STEREO_METER_SIZE` 12 x 120 (two 5 wide columns, gap 2),
bands ok 0..70 % / accent 70..88 % / clip 88..100 % (`METER_HOT_LEVEL` 0.70, `METER_HOT_TOP`
0.12, `band_colour`), panel ticks every 4 pt; `meter_horizontal(ui, theme, (l, r), size)`
(`METER_HORIZONTAL` 72 x 12), `gr_meter(ui, theme, reduction, height)` (4 wide, fills from the
top, named "Gain reduction"). `step_cell.rs`: `STEP_CELL` 24, `STEP_GAP` 2, `STEP_GROUP_GAP` 6,
`STEP_PITCH` 28 (a row of 16 is 426 wide, `step_x_offset(4)` = 108); `step_cell_at(ui, theme,
spec, name, index, colour: Option<Color32>)` draws the beat alternation (`is_alt_beat`) and
note cells, `step_row_coloured(.., colour)`, `paint_step_cell`; states off / off-alt / hover
(text_2 border) / on (accent, accent_press border) / accent (accent_hover, 2 px inset ring,
2 pt on_accent notch) / playhead (1 px text_2 ring outside) / selected (1 px text ring) /
note (track colour, border 70 % black, 10/700 on_track label); `playhead_dot` is a 6 x 6
accent triangle (`paint_playhead_triangle`) or a 2 pt text_3 dot, `beat_numbers` the 16 h
micro ruler with dots, `playhead_tint` = panel_4. `value_edit.rs`: the inline editor is a
Deck text field (panel_2, accent border + ring, radius 4) and its text input is named
"Value editor". `visualizer.rs`: a panel_2 well with border_2 (`PANEL_RADIUS` 4) and a
grid_beat grid.

**`piano_keys.rs`.** White keys 56 x 64, black 32 x 38 centred on the boundary (`KEY_GAP` 0,
14 keys = 784), radius 0 0 4 4, border_2 over a panel_3 to panel_4 gradient, the note name
(micro text_3) above the keycap (16 h) at the bottom (gap 2, padding 4); black keys use the
theme's black-key colours; held = accent / accent_press / on_accent with a transparent
keycap. `GENERIC_WHITE_KEY` 40 x 64 / `GENERIC_BLACK_KEY` 24 x 38.

**`piano_roll.rs`.** The roll is ruler 16 (`RULER_HEIGHT`, panel_2, "bar.beat" micro labels,
playhead triangle) + grid + velocity lane 44 (`VELOCITY_LANE_HEIGHT`, panel_2, hairline
above, "VEL", 3 wide bars by velocity, a 7 x 3 text marker at velocity >= 0.85, selected =
text ring): `PianoRollModel::size()` includes both, `grid_rect(outer)` subtracts them,
`ruler_rect`, `velocity_rect`, `key_column_rect`, `grid_height()`. `KEY_COLUMN_WIDTH` 40, row
height 13 by default, `NOTE_RADIUS` 3, bar 12 (`NOTE_INSET_Y` 0.5), `PLAYHEAD_WIDTH` 1, a
2 pt handle inset 2. Key column: panel_2, black-key rows panel_4 on the left 60 %, C labels
micro text_2, hairlines between rows; grid: step lines grid_beat, beat / bar lines grid_bar,
row dividers grid_beat (B-C and E-F grid_bar), black-key rows `black_row_tint` (panel 60 %
towards panel_2). Notes: `note_fill(colour, velocity)` (alpha 0.5 + 0.5 v), outline 60 %
black, selected 1 px text ring; ghosts 35 % / 60 %. `piano_roll` / `piano_roll_with_ghosts`
draw the notes in the accent; `piano_roll_coloured(ui, theme, model, notes, ghosts, colour)`
in the track colour. A press / drag in the velocity lane sets the velocity of the note
starting in that column (`moved`); `velocity_bar_rect`, `velocity_at`. The accent dot and
white-key lift constants are retired (0).

**Gallery.** `cargo run -p neu --example gallery [-- --dark|--light|--both] [--exit-after-secs
N] [--scroll PX]`: the component sheet of WINNER section 8 (buttons, segmented / dropdown /
switch, text field / LEDs / tags / keycaps / name buttons / pattern tiles, knobs, fader /
meters / sliders, step cells / note bars / ghosts with a live roll, tabs / list rows /
tooltip / hint bar, the menu, scrollbars in a window header + the drawer handle, swatches,
the type scale, keyboard keys / a mixer strip / browser chips with a scope well), dark beside
light by default.

### Tests
- `neu`: 146 = 64 unit tests (theme hex values, every AA ratio WINNER lists at a 0.05
  rounding tolerance, the derived formulas, `Theme::apply`'s style, the flat shapes carrying
  no blur, the gradient meshes, knob / slider / meter / step / roll geometry) and 82 kittests
  across `controls` 12, `deck` 16 (new: every button kind, state and size; switches; the
  window frame, toolbars, tabs and hint bar; rail buttons / LEDs / tags / chips / keycaps /
  tiles; the number box drag / wheel / typing; name buttons; the drawer handle and list rows;
  the menu; the dropdown popup; the tooltip with keycap; the text field; sliders / meters /
  group; the coloured roll's velocity lane; "no shadows except floating" over a sheet of
  every widget and the menu's single shadow; text on every surface), `mixer_widgets` 7,
  `piano` 6, `piano_roll` 14, `step_cell` 5, `tooltips` 9, `value_edit` 4, `visualizer` 5,
  `widgets` 4. Every old assertion was re-pointed at the Deck geometry (fader 24 x 120, cells
  24 with 426 rows, keys 56 / 32, roll 40 + ruler + lane, segmented chip + underline, popup
  shadow 0 8 24, radius 4, the project-name `Brand` level).
- Design finding: light `on_accent` (white) on `accent_hover` `#C9640F` is 3.95:1 (AA UI,
  not AA text); the WINNER table lists on-accent against accent only. The test checks 3:1
  there and 5:1 on accent.
- `crates/app` compiles and passes `cargo clippy -p app --all-targets -- -D warnings`
  unchanged. `cargo test -p app --no-fail-fast` before the M12 layout rewrite: 26 failures,
  all geometry: unit tests `ui::mixer::tests::the_docks_fit_the_panel_width`,
  `ui::sequencer::tests::cell_positions_follow_the_mock_maths`,
  `ui::tracks::tests::a_row_fits_the_column` (3 of 207); `tests/mixer_panel.rs` 1 of 10
  (`the_mixer_stays_inside_its_slot_at_the_design_size`); `tests/preset_browser.rs` 1 of 6
  (`browser_keeps_the_whole_sequencer_visible_at_the_design_size`); `tests/tracks_panel.rs`
  2 of 9 (`whole_sequencer_is_visible_with_the_tracks_tab_shown`,
  `tracks_panel_level_knob_edits_the_strip_and_the_row_fits`); `tests/ui_renders.rs` 19 of
  76 (the roll tests, since the roll grew a 16 ruler and a 44 velocity lane and its key
  column is 40; the whole-sequencer / mixer / visualizer fit tests, since cells are 24 and
  faders 24 wide). `tests/plugins_ui.rs` and `main.rs` stay green.
