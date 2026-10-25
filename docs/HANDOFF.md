# Orchestrator handoff (written 2026-10-25)

The previous orchestrator session's context grew too large. This file is the complete state
for the next orchestrator. Read it first, then docs/PLAN.md (rules, test gate, loop procedure,
Phase 2 and Phase 3 briefs), docs/ARCHITECTURE.md (interfaces, M0-M8), PROGRESS.md,
DECISIONS.md, BLOCKERS.md, and `git log --oneline -15`.

## Where main is
- Tags m0..m7 are done (keyboard/MIDI synth, FX, drums, 16-step sequencer with 8 patterns and
  song mode, mixer with meters, piano roll, undo/redo, shortcuts, projects, WAV export).
- Phase 3 is in progress (docs/PLAN.md "Phase 3 brief"): **M8 synthesis extensions**, then
  **M9 EDM + Joji-style lofi preset library with a browser**.
- M8 status on main:
  - DONE and committed: the 33 new `ParamId`s (`crates/engine/src/params.rs`, COUNT = 60);
    FX modules `fx/lofi.rs`, `fx/chorus.rs`, `fx/ducker.rs`, `fx/mod_delay.rs` with
    `FxChain::process_synth_insert`, `set_ducker`, `ducker_trigger` (tests/fx_m8.rs); the app's
    synth-panel and FX-rack groups for every new parameter (crates/app/src/ui).
  - IN FLIGHT when this file was written: the **synth DSP** (pulse width, FM, noise, pluck
    string, filter types, filter ADSR, key tracking, velocity, pitch env, unison, sub) in
    `crates/engine/src/synth/{oscillator,fm,noise,pluck,svf,mod}.rs`, `voice.rs` and
    `tests/synth_m8.rs`. The old session commits and pushes it when its subagent finishes.
    **Pull first.** If `git status` still shows those files uncommitted and `cargo test -p
    engine` is red or the files stop changing for 10+ minutes, finish that work yourself per
    the M8 synth bullet in docs/ARCHITECTURE.md ("M8 additions / engine: DSP").
  - NOT DONE (orchestrator integration, small): in `crates/engine/src/engine.rs` replace the
    synth-channel insert `self.fx.drive().process(self.voices.tick())` with
    `self.fx.process_synth_insert(self.voices.tick())` and make the synth channel stereo from
    there (pan/level/sends act on the pair); route `ParamId::SidechainAmount/SidechainRelease`
    in `Engine::set_param` to `self.fx.set_ducker(amount, release)`; call
    `self.fx.ducker_trigger()` where the kick fires in `fire_step`; bump
    `project::FORMAT_VERSION` to 6 (older files load with defaults; update the version
    literals in tests/project.rs, tests/mixer.rs, tests/notes.rs and the "refuse newer"
    probe). Add two engine tests: sidechain pumps the synth on a kick step; lo-fi mix 1 with
    4 bits changes the synth output. Then the full gate, PROGRESS.md ticks, commit, tag m8, push.
- Then M9 per docs/PLAN.md and docs/ARCHITECTURE.md ("M9 additions"): a presets subagent
  (`crates/engine/src/presets.rs`: `Category { LofiKeys, LofiGuitars, LofiPads, EdmLeads,
  EdmBass, EdmPlucks, EdmPads, Risers }`, 80+ presets with tags, search helpers, per-preset
  tests) and an app subagent (`ui/preset_browser.rs`). Gate, tag m9, update the PROGRESS.md
  final summary, stop.

## Side sessions and their branches (merge them into main as they finish)
The user asked: "as other sessions finish please merge in their worktrees so we can test all
feats". Each runs in its own git worktree under `.claude/worktrees/` on a local branch:
- `madi/test-feedback-deb72b`: the user's live change-request session (right-click clears a
  step; piano-roll key column, held keys lit, resizable, chord palette). **Already merged**
  into main at 7a4b29b. The user keeps sending it changes, so re-merge it periodically
  (`git merge --no-ff madi/test-feedback-deb72b`).
- `madi/plugins-eq-reverb-1bffdb`: pop-out EQ (7-band parametric, spectrum analyser, presets)
  and Reverb (tuning, impulse-decay visualisation, presets) windows via egui viewports;
  bumps FORMAT_VERSION and adds `EqState`/`ReverbState` + messages.
- `madi/visualizers-bc3bf5`: audio visualizers (scope, bars, radial, vector, waves, particles)
  with presets; adds `engine/src/scope.rs` (audio tap + hit bits) and `neu/src/visualizer.rs`.
- `madi/qol-polish-6ccb28`: typed value entry on every readout, tap tempo, BPM nudge,
  drag-paint steps, Ctrl-click accent, row mute/solo, recent projects, clean-ups.
Merge procedure per branch: `git merge --no-ff <branch>`; resolve conflicts (likely spots:
`engine.rs` render path, `project.rs` FORMAT_VERSION (take the highest, keep every
`#[serde(default)]` field), `app.rs` state fields, `ui/mod.rs` layout, `lib.rs` module lists,
`Cargo.toml` deps); run the full gate; commit; push. Never force-push. If a branch is not done
yet, leave it; its session reports to the old orchestrator session, so check the branch log
(`git log main..<branch>`) and the session's row in the sidebar for whether it finished.

## Facts the docs do not spell out
- egui/eframe 0.37: `App::logic` + `App::ui` instead of `update`; panels are `egui::Panel`;
  `Id::new`/`Ui::id` are deprecated (use `ui.scope_id()`); `Context::wants_keyboard_input` does
  not exist (use `ctx.text_edit_focused()`); tooltips keep repainting while hovered, so kittest
  harnesses that hover a control and call `run()` need `.with_max_steps(32)` or `run_steps`.
- The app sends UI->engine messages through `AppState::send`, which spills into an outbox
  drained each frame, so bursts (bank resend, note edits) never drop; tests use the file's
  `sync()` helper.
- `send_pattern`/`pattern_messages` must send only a pattern's `length` steps and replay notes
  with `ClearNotes` + `SetNote` per note (the lane is a derived view).
- Window: the app re-requests its size on the first frame if the persisted size is >10 % off
  (`window.rs`); default 1280x1140, min 1270x760; two kittest tests assert the whole sequencer
  is visible at 1280x1140 (grid and mixer variants) and must stay green.
- The build machine has no MIDI device and nobody can listen: sound design is verified by
  DSP measurements in tests (brightness = first-difference RMS ratio, pitch by zero crossings
  or autocorrelation, Goertzel for single bins).
- Side sessions needed a one-time "Trust repository" click in Claude Desktop for their
  worktree folders.
- The user's taste: FL Studio workflow, EDM, Joji-style lofi.

## Loop command for the new session
Paste this to run unattended:

    /loop Continue the BeatSlopper build: read docs/HANDOFF.md first, then follow docs/PLAN.md "Loop procedure" and the "Phase 3 brief", using PROGRESS.md, DECISIONS.md and BLOCKERS.md as state; merge the side-session branches listed in HANDOFF.md into main as they finish. Stop the loop when M9 is done and green and every finished branch is merged, or everything is blocked.
