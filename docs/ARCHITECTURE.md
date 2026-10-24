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
