//! M3 sequencer offline-render tests: sample-accurate step triggers, swing, tempo, transport
//! messages, keyboard/sequencer note separation, pattern messages and block-size independence.
//!
//! Everything renders at 48 kHz. Onsets are measured through the synth lane with a click-fast
//! envelope and the FX chain dry: a voice starts at a waveform zero crossing, so the first
//! non-zero sample is at most one sample after the trigger (the tests allow +-1). The drum
//! kit is exercised by its own onset test, which checks every step from a fresh engine so it
//! does not depend on how long a drum rings.
//!
//! M7 note-off rule: a lane note (a length-1 [`engine::Note`]) now sounds until one sample
//! before the next step's trigger (the M3 build gated it at 90 % of the step), and a note of
//! the same pitch on the very next step retriggers the voice click-free, so back-to-back lane
//! steps leave no silence for the onset detector. The lanes below therefore never put notes
//! on two consecutive steps; every step is still covered (split into even/odd renders).

mod common;

use common::*;
use engine::drums::DrumRow;
use engine::engine::MAX_MASTER_GAIN;
use engine::pattern::STEPS;
use engine::{Message, Pattern, new_engine};

/// Step length at 120 BPM / 48 kHz.
const STEP_120: usize = 6000;
/// Step length at 90 BPM / 48 kHz.
const STEP_90: usize = 8000;
/// One bar at 120 BPM.
const BAR_120: usize = STEP_120 * STEPS;
/// One bar at 90 BPM.
const BAR_90: usize = STEP_90 * STEPS;
/// MIDI A4, the default lane note in these tests.
const A4: u8 = 69;
/// Anything above this counts as sound.
const EPS: f32 = ONSET_EPS;

// (a) Synth lane: every step at 120 BPM triggers at exactly n * 6000.
//
// Even and odd steps are rendered separately (see the module docs): with the M7 rule a note
// fills its whole step, so consecutive notes would merge for the onset detector. Together the
// two renders check all 16 steps.
#[test]
fn steps_trigger_every_6000_samples_at_120_bpm() {
    for parity in 0..2 {
        let (_h, mut engine) = seq_engine();
        lane(&mut engine, A4, |n| n % 2 == parity);
        let mono = play_and_render(&mut engine, BAR_120);
        assert!(all_finite(&mono));
        let expected: Vec<usize> = (parity..STEPS).step_by(2).map(|n| n * STEP_120).collect();
        assert_onsets(&onsets(&mono), &expected);

        // Each note sounds right up to the next step's trigger (its note-off is one sample
        // before it; the M3 90 % gate would already have been silent here) and the lane is
        // silent again after the 1 ms release, well before the step after that.
        for n in (parity..STEPS).step_by(2) {
            let start = n * STEP_120;
            assert!(
                !is_silent(&mono[start + 2..start + STEP_120 / 2]),
                "step {n} should sound during its first half"
            );
            assert!(
                !is_silent(&mono[start + STEP_120 - 100..start + STEP_120 - 1]),
                "step {n} should still sound just before the next step"
            );
            let quiet_from = start + STEP_120 + ms_to_frames(5.0);
            let next_note = (start + 2 * STEP_120).min(BAR_120);
            if quiet_from < next_note {
                assert!(
                    is_silent(&mono[quiet_from..next_note]),
                    "step {n} should be silent after its release"
                );
            }
        }
    }
}

// (a, drums) Every kick step from a fresh engine starts sounding at exactly n * 6000.
#[test]
fn kick_steps_trigger_every_6000_samples_at_120_bpm() {
    for n in 0..STEPS {
        let (_h, mut engine) = seq_engine();
        engine.set_drum_step(DrumRow::Kick, n as u8, true, n % 4 == 0);
        let mono = play_and_render(&mut engine, BAR_120);
        assert!(all_finite(&mono), "step {n}");
        let first = mono
            .iter()
            .position(|x| x.abs() > EPS)
            .unwrap_or_else(|| panic!("kick on step {n} never sounded"));
        assert!(
            first.abs_diff(n * STEP_120) <= 1,
            "kick on step {n} first sounds at {first}, expected {}",
            n * STEP_120
        );
    }
}

// (b) Swing 0.5 delays odd steps by half a step and leaves even steps alone.
#[test]
fn swing_half_delays_odd_steps_only() {
    // Odd steps only (so the delayed notes are separated by silence).
    let (_h, mut engine) = seq_engine();
    engine.set_swing(0.5);
    assert_eq!(engine.transport().swing(), 0.5);
    lane(&mut engine, A4, |n| n % 2 == 1);
    let mono = play_and_render(&mut engine, BAR_120);
    let expected: Vec<usize> = (0..STEPS)
        .filter(|n| n % 2 == 1)
        .map(|n| n * STEP_120 + STEP_120 / 2)
        .collect();
    assert_onsets(&onsets(&mono), &expected);

    // Even steps are unchanged.
    let (_h, mut engine) = seq_engine();
    engine.set_swing(0.5);
    lane(&mut engine, A4, |n| n % 2 == 0);
    let mono = play_and_render(&mut engine, BAR_120);
    let expected: Vec<usize> = (0..STEPS).step_by(2).map(|n| n * STEP_120).collect();
    assert_onsets(&onsets(&mono), &expected);
}

// (c) At 90 BPM a step is 8000 samples and the bar wraps after 128 000, firing step 0 again.
// (Even steps only, so every note ends in silence; see the module docs.)
#[test]
fn ninety_bpm_steps_are_8000_samples_and_the_bar_wraps() {
    let (handle, mut engine) = seq_engine();
    engine.set_bpm(90.0);
    assert_eq!(engine.transport().samples_per_step(SAMPLE_RATE), 8000.0);
    assert_eq!(engine.transport().bar_samples(SAMPLE_RATE), 128_000.0);
    lane(&mut engine, A4, |n| n % 2 == 0);

    let mono = play_and_render(&mut engine, BAR_90);
    let pos = handle.position();
    assert_eq!(pos.step, 0, "the bar wrapped back to step 0");
    assert!(pos.frac < 0.001, "frac {}", pos.frac);
    assert!(pos.playing);

    let more = left(&render(&mut engine, STEP_90));
    let all: Vec<f32> = mono.into_iter().chain(more).collect();
    let expected: Vec<usize> = (0..=STEPS).step_by(2).map(|n| n * STEP_90).collect();
    assert_onsets(&onsets(&all), &expected);
}

// (d) A tempo change mid-bar keeps the position in the bar: no step repeats or is skipped.
// (Odd steps only, so every note ends in silence; see the module docs.)
#[test]
fn bpm_change_mid_bar_neither_retriggers_nor_skips_steps() {
    let (mut handle, mut engine) = seq_engine();
    lane(&mut engine, A4, |n| n % 2 == 1);
    // 50 000 samples at 120 BPM: steps 0..=8 have fired (odd ones: 1, 3, 5, 7),
    // pos = 50 000 / 96 000 of the bar.
    let first = play_and_render(&mut engine, 50_000);
    assert_eq!(onsets(&first).len(), 4);

    assert!(handle.send(Message::SetBpm(90.0)));
    // Rescaled position: round(50 000 / 96 000 * 128 000) = 66 667, so 61 333 samples remain
    // in this bar, then one more full bar of 128 000.
    let rest = left(&render(&mut engine, 61_333 + BAR_90));
    assert_eq!(engine.bpm(), 90.0);
    let all: Vec<f32> = first.into_iter().chain(rest).collect();
    let found = onsets(&all);
    assert_eq!(found.len(), STEPS, "onsets {found:?}");

    // Step 9 of the first bar lands at 72 000 on the new grid, i.e. 5 333 samples after the
    // change; everything after that is spaced exactly two 90 BPM steps apart.
    let after: Vec<usize> = found.iter().copied().filter(|&i| i > 50_000).collect();
    assert_eq!(after.len(), STEPS - 4);
    assert!(
        after[0].abs_diff(55_333) <= 1,
        "first onset after the change: {}",
        after[0]
    );
    for w in after.windows(2) {
        assert!(
            w[1].abs_diff(w[0] + 2 * STEP_90) <= 1,
            "uneven spacing after the tempo change: {w:?}"
        );
    }
    assert_eq!(handle.position().step, 0, "exactly two bars elapsed");
}

// (e) Play / Stop messages drive the playhead and release the sequencer note.
#[test]
fn play_and_stop_messages_drive_the_position() {
    let (mut handle, mut engine) = seq_engine();
    lane(&mut engine, A4, |_| true);
    assert!(!handle.position().playing);
    assert!(!engine.is_active());

    assert!(handle.send(Message::Play));
    let out = left(&render(&mut engine, STEP_120 + 1));
    assert!(!is_silent(&out));
    let pos = handle.position();
    assert!(pos.playing);
    assert_eq!(pos.step, 1);
    assert!(pos.frac < 0.001, "frac {}", pos.frac);
    // The engine-side view agrees (the handle quantizes `frac` to thousandths).
    let own = engine.position();
    assert_eq!((own.step, own.playing), (pos.step, pos.playing));
    assert!((own.frac - pos.frac).abs() < 0.001);
    assert_eq!(
        engine
            .voices()
            .voices()
            .iter()
            .filter(|v| !v.env().is_idle())
            .count(),
        1
    );
    assert!(engine.is_active());

    // Half a step later the frac has moved on.
    let _ = render(&mut engine, STEP_120 / 2);
    let pos = handle.position();
    assert_eq!(pos.step, 1);
    assert!((pos.frac - 0.5).abs() < 0.002, "frac {}", pos.frac);

    assert!(handle.send(Message::Stop));
    let _ = render(&mut engine, 1);
    let pos = handle.position();
    assert!(!pos.playing);
    assert_eq!(pos.step, 0);
    assert_eq!(pos.frac, 0.0);
    assert!(!engine.transport().is_playing());
    assert_eq!(
        engine.voices().held_voices(),
        0,
        "the sequencer note was released"
    );

    // After the release tail nothing sounds and the engine reports idle.
    let tail = left(&render(&mut engine, ms_to_frames(50.0)));
    assert!(is_silent(&tail[ms_to_frames(10.0)..]));
    assert!(!engine.is_active());

    // Play again restarts from step 0 and fires immediately.
    assert!(handle.send(Message::Play));
    let out = left(&render(&mut engine, 100));
    assert_onsets(&onsets(&out), &[0]);
    assert_eq!(handle.position().step, 0);
}

// (f) A note held from the keyboard survives the sequencer's note-offs.
#[test]
fn keyboard_note_is_not_cut_by_the_sequencer() {
    // Different pitch: the keyboard voice is untouched by the sequencer.
    let (_h, mut engine) = seq_engine();
    lane(&mut engine, 36, |_| true);
    engine.note_on(A4, 1.0);
    engine.play();
    let _ = render(&mut engine, BAR_120);
    let held: Vec<u8> = engine
        .voices()
        .voices()
        .iter()
        .filter(|v| !v.env().is_idle() && !v.env().is_releasing())
        .map(|v| v.note())
        .collect();
    assert_eq!(held, vec![A4], "only the keyboard note is still held");

    // Same pitch: the sequencer retriggers the keyboard's voice but never releases it.
    let (_h, mut engine) = seq_engine();
    lane(&mut engine, A4, |_| true);
    engine.note_on(A4, 1.0);
    engine.play();
    let out = left(&render(&mut engine, BAR_120));
    assert_eq!(engine.voices().held_voices(), 1);
    assert!(
        !is_silent(&out[BAR_120 - 100..]),
        "the held key keeps sounding through the sequencer's gate end"
    );
    engine.stop();
    assert_eq!(
        engine.voices().held_voices(),
        1,
        "stop keeps keyboard notes"
    );
    engine.note_off(A4);
    assert_eq!(engine.voices().held_voices(), 0);
}

// (g) Pattern edits arrive through the queue.
#[test]
fn pattern_messages_edit_the_engine_pattern() {
    let (mut handle, mut engine) = seq_engine();
    for m in [
        Message::SetDrumStep {
            row: DrumRow::Snare,
            step: 4,
            on: true,
            accent: true,
        },
        Message::SetDrumStep {
            row: DrumRow::Kick,
            step: 0,
            on: true,
            accent: false,
        },
        Message::SetSynthStep {
            step: 2,
            note: Some(40),
            accent: true,
        },
        Message::SetDrumVolume {
            row: DrumRow::Clap,
            db: -6.0,
        },
        Message::SetDrumTune {
            row: DrumRow::OpenHat,
            semitones: 3.0,
        },
        Message::SetSwing(0.25),
    ] {
        assert!(handle.send(m));
    }
    let _ = render(&mut engine, 1);
    let p = engine.pattern();
    assert!(p.drums[DrumRow::Snare.index()][4].on);
    assert!(p.drums[DrumRow::Snare.index()][4].accent);
    assert!(p.drums[DrumRow::Kick.index()][0].on);
    assert!(!p.drums[DrumRow::Kick.index()][0].accent);
    assert_eq!(p.synth[2].note, Some(40));
    assert!(p.synth[2].accent);
    assert_eq!(p.drum_volume[DrumRow::Clap.index()], -6.0);
    assert_eq!(p.drum_tune[DrumRow::OpenHat.index()], 3.0);
    assert_eq!(engine.transport().swing(), 0.25);

    assert!(handle.send(Message::ClearRow(DrumRow::Snare)));
    let _ = render(&mut engine, 1);
    assert!(
        engine.pattern().drums[DrumRow::Snare.index()]
            .iter()
            .all(|s| !s.on)
    );
    assert!(engine.pattern().drums[DrumRow::Kick.index()][0].on);
    assert_eq!(engine.pattern().synth[2].note, Some(40));

    assert!(handle.send(Message::ClearSynth));
    let _ = render(&mut engine, 1);
    assert!(engine.pattern().synth.iter().all(|s| s.note.is_none()));
    assert!(engine.pattern().drums[DrumRow::Kick.index()][0].on);

    assert!(handle.send(Message::ClearAll));
    let _ = render(&mut engine, 1);
    assert!(engine.pattern().is_empty());
    // Volume and tune survive a clear.
    assert_eq!(engine.pattern().drum_volume[DrumRow::Clap.index()], -6.0);

    // Whole-pattern load.
    engine.set_pattern(&Pattern::demo());
    assert_eq!(engine.pattern(), &Pattern::demo());
}

// (h) The demo pattern at maximum master gain renders finite and limited.
#[test]
fn demo_pattern_at_max_gain_is_finite_and_limited() {
    let (_h, mut engine) = new_engine(SAMPLE_RATE);
    engine.set_pattern(&Pattern::demo());
    engine.set_master_gain(MAX_MASTER_GAIN);
    engine.set_swing(0.3);
    engine.play();
    let out = render_in_blocks(&mut engine, 2 * BAR_120, 512);
    assert!(all_finite(&out));
    assert!(peak(&out) <= 1.0, "peak {}", peak(&out));
    assert!(!is_silent(&out), "the demo bassline must be audible");
    assert!(engine.is_active());
    engine.stop();
    assert!(!engine.transport().is_playing());
}

// (i) Trigger positions do not depend on the render block size.
#[test]
fn trigger_positions_are_block_size_independent() {
    let mut results: Vec<(usize, Vec<f32>)> = Vec::new();
    for block in [1_usize, 7, 512] {
        let (_h, mut engine) = seq_engine();
        engine.set_swing(0.25);
        // Steps 0, 3, 5, 8, 11, 13 (and 16 = step 0 of the next bar): a mix of straight and
        // swung steps with no two in a row, so every note ends in silence before the next
        // one and is detectable as an onset (see the module docs).
        lane(&mut engine, A4, |n| matches!(n % 8, 0 | 3 | 5));
        engine.play();
        let mono = left(&render_in_blocks(&mut engine, BAR_120 + STEP_120, block));
        results.push((block, mono));
    }
    let reference = onsets(&results[0].1);
    for (block, mono) in &results[1..] {
        assert_eq!(onsets(mono), reference, "block size {block}");
        assert_eq!(
            mono, &results[0].1,
            "block size {block}: output differs sample-wise"
        );
    }
    // Sanity: the expected swung grid, including step 0 of the next bar.
    let expected: Vec<usize> = (0..=STEPS)
        .filter(|n| matches!(n % 8, 0 | 3 | 5))
        .map(|n| {
            let swing = if n % 2 == 1 { STEP_120 / 4 } else { 0 };
            n * STEP_120 + swing
        })
        .collect();
    assert_onsets(&reference, &expected);
}
