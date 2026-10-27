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

    /loop Continue the BeatSlopper build: read docs/HANDOFF.md first (its takeover notes), then follow docs/PLAN.md "Loop procedure" with the "Phase 5 brief" (M12 -> M13 -> M14, design/round3/WINNER.md is the source of truth), using PROGRESS.md, DECISIONS.md and BLOCKERS.md as state. Stop the loop when M14 is done and green, or everything is blocked.

## Added after the handoff was written
- `madi/design-cleanup-3a9db8` (session "Design cleanup: selection states and polish"): the
  segmented control's selected item becomes an accent chip, plus a panel-by-panel alignment,
  contrast and labelling sweep; touches crates/neu widgets and small crates/app ui fixes. Merge
  it like the others when its session reports done.
- Main now also carries the M8 synth DSP commit (77dd850); M8 still needs only the engine
  wiring listed above, then the gate and tag m8.

## Taken over 2026-10-25 (second orchestrator)
- Every branch above (test-feedback, plugins-eq-reverb, visualizers, qol-polish, design-cleanup,
  reverb-audit) had no commits left outside main at takeover; only `madi/test-feedback-deb72b`
  may still grow (the user's live session): check `git log main..madi/test-feedback-deb72b`.
- `project::FORMAT_VERSION` is already 6 (the plugins branch bumped it); M8 adds no bump.
- New Phase 4 brief (docs/PLAN.md): M10 instrument tracks (layer synths into rolls, Tracks list,
  ghost notes, selected track gets the keys), M11 FL-style mixer (inserts, slots, routing).
  Order: M8 wiring -> M10 -> M9 -> M11. Interfaces in docs/ARCHITECTURE.md, reasons and sources
  in DECISIONS.md.

## Takeover notes for the next orchestrator (written 2026-10-25 ~19:50, user low on usage)
State of main (pushed): m8, m9, m10, m11 tagged and green. Phase 3 + Phase 4 are DONE: tracks
(layering), preset library + browser, FL-style mixer with inserts / slots / routing. The user
then dropped the neumorphic design: **Phase 5 = UI overhaul** (docs/PLAN.md "Phase 5 brief").
- The user picked design **B "Deck"** (FL-Studio-like) from `design/round3/`; the token sheet
  and layout budgets are `design/round3/WINNER.md` (the source of truth; the old
  `design/WINNER.md` is history). Milestones M12 (neu restyle + shell + PATTERN view), M13
  (piano roll, mixer drawer, synth & fx windows), M14 (song, browser, plugins, keyboard,
  polish, captures, tag). Stop the loop when M14 is done and green.
- **M12 neu half is committed** (3dd6203: `crates/neu` fully restyled, 146 tests; see
  docs/ARCHITECTURE.md "M12 as built (neu)"). **26 app kittests fail on the old geometry**
  until the shell lands; that is expected and listed in that section.
- **M12 app half (the shell rewrite) was IN FLIGHT** when this note was written: a subagent was
  rewriting `crates/app/src/ui/mod.rs` (+ new `ui/shell.rs`), `window.rs`, `app.rs` (a `View`
  enum replacing right_tab / right_panel / lane_view) and the app tests, per the brief in
  docs/PLAN.md Phase 5 step 2 and WINNER.md section 5. If `git status` shows uncommitted app
  changes: run `cargo build -p app` and `cargo test -p app`; if green, review and commit; if
  mid-way, finish the shell yourself from WINNER.md (toolbar, browser column, channel rack,
  main window with F5..F9 views, mixer bar, keyboard strip, hint bar, work-area window sizing,
  tests at 840x600 / 1280x800 / 1920x1080). Then the full gate, tag m12, push, then M13.
- Gate pitfalls: the running app locks `target/release/beatslopper.exe`, so the release build
  and smoke test run from a detached worktree with its own target dir (see the orchestrator
  memory "beatslopper-gate-release-lock"); the user works at this PC, so never launch the
  app window while they ask for quiet (memory "no-app-launch-while-user-works"); PrintWindow
  captures need `SetProcessDPIAware` and the window handle from the process (the title now
  carries BPM + pattern). The display is 3840x2160 at 150 %: the old 1280-pt page did not fit
  in a 1280-px window, which is what started the redesign (capture in design/round3/captures).
- Side branches: every one listed above is merged; `madi/test-feedback-deb72b` had no new
  commits all day.
- Open follow-ups (small): the engine's `cargo doc -D warnings` has four stale intra-doc links
  (fx/ducker.rs:12, project.rs:75, engine.rs ~1293, pattern.rs:13); export vs live render
  differ for the first ~50 ms because live smoothers glide (a "snap" message would fix it);
  the 8 classic presets are still outside `presets::LIBRARY`.
- **Update 20:30:** the M12 shell LANDED and is tagged **m12** (2f0d6ac, 900 tests, pushed).
  Next is M13; start with the two capture findings listed under PROGRESS.md "Phase 5"
  (toolbar overflow + hidden main window at 853 logical points; TRACKS header label overlap).
- **Scope update 2026-10-27:** the project is now an open-source audio toy aiming at full-DAW
  features (docs/PLAN.md "Scope update" at the top, DECISIONS.md 2026-10-27). The old "first
  beat" out-of-scope list no longer binds, but the loop target is unchanged: M13, then M14.
  License is now `MIT OR Apache-2.0`.
