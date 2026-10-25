# Project BeatSlopper: Autonomous Build Brief (Rust, Desktop)

> **Before you send this (do these yourself, once):**
> 0. **Windows Smart App Control must be Off** (Windows Security → App & browser control → Smart App Control). Otherwise it blocks the Rust installer and every unsigned program Cargo compiles, including the app itself.
> 1. Install Rust: https://rustup.rs (default MSVC toolchain).
> 2. Install **Visual Studio 2022 Build Tools** with the "Desktop development with C++" workload (Rust needs its linker).
> 3. Install **git** and **GitHub CLI** (`gh`), then run `gh auth login` yourself. The agent must never handle tokens.
> 4. Open a Claude Code session in an **empty folder** where the project should live (e.g. `Documents\GitHub\BeatSlopper`).
> 5. Set a permission mode that lets it run `cargo`, `git`, and `gh` without asking (auto mode or an allowlist), and turn **keep-awake** on so the PC doesn't sleep mid-loop.
> 6. Replace `Madison` below with your name.
>
> **Message 1:** paste everything below the line.
> **Message 2 (after you pick a design):** see "Kickoff after design pick" at the bottom.

---

## Role

You are the **lead engineer and orchestrator** for **BeatSlopper**, a desktop beat-making app written in **Rust**. You plan, delegate to subagents (Agent tool), integrate their work, run the test gate, fix failures, commit, and keep going without checking in with me. I am not going to monitor you. Only stop when this brief says to stop.

## Product goal (scope ends at "first beat")

I want to make my first beat:
- Play a synth with my **computer keyboard as a piano**.
- Shape the sound with **synth knobs** and **effects**.
- Program **drums and a bass/melody line** in a looping **step sequencer** at a set BPM.
- **Save** the project and **export the loop to WAV**.

**Out of scope, do NOT start:** piano roll, arrangement timeline, audio/mic recording, mixer view, VST hosting, MIDI hardware (optional stretch only if everything else is done and green). When M3 is done, **stop**.

## Visual style (hard requirement)

**Neumorphism** (soft UI): controls look extruded from or pressed into the background, using two soft shadows per element (a light highlight to the top-left and a dark shadow to the bottom-right). Pressed or active controls look **inset**.

- **Native light & dark mode:** on launch, follow the OS theme (eframe/egui system theme), plus a manual toggle in the UI that is remembered between runs.
- **Dark theme: blacks with high-contrast orange.**
  - Base surface: near-black, e.g. `#16171A` (NOT pure `#000`, because shadows vanish on pure black).
  - Highlight shadow: approximately `#24262B`. Dark shadow: approximately `#0A0B0D`.
  - Accent: high-contrast orange, e.g. `#FF7A1A` (hover/active `#FF9440`).
  - Text: `#F2F2F2` primary, `#A8A8A8` secondary.
- **Light theme: whites with high-contrast blue.**
  - Base surface: off-white, e.g. `#ECF0F5` (NOT pure `#FFF`, for the same reason).
  - Highlight shadow: `#FFFFFF`. Dark shadow: approximately `#C5CCD6`.
  - Accent: strong blue, e.g. `#1E5BFF` (hover/active `#3D74FF`).
  - Text: `#141821` primary, `#5A6372` secondary.
- **Accessibility:** neumorphism is notorious for low contrast. All text and every active/on state (lit steps, pressed keys, toggles) must meet **WCAG AA contrast** using the accent color. Don't rely on shadow alone to show state.
- These hex values are a starting point. The design phase may tune them, but the winning design's values become the source of truth.

## Phase 0: Repo + 3 design previews (NO Rust code yet)

Do these steps in order:

1. `git init`. Create `README.md`, a `.gitignore` for Rust (`/target`, etc.), and `LICENSE`.
   - **LICENSE:** proprietary, "Copyright (c) 2026 Madison. All Rights Reserved." State that no permission is granted to use, copy, modify, or distribute without written permission.
2. Create the private GitHub repo and push: `gh repo create BeatSlopper --private --source . --remote origin --push`. If `gh` isn't authenticated, write that to `BLOCKERS.md`, tell me, and stop. Do not try to log in yourself.
3. Create the state files (see "State files"): `PROGRESS.md`, `DECISIONS.md`, `BLOCKERS.md`, `docs/PLAN.md` (a copy of this brief).
4. **Design previews:** spawn **3 subagents in parallel**. Each one writes one file, so there are no conflicts:
   - `design/preview-a.html`, `design/preview-b.html`, `design/preview-c.html`
   - Each is a **distinct design direction** within neumorphism. For example: A "Studio hardware" (chunky knobs, big pads, drum-machine feel), B "Minimal flat-soft" (airy, thin type, subtle depth), C "Synth module" (dense, rack-style panels, LED-style step lights). Give each one a name.
   - Each file **mocks the real M3 main screen**: top transport bar (play/stop, BPM, swing, master volume, theme toggle), synth panel (oscillator wave, filter cutoff/resonance, ADSR, LFO knobs), FX rack (drive, delay, reverb with mix knobs), a **16-step sequencer** (drum rows: kick, snare, clap, closed hat, open hat; plus one bass/melody lane with per-step note), and an **on-screen piano keyboard** (about 2 octaves, labelled with computer keys).
   - **Light AND dark** in the same file: default to `prefers-color-scheme`, with a visible toggle.
   - **Light interactivity:** the toggle works, step cells toggle on click, knobs rotate on drag, and on-screen keys light up when the matching computer keys are pressed. No audio needed.
   - **Self-contained:** one HTML file each, inline CSS/JS, no CDN, no web fonts, no images.
   - **egui-buildable only:** use only effects we can reproduce in Rust/egui's painter: rounded rectangles, up to two offset soft box-shadows per element (outer for raised, simulated inset for pressed), solid fills, simple linear gradients, circles/arcs for knobs. **Forbidden:** `backdrop-filter`, `filter: blur()` on content, glassmorphism, CSS 3D transforms, animations that egui can't do cheaply. Put a comment at the top of each file listing its exact color and shadow tokens.
5. Create `design/index.html` that links to all three side by side, with each name and a one-line pitch.
6. Commit and push ("Phase 0: design previews").
7. **STOP.** End your turn with a short message: where the files are, the three names, and "Reply with the winner (A, B, or C) and any tweaks." **Do not write any Rust, do not run `cargo new`, do not start a loop.**

## Kickoff after design pick (what happens when I reply)

When I reply with a winner:
1. Write `design/WINNER.md` with the letter, name, any tweaks I requested, and the final token table for both themes (colors, corner radii, shadow offset/blur/spread, spacing, font sizes).
2. Commit and push.
3. The build then runs in a loop (I will start it with `/loop`, see the end of this file). Every iteration follows the "Loop procedure" below.

## Architecture (must follow)

A Cargo **workspace** split so that subagents can work in parallel on separate crates:

```
BeatSlopper/
  Cargo.toml                 # [workspace]
  crates/
    engine/                  # NO UI or audio-device deps. Pure DSP + sequencing.
      src/ lib.rs, engine.rs, transport.rs, voice.rs, synth/, drums/, fx/, params.rs, project.rs
      tests/                 # offline-render tests
    neu/                     # Neumorphic egui widget kit + theme tokens (from design/WINNER.md)
      src/ lib.rs, theme.rs, shadow.rs, knob.rs, button.rs, step_cell.rs, slider.rs, panel.rs, piano_keys.rs
    app/                     # eframe app + cpal audio output + keyboard input
      src/ main.rs, audio_io.rs, input.rs, ui/ (transport_bar, synth_panel, fx_rack, sequencer, keyboard)
  design/  docs/  PROGRESS.md  DECISIONS.md  BLOCKERS.md
```

**Crates:**
- `eframe`/`egui` for the UI.
- `cpal` for audio output.
- `rtrb` (or `crossbeam-channel`) for lock-free UI-to-audio messages.
- `serde` + `serde_json` for project files.
- `hound` for WAV export.
- Optional: `fundsp`, but hand-written DSP is fine and often simpler.
- `egui_kittest` (dev) for UI tests.
- Every dependency must be MIT/Apache/BSD/Zlib or similarly permissive. **No GPL/LGPL/AGPL.** Enforce this with `cargo-deny` (`deny.toml` license allowlist).

**Real-time rules (non-negotiable):** the audio callback **never** allocates, locks a mutex, does I/O, logs, or panics. It talks to the UI only through lock-free queues and atomics. Pre-allocate voices and buffers. Smooth parameter changes to avoid zipper noise. Flush denormals or add tiny DC offsets.

**Testability rule:** the `engine` crate must be able to **render offline** into a buffer with no audio device (`Engine::render(&mut [f32], frames)`). Everything audible gets tested this way.

## Milestones and definition of done

### M0: Skeleton
- Workspace builds. The app opens a window in the winning theme, with OS light/dark detection and a toggle.
- cpal output stream plays a test tone from the engine via a button. The engine also renders offline.
- `neu` crate: theme tokens from `WINNER.md` plus the raised/inset dual-shadow painter, panel and button.
- `--smoke-test` CLI flag: launch, render about 60 frames, exit code 0.

### M1: Keyboard piano
- Computer keys map to notes: `A W S E D F T G Y H U J K O L P ;` covers about 1.5 octaves. `Z`/`X` shift the octave down/up. Velocity is fixed (adjustable in UI).
- Ignore OS key-repeat: track held keys so holding a key doesn't retrigger.
- Polyphonic (8–16 voices) with voice stealing and an ADSR envelope.
- On-screen piano keys light up with the accent color while held, and can be clicked with the mouse.

### M2: Synth + effects
- **Synth:**
  - 2 oscillators (sine/saw/square/triangle, band-limited, e.g. PolyBLEP), detune, mix.
  - Lowpass SVF filter with cutoff and resonance, and a filter envelope amount.
  - Amp ADSR.
  - LFO (rate, depth, target: pitch or cutoff).
  - Glide (optional).
- **FX chain:** drive/distortion → delay (time synced to BPM, feedback, mix) → reverb (Freeverb-style, size, damping, mix) → master gain with a soft limiter.
- All parameters are neumorphic knobs, with double-click to reset.
- 6–10 built-in presets (bass, lead, pad, pluck…), switchable from a dropdown.

### M3: First beat
- Transport: play/stop, BPM (60–200), swing, loop of 16 steps (1 bar of 16th notes). **Sample-accurate** scheduling inside the audio thread.
- **Drum kit synthesized procedurally** (no sample files, no licensing issues): kick (sine pitch sweep + click), snare (tone + filtered noise), clap (noise bursts), closed hat and open hat (high-passed noise/metallic), with per-row volume and tune.
- **Step sequencer grid:** 5 drum rows plus **1 synth lane** where each step has a note. Two ways to enter notes:
  - Click a step and press a piano key.
  - **Record-arm overdub:** while the loop plays, keys you play get quantized into the synth lane.
- Steps support accent/velocity. Clear-row and clear-all buttons.
- **Save/Load** project as `.beatslopper.json`: BPM, swing, pattern, synth params, FX, preset.
- **Export loop to WAV** (choose 1/2/4/8 repeats) using the offline renderer.
- **Done =** I can open the app, pick a sound, program drums and a bassline, tweak knobs, hear it loop, save it, and export a WAV.

## Test gate (run after every phase and every integration)

The **orchestrator alone** runs the gate, serially, so that parallel subagents don't fight over the Cargo build lock:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. `cargo deny check licenses`
5. `cargo build --release`
6. `cargo run --release -p app -- --smoke-test`

**Required tests (write them as the features land):**
- **Engine (offline render):**
  - The output is not silent when a note is on, and goes silent after release + tail.
  - Pitch is correct within about 1% (zero-crossing or autocorrelation estimate) for A4 = 440 Hz.
  - No NaN/inf values, and peak stays ≤ 1.0 after the limiter.
  - Voice stealing works at max polyphony.
  - At 120 BPM / 48 kHz, step `n` triggers at exactly `n × 6000` samples; swing offsets odd steps correctly.
  - Delay time matches BPM sync.
  - Project JSON round-trips losslessly.
  - Exported WAV has the expected length and is non-silent.
- **UI (`egui_kittest`):** the theme toggle switches palettes; clicking a step cell toggles it; pressing mapped keys sets pressed state on the on-screen keyboard; the app renders in both themes without panicking.
- **Input:** a key-repeat event doesn't retrigger a note.

## Auto-fix policy

When the gate fails:
1. Read the error, fix the root cause, and re-run the gate. Never fix a test by deleting it or weakening its assertions unless the test itself is wrong; if so, record why in `DECISIONS.md`.
2. Allow up to **3 fix attempts** per distinct failure. After that, try a **different approach** (e.g. a different crate or algorithm), with up to 2 alternate approaches.
3. If still failing: log it in `BLOCKERS.md` (what you tried, the exact error), skip to the next independent task if one exists, and only stop the loop if nothing else can progress.

## Subagent strategy

- You (the orchestrator) own planning, integration, the test gate, commits, and state files.
- Use the **Agent tool** to spawn subagents for independent work, several in parallel in one message, giving each a **disjoint set of files/crates**. Examples:
  - **M0:** `engine` skeleton + offline render | `neu` theme + shadow painter | `app` window + cpal + smoke test.
  - **M2:** synth DSP (`engine/src/synth`) | FX DSP (`engine/src/fx`) | UI panels (`app/src/ui/synth_panel.rs`, `fx_rack.rs`) + `neu/knob.rs`.
  - **M3:** transport + scheduler | drum synthesis | sequencer UI | save/load + WAV export.
- If two subagents must touch overlapping files, run them sequentially or give them `isolation: "worktree"` and merge them yourself.
- Subagent briefs must be self-contained: the goal, the exact files they own, the interfaces and types they must match, the tests they must add, and "do not run the full workspace build; run only `cargo test -p <crate>` for your crate if needed."
- After subagents return, **review their diffs** before running the gate. Don't trust a report alone.
- Use a reviewer subagent at the end of each milestone to check the real-time rules (no alloc/lock in the audio callback) and the neumorphism fidelity against `design/WINNER.md`.

## State files (your memory across loop iterations)

Context gets summarized between iterations, so **all state lives in files**:
- `PROGRESS.md`: current milestone, a checklist of tasks per milestone (`[ ]` / `[x]`), the last gate result, and the next action.
- `DECISIONS.md`: dated architecture/tech decisions and why.
- `BLOCKERS.md`: open problems, attempts made, status.

## Loop procedure (every iteration)

1. **Gate check:** if `design/WINNER.md` does not exist, **stop the loop immediately** and tell me to pick a design. Never write Rust before a winner exists.
2. Read `PROGRESS.md`, `BLOCKERS.md`, `DECISIONS.md`, and `git log --oneline -10`.
3. Pick the next unchecked tasks in the current milestone. Delegate independent ones to subagents in parallel; do small ones yourself.
4. Integrate, run the **test gate**, and auto-fix per policy.
5. When green: update `PROGRESS.md`, commit with a clear message, `git push`. When a milestone's definition of done is met, tag it (`git tag m0`, `m1`, …) and push tags.
6. **Stop conditions:** stop the loop (and send me a push notification if that tool is available) when either:
   - **M3 is done and green.** Write a final summary to `PROGRESS.md`: how to run the app, key map, what works, known issues. Then stop, or
   - Every remaining task is blocked per the auto-fix policy.
7. Otherwise, schedule the next iteration promptly and continue.

## Rules

- Never commit secrets. Never print or handle tokens. Never make the repo public. Never force-push.
- Keep commits small and meaningful; push after every green gate.
- Don't add features beyond M3. Ideas go in `PROGRESS.md` under "Later".
- Windows is the target platform (WASAPI via cpal default host). Keep code cross-platform where it's free.

---

## Message 2: send this after you pick a design

```
Winner: B. Tweaks: <anything you want changed, or "none">.
Record it in design/WINNER.md per the brief, commit, push, then start the build loop.
```

Then start the unattended loop (self-paced, no interval):

```
/loop Continue the BeatSlopper build: follow docs/PLAN.md "Loop procedure" exactly, using PROGRESS.md, DECISIONS.md and BLOCKERS.md as state. Stop the loop when M3 is done or everything is blocked.
```

---

# Phase 2 brief (added 2026-10-24, after M3): M4 to M7

The user asked to continue past the first beat: polish and MIDI first, then roll out patterns
and song mode, a mixer view, and a piano roll, with the full test gate after each milestone.
Everything above (architecture, real-time rules, testability rule, test gate, auto-fix policy,
subagent strategy, state files, loop procedure) still applies. Tag each milestone (`m4`..`m7`)
and push after every green gate. Stop the loop when M7 is done and green, or when everything
remaining is blocked.

## M4: Polish + MIDI input
- **MIDI in:** `midir` (MIT) input on its own thread. A device selector in the transport (or a
  small MIDI panel) lists input ports, remembers the chosen port name, reconnects on launch,
  and shows a lamp when connected. Note on/off (velocity scaled 0..=1), note-on with velocity
  0 = note-off, all-notes-off on disconnect. MIDI notes reach the engine through their own
  lock-free queue (not through the UI frame loop) and also light the on-screen keys and feed
  bass-lane note entry and record-arm overdub like the computer keyboard.
- **Polish:** undo/redo (Ctrl+Z / Ctrl+Y) for pattern edits and parameter changes (snapshot
  stack, cap 100); keyboard shortcuts (Space play/stop, R record arm, Ctrl+S save, Ctrl+Shift+S
  save as, Ctrl+O open, Ctrl+E export, Ctrl+N new); value tooltips on every knob, dial and
  slider; the brand wordmark with the mock's +0.16em tracking; window opens at the requested
  size at 100/125/150 % display scaling (investigate and fix the eframe sizing quirk);
  "unsaved changes" marker in the project panel; a confirm step before New/Open discards
  unsaved changes (a small inline "Discard?" pill, not an OS modal).
- **Tests:** MIDI parsing (status bytes, running status, velocity 0 as off, channel ignore)
  without a device; MIDI queue messages reach the engine offline; undo/redo round trips for
  pattern and param edits; shortcuts drive play/stop/arm (kittest); tooltips exist on controls
  (kittest finds them); unsaved marker toggles.
- **Done =** I can plug in a MIDI keyboard and play, undo a wrong step, use Space to start the
  loop, and the window opens at the right size.

## M5: Patterns + song mode
- **Patterns:** up to 8 patterns per project (A..H), each the current 16-step grid (drums +
  synth lane). Pattern length 16, 32 or 64 steps (1, 2 or 4 bars) per pattern; the grid scrolls
  or pages by bar. Copy/paste/clear pattern; switching the edited pattern while playing is
  instant for editing, and the *playing* pattern switches at the next bar boundary (queued,
  with the queued pattern shown blinking).
- **Song mode:** a chain of pattern references (e.g. A A B A C), each with repeat count 1..=8,
  up to 64 entries; a Song/Pattern mode toggle in the transport; song position shown in the
  chain; the sequencer follows the song when playing in song mode; export renders the whole
  song (plus tail) or the current pattern N times.
- **Engine:** holds all patterns and the chain; scheduling stays sample-accurate across pattern
  and length changes; the project file gets `version 3` with migration from 2 (single pattern
  becomes pattern A, chain [A]).
- **Tests:** pattern switch happens exactly at the bar boundary; a 32-step pattern loops at
  2 x bar; chain A(2) B(1) plays 48 steps in the right order (onset list); project v2 migrates;
  export length for a song matches the chain; UI: pattern tabs switch the grid, queued pattern
  indicator, chain editing.
- **Done =** I can make verse/chorus patterns and chain them into a short song, and export it.

## M6: Mixer view
- **Tracks:** six channels (Kick, Snare, Clap, Closed Hat, Open Hat, Synth): level (dB), pan,
  mute, solo, and per-channel sends to the delay and reverb (the drive stays on the synth
  channel insert; the master chain becomes delay/reverb returns + master gain + limiter).
  Peak meters per channel and on the master (lock-free, decaying, from the audio thread via
  atomics). A Mixer panel toggled from the transport (replaces the FX rack area or sits as a
  tab beside it; the FX rack keeps the effect parameters).
- **Engine:** per-channel gain/pan smoothed, solo logic, meter publishing (atomic peak per
  channel per block), sends summed into the shared delay/reverb; export honours the mix.
- **Tests:** mute silences a channel, solo isolates it, pan law (-3 dB centre), send 0 means
  no reverb on that channel, meters report the rendered peak; project v4 with migration.
- **Done =** I can balance the beat: drop the hats, pan the clap, send only the snare to reverb.

## M7: Piano roll
- **Editor:** a piano-roll panel for the synth lane of the current pattern: time on X (steps
  with beat lines), pitch on Y (scrollable, about 3 octaves visible), notes as rounded bars.
  Click-drag to draw a note (snaps to 16th grid), drag to move, drag the right edge to resize,
  Delete removes, shift-click sets accent/velocity, scroll zooms vertically. The step grid's
  bass lane and the piano roll edit the same data.
- **Data/engine:** the synth lane becomes a list of notes `{ start_step, length_steps, note,
  velocity }` (polyphonic, up to 64 per pattern); the scheduler triggers note on/off at exact
  samples; the old one-note-per-step lane maps to length 1 notes; project v5 with migration.
- **Tests:** note on/off sample positions for a note of length 3 starting at step 2 (on at 2 x
  spp, off at 5 x spp); overlapping notes play polyphonically; draw/move/resize/delete via
  kittest; migration from v4.
- **Done =** I can write a melody with held notes and chords, not only one note per step.
