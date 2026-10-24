# BeatSlopper

A desktop beat-making app written in Rust. Scope ends at "first beat": play a synth from the
computer keyboard, shape it with knobs and effects, program drums and a bass/melody line in a
16-step sequencer, save the project, and export the loop to WAV.

**Visual style:** neumorphism (soft UI) with native light/dark mode. Dark theme is near-black
with high-contrast orange; light theme is off-white with high-contrast blue.

## Status

Phase 0: design previews. See `design/index.html` for the three candidate directions and
`PROGRESS.md` for the current state of the build.

## Planned architecture

Cargo workspace with three crates:

- `crates/engine` - pure DSP and sequencing, no UI or audio-device dependencies, renders offline.
- `crates/neu` - neumorphic egui widget kit and theme tokens.
- `crates/app` - eframe app, cpal audio output, keyboard input.

See `docs/PLAN.md` for the full build brief.

## License

Proprietary. See `LICENSE`.
