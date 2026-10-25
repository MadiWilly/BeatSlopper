//! M5 offline-render tests: pattern switching at the bar boundary, 32/64-step patterns, the
//! song chain (repeats, wrapping, empty chain), play-mode changes, length changes while
//! playing, bank/pattern messages and the extended `Position`.
//!
//! Everything renders at 48 kHz / 120 BPM (6000 frames per step, 96 000 per bar). Onsets are
//! measured through the synth lane with the click-fast patch from `tests/common`
//! (`seq_engine`, `onsets`); a voice starts at a zero crossing, so the first non-zero sample
//! is at most one sample after the trigger.

mod common;

use common::*;
use engine::drums::DrumRow;
use engine::dsp::db_to_gain;
use engine::pattern::{BAR_STEPS, MAX_CHAIN, MAX_STEPS, PATTERN_COUNT};
use engine::{Bank, ChainEntry, Engine, Message, Pattern, PatternLength, PlayMode, STEPS};

/// Step length at 120 BPM / 48 kHz.
const SPP: usize = 6000;
/// One 16-step bar.
const BAR: usize = SPP * STEPS;
/// MIDI A4.
const A4: u8 = 69;

/// Pattern `index` gets a single lane note on step `step` (and the given length).
fn single_note(engine: &mut Engine, index: u8, step: u8, length: PatternLength) {
    engine.set_edit_pattern(index);
    let mut p = Pattern::empty();
    p.set_length(length);
    p.set_synth_step(0, step, Some(A4), false);
    engine.set_pattern(&p);
}

/// A: note on step 0; B: note on step 4; both one bar. Edit pattern back on A.
fn ab_engine() -> (engine::EngineHandle, Engine) {
    let (handle, mut engine) = seq_engine();
    single_note(&mut engine, 0, 0, PatternLength::Bar1);
    single_note(&mut engine, 1, 4, PatternLength::Bar1);
    engine.set_edit_pattern(0);
    (handle, engine)
}

// (a) SelectPattern while playing switches exactly at the next 16-step bar boundary.
#[test]
fn select_pattern_while_playing_switches_at_the_next_bar() {
    let (mut handle, mut engine) = ab_engine();
    // Bar 0 of A: the kick-position note on step 0.
    let first = play_and_render(&mut engine, 1000);
    assert_onsets(&onsets(&first), &[0]);
    let pos = handle.position();
    assert_eq!((pos.pattern, pos.bar, pos.queued), (0, 0, None));

    // Queue B one step in: nothing changes until the bar wraps.
    assert!(handle.send(Message::SelectPattern(1)));
    let rest = left(&render(&mut engine, BAR - 2000));
    let pos = handle.position();
    assert_eq!(pos.queued, Some(1), "B is queued while bar 0 plays out");
    assert_eq!(pos.step, 15);
    assert_eq!(pos.pattern, 0, "A keeps playing until the boundary");
    assert_eq!(
        engine.edit_pattern(),
        1,
        "SelectPattern also moves the edit pattern"
    );
    assert_eq!(engine.bank().current, 1);

    // Bar 1: B plays (its note is on step 4), no A note on step 0 of bar 1.
    let bar1 = left(&render(&mut engine, BAR));
    let pos = handle.position();
    assert_eq!((pos.pattern, pos.bar, pos.queued), (1, 0, None));
    let all: Vec<f32> = first.into_iter().chain(rest).chain(bar1).collect();
    assert_onsets(&onsets(&all), &[0, BAR + 4 * SPP]);

    // B loops: bar 2 is B again.
    let bar2 = left(&render(&mut engine, BAR));
    assert_onsets(&onsets(&bar2), &[4 * SPP + 1000]);

    // Re-selecting the playing pattern cancels a pending queue.
    assert!(handle.send(Message::SelectPattern(0)));
    let _ = render(&mut engine, 10);
    assert_eq!(handle.position().queued, Some(0));
    assert!(handle.send(Message::SelectPattern(1)));
    let _ = render(&mut engine, 10);
    assert_eq!(handle.position().queued, None);
    assert_eq!(handle.position().pattern, 1);
}

// (a2) When stopped the switch is immediate and Play starts at step 0 of the new pattern.
#[test]
fn select_pattern_while_stopped_is_immediate() {
    let (mut handle, mut engine) = ab_engine();
    assert!(handle.send(Message::SelectPattern(1)));
    let _ = render(&mut engine, 1);
    let pos = handle.position();
    assert_eq!((pos.pattern, pos.queued, pos.playing), (1, None, false));
    assert_eq!(engine.edit_pattern(), 1);
    let mono = play_and_render(&mut engine, BAR);
    assert_onsets(&onsets(&mono), &[4 * SPP]);
    // Stop rewinds; Play restarts at step 0 of the current pattern.
    engine.stop();
    let pos = engine.position();
    assert_eq!((pos.pattern, pos.bar, pos.step), (1, 0, 0));
    engine.select_pattern(0);
    let mono = play_and_render(&mut engine, SPP);
    assert_onsets(&onsets(&mono), &[0]);
}

// (b) A 32-step pattern loops after two bars.
#[test]
fn thirty_two_step_pattern_loops_at_two_bars() {
    let (handle, mut engine) = seq_engine();
    single_note(&mut engine, 0, 0, PatternLength::Bar2);
    assert_eq!(engine.pattern().len(), 32);
    let mono = play_and_render(&mut engine, 2 * BAR + SPP);
    assert_onsets(&onsets(&mono), &[0, 32 * SPP]);
    let pos = handle.position();
    assert_eq!((pos.pattern, pos.bar, pos.step), (0, 0, 1));

    // A note in the second bar (step 20) fires at 20 x spp, and again one pattern later.
    let (_h, mut engine) = seq_engine();
    single_note(&mut engine, 0, 20, PatternLength::Bar2);
    let mono = play_and_render(&mut engine, 4 * BAR);
    assert_onsets(&onsets(&mono), &[20 * SPP, 52 * SPP]);
    let pos = engine.position();
    assert_eq!((pos.bar, pos.step), (0, 0), "two full passes");

    // Position.bar counts bars within the pattern.
    let (handle, mut engine) = seq_engine();
    single_note(&mut engine, 0, 0, PatternLength::Bar4);
    engine.play();
    for bar in 0..4_u8 {
        let _ = render(&mut engine, BAR / 2);
        assert_eq!(handle.position().bar, bar);
        assert_eq!(handle.position().step, 8);
        assert_eq!(handle.position().pattern_step(), usize::from(bar) * 16 + 8);
        let _ = render(&mut engine, BAR / 2);
    }
    assert_eq!(handle.position().bar, 0, "wrapped after four bars");
}

// (c) Chain A(2) B(1): A, A, B, then A again.
#[test]
fn chain_plays_entries_in_order_with_repeats() {
    let (handle, mut engine) = ab_engine();
    engine.set_chain_len(0);
    engine.set_chain_entry(0, 0, 2);
    engine.set_chain_entry(1, 1, 1);
    engine.set_play_mode(PlayMode::Song);
    assert_eq!(engine.bank().chain_entries().len(), 2);
    assert_eq!(engine.bank().song_steps(), 48);

    let mono = play_and_render(&mut engine, 4 * BAR);
    assert_onsets(&onsets(&mono), &[0, 16 * SPP, 36 * SPP, 48 * SPP]);
    let pos = handle.position();
    assert_eq!(pos.pattern, 0);
    assert_eq!(pos.song_index, 0, "wrapped back to the first entry");
    assert_eq!(pos.bar, 0);
    assert_eq!(pos.queued, None);
}

// (c2) The same chain through messages, and the song position is reported along the way.
#[test]
fn chain_messages_and_song_index_through_the_handle() {
    let (mut handle, mut engine) = ab_engine();
    for m in [
        Message::SetChainLen(0),
        Message::SetChainEntry {
            index: 0,
            pattern: 0,
            repeats: 2,
        },
        Message::SetChainEntry {
            index: 1,
            pattern: 1,
            repeats: 1,
        },
        Message::SetPlayMode(PlayMode::Song),
        Message::Play,
    ] {
        assert!(handle.send(m));
    }
    let expected = [(0_u8, 0_u8), (0, 0), (1, 1), (0, 0)];
    let mut all = Vec::new();
    for (bar, (song_index, pattern)) in expected.into_iter().enumerate() {
        all.extend(left(&render(&mut engine, BAR / 2)));
        let pos = handle.position();
        assert_eq!(pos.song_index, song_index, "bar {bar}");
        assert_eq!(pos.pattern, pattern, "bar {bar}");
        assert_eq!(pos.bar, 0, "bar {bar}: every chain pattern is one bar");
        assert!(pos.playing);
        all.extend(left(&render(&mut engine, BAR / 2)));
    }
    assert_onsets(&onsets(&all), &[0, 16 * SPP, 36 * SPP, 48 * SPP]);
}

// (d) Song mode with repeats wraps the chain, and multi-bar patterns count as their bars.
#[test]
fn song_mode_wraps_the_chain_and_honours_pattern_lengths() {
    let (handle, mut engine) = seq_engine();
    single_note(&mut engine, 0, 0, PatternLength::Bar1); // A: 1 bar, hit at 0
    single_note(&mut engine, 1, 20, PatternLength::Bar2); // B: 2 bars, hit in its 2nd bar
    single_note(&mut engine, 2, 8, PatternLength::Bar1); // C: 1 bar, hit at 8
    let mut bank = engine.bank().clone();
    bank.truncate(0);
    bank.push(ChainEntry::new(0, 1));
    bank.push(ChainEntry::new(1, 1));
    bank.push(ChainEntry::new(2, 2));
    bank.mode = PlayMode::Song;
    engine.set_bank(&bank);
    assert_eq!(engine.bank().song_bars(), 1 + 2 + 2);

    // Two passes of the five-bar song.
    let mono = play_and_render(&mut engine, 10 * BAR);
    let pass = [0, (16 + 20) * SPP, (48 + 8) * SPP, (64 + 8) * SPP];
    let expected: Vec<usize> = pass
        .iter()
        .copied()
        .chain(pass.iter().map(|o| o + 5 * BAR))
        .collect();
    assert_onsets(&onsets(&mono), &expected);
    let pos = handle.position();
    assert_eq!((pos.song_index, pos.pattern, pos.bar), (0, 0, 0));

    // Position during pass 1: entry 2 (C), second repeat, is bar 4 -> song_index 2.
    let (handle, mut engine) = seq_engine();
    engine.set_bank(&bank);
    engine.play();
    let _ = render(&mut engine, 4 * BAR + SPP);
    let pos = handle.position();
    assert_eq!(
        (pos.song_index, pos.pattern, pos.bar, pos.step),
        (2, 2, 0, 1)
    );
    let _ = render(&mut engine, 2 * BAR);
    let pos = handle.position();
    assert_eq!(
        (pos.song_index, pos.pattern, pos.bar, pos.step),
        (1, 1, 0, 1)
    );
}

// (d2) An empty chain in song mode plays the current pattern, and SelectPattern queues.
#[test]
fn empty_chain_in_song_mode_plays_the_current_pattern() {
    let (mut handle, mut engine) = ab_engine();
    engine.set_chain_len(0);
    engine.set_play_mode(PlayMode::Song);
    engine.select_pattern(1);
    let mono = play_and_render(&mut engine, 2 * BAR);
    assert_onsets(&onsets(&mono), &[4 * SPP, BAR + 4 * SPP]);
    // Sent at the very start of bar 2, the switch waits for the *next* boundary: bar 2 is
    // still B, bar 3 is A.
    assert!(handle.send(Message::SelectPattern(0)));
    let mono = left(&render(&mut engine, 2 * BAR));
    assert_onsets(&onsets(&mono), &[4 * SPP, BAR]);
    assert_eq!(handle.position().pattern, 0);
}

// (e) Position fields through the handle, and queued is None outside pattern mode.
#[test]
fn position_reports_pattern_bar_song_index_and_queue() {
    let (mut handle, mut engine) = ab_engine();
    let pos = handle.position();
    assert_eq!(pos, engine::Position::stopped(0));
    assert!(handle.send(Message::Play));
    let _ = render(&mut engine, SPP * 3 + 100);
    let pos = handle.position();
    assert_eq!(pos.step, 3);
    assert!((pos.frac - 100.0 / SPP as f32).abs() < 0.002);
    assert_eq!(
        (pos.pattern, pos.bar, pos.song_index, pos.queued),
        (0, 0, 0, None)
    );
    assert!(pos.playing);
    assert_eq!(pos.pattern_step(), 3);

    assert!(handle.send(Message::SelectPattern(1)));
    let _ = render(&mut engine, 1);
    assert_eq!(handle.position().queued, Some(1));
    assert_eq!(engine.queued_pattern(), Some(1));
    assert_eq!(engine.playing_pattern(), 0);
    // The engine-side view agrees.
    let own = engine.position();
    assert_eq!(
        (own.pattern, own.bar, own.song_index, own.queued),
        (0, 0, 0, Some(1))
    );

    // Switching to song mode drops the queue; the chain restarts at the next bar.
    engine.set_chain_len(0);
    engine.set_chain_entry(0, 1, 1);
    assert!(handle.send(Message::SetPlayMode(PlayMode::Song)));
    let _ = render(&mut engine, 1);
    assert_eq!(handle.position().queued, None);
    let _ = render(&mut engine, BAR);
    let pos = handle.position();
    assert_eq!((pos.pattern, pos.song_index, pos.bar), (1, 0, 0));

    // Stop rewinds everything.
    assert!(handle.send(Message::Stop));
    let _ = render(&mut engine, 1);
    let pos = handle.position();
    assert_eq!(
        (pos.step, pos.bar, pos.song_index, pos.queued, pos.playing),
        (0, 0, 0, None, false)
    );
    assert_eq!(
        pos.pattern, 1,
        "song mode starts on chain entry 0's pattern"
    );
}

// (f) A length change of the playing pattern takes effect at the bar boundary.
#[test]
fn length_change_while_playing_applies_at_the_bar_boundary() {
    // Grow 16 -> 32 during bar 0: bar 1 is the (empty) second half, the restart is at 32.
    let (mut handle, mut engine) = seq_engine();
    single_note(&mut engine, 0, 0, PatternLength::Bar1);
    let first = play_and_render(&mut engine, 1000);
    assert!(handle.send(Message::SetPatternLength {
        pattern: 0,
        steps: 32
    }));
    let rest = left(&render(&mut engine, 4 * BAR - 1000));
    assert_eq!(engine.pattern().len(), 32);
    let all: Vec<f32> = first.into_iter().chain(rest).collect();
    assert_onsets(&onsets(&all), &[0, 32 * SPP]);
    assert_eq!(handle.position().bar, 0);

    // Shrink 64 -> 16 during bar 0: the pattern restarts at the next bar boundary.
    let (mut handle, mut engine) = seq_engine();
    single_note(&mut engine, 0, 0, PatternLength::Bar4);
    let first = play_and_render(&mut engine, 1000);
    assert!(handle.send(Message::SetPatternLength {
        pattern: 0,
        steps: 16
    }));
    let rest = left(&render(&mut engine, 3 * BAR - 1000));
    let all: Vec<f32> = first.into_iter().chain(rest).collect();
    assert_onsets(&onsets(&all), &[0, BAR, 2 * BAR]);
    assert_eq!(handle.position().bar, 0);

    // Shrink 64 -> 16 while in bar 2: bar 2 plays out (its steps still fire), then restart.
    let (mut handle, mut engine) = seq_engine();
    single_note(&mut engine, 0, 0, PatternLength::Bar4);
    engine.set_synth_step(40, Some(A4), false); // bar 2, step 8
    let first = play_and_render(&mut engine, 2 * BAR + 1000);
    assert!(handle.send(Message::SetPatternLength {
        pattern: 0,
        steps: 16
    }));
    let rest = left(&render(&mut engine, 2 * BAR - 1000));
    let all: Vec<f32> = first.into_iter().chain(rest).collect();
    assert_onsets(&onsets(&all), &[0, 40 * SPP, 48 * SPP]);
    assert_eq!(handle.position().bar, 0);
    assert_eq!(
        engine.pattern().synth[40].note,
        Some(A4),
        "hidden steps are kept"
    );
}

// (g) Pattern-bank messages: edit pattern, copy, clear, chain editing, mode.
#[test]
fn bank_messages_edit_the_bank() {
    let (mut handle, mut engine) = seq_engine();
    for m in [
        Message::SetEditPattern(2),
        Message::SetSynthStep {
            step: 1,
            note: Some(50),
            accent: true,
        },
        Message::SetDrumStep {
            row: DrumRow::Snare,
            step: 3,
            on: true,
            accent: false,
        },
        Message::SetDrumVolume {
            row: DrumRow::Kick,
            db: -4.0,
        },
        Message::SetPatternLength {
            pattern: 2,
            steps: 40,
        },
        Message::CopyPattern { from: 2, to: 5 },
        Message::ClearPattern(2),
        Message::SetChainLen(3),
        Message::SetChainEntry {
            index: 1,
            pattern: 5,
            repeats: 20,
        },
        Message::SetChainEntry {
            index: 6,
            pattern: 9,
            repeats: 0,
        },
        Message::SetPlayMode(PlayMode::Song),
    ] {
        assert!(handle.send(m));
    }
    let _ = render(&mut engine, 1);
    assert_eq!(engine.edit_pattern(), 2);
    let bank = engine.bank();
    assert!(bank.patterns[0].is_empty(), "pattern A was never touched");
    assert!(bank.patterns[2].is_empty(), "C was cleared");
    assert_eq!(
        bank.patterns[2].len(),
        64,
        "40 rounds up; clear keeps the length"
    );
    assert_eq!(bank.patterns[2].drum_volume[DrumRow::Kick.index()], -4.0);
    let f = &bank.patterns[5];
    assert_eq!(f.len(), 64);
    assert_eq!(f.synth[1].note, Some(50));
    assert!(f.synth[1].accent);
    assert!(f.drums[DrumRow::Snare.index()][3].on);
    assert_eq!(f.drum_volume[DrumRow::Kick.index()], -4.0);
    assert_eq!(bank.chain_len, 7);
    assert_eq!(bank.chain[0], ChainEntry::new(0, 1));
    assert_eq!(bank.chain[1], ChainEntry::new(5, 8), "repeats clamp to 8");
    assert_eq!(bank.chain[2], ChainEntry::new(0, 1));
    assert_eq!(
        bank.chain[6],
        ChainEntry::new(7, 1),
        "pattern clamps, repeats >= 1"
    );
    assert_eq!(bank.mode, PlayMode::Song);
    assert_eq!(engine.play_mode(), PlayMode::Song);

    // Out-of-range chain length clamps; SetChainLen resets the new entries.
    assert!(handle.send(Message::SetChainLen(200)));
    let _ = render(&mut engine, 1);
    assert_eq!(engine.bank().chain_len as usize, MAX_CHAIN);
    assert_eq!(engine.bank().chain[63], ChainEntry::default());
    assert_eq!(engine.bank().chain[1], ChainEntry::new(5, 8), "kept");

    // Edit-pattern index wraps.
    assert!(handle.send(Message::SetEditPattern(PATTERN_COUNT as u8 + 3)));
    let _ = render(&mut engine, 1);
    assert_eq!(engine.edit_pattern(), 3);
    assert_eq!(BAR_STEPS, 16);
    assert_eq!(MAX_STEPS, 64);
}

// (g2) set_bank / bank round trip and edit vs playing drum settings.
#[test]
fn set_bank_loads_everything_and_kit_settings_follow_the_playing_pattern() {
    let (_h, mut engine) = seq_engine();
    let mut bank = Bank::demo();
    bank.current = 1;
    bank.patterns[1].set_drum_volume(DrumRow::Kick, -12.0);
    bank.patterns[0].set_drum_tune(DrumRow::Kick, 7.0);
    engine.set_bank(&bank);
    assert_eq!(engine.bank(), &bank);
    assert_eq!(engine.edit_pattern(), 1);
    assert_eq!(engine.playing_pattern(), 1);
    assert_eq!(engine.pattern(), &bank.patterns[1]);
    assert_eq!(engine.drums().volume_gain(DrumRow::Kick), db_to_gain(-12.0));
    assert_eq!(engine.drums().tune(DrumRow::Kick), 0.0);

    // Editing a pattern that is not playing stores the value but leaves the kit alone.
    engine.set_edit_pattern(0);
    engine.set_drum_volume(DrumRow::Kick, 3.0);
    assert_eq!(
        engine.bank().patterns[0].drum_volume[DrumRow::Kick.index()],
        3.0
    );
    assert_eq!(engine.drums().volume_gain(DrumRow::Kick), db_to_gain(-12.0));

    // Selecting A applies its settings to the kit.
    engine.select_pattern(0);
    assert_eq!(engine.drums().volume_gain(DrumRow::Kick), db_to_gain(3.0));
    assert_eq!(engine.drums().tune(DrumRow::Kick), 7.0);

    // set_pattern replaces the edit pattern and normalises it.
    let mut p = Pattern::demo();
    p.length = 50;
    engine.set_pattern(&p);
    assert_eq!(engine.pattern().len(), 64);

    // The default bank of a fresh engine is empty.
    let (_h, fresh) = engine::new_engine(SAMPLE_RATE);
    assert_eq!(fresh.bank(), &Bank::empty());
    assert!(fresh.bank().patterns.iter().all(Pattern::is_empty));
}

// (h) Switching modes while playing: song -> pattern queues the selected pattern.
#[test]
fn switching_to_pattern_mode_while_playing_queues_the_selected_pattern() {
    let (handle, mut engine) = ab_engine();
    engine.set_chain_len(0);
    engine.set_chain_entry(0, 1, 1); // the song is just B
    engine.set_play_mode(PlayMode::Song);
    engine.select_pattern(0); // selection does not disturb the song
    let mono = play_and_render(&mut engine, BAR + 1000);
    assert_onsets(&onsets(&mono), &[4 * SPP]);
    assert_eq!(handle.position().pattern, 1);

    engine.set_play_mode(PlayMode::Pattern);
    let _ = render(&mut engine, 1);
    assert_eq!(handle.position().queued, Some(0));
    let mono = left(&render(&mut engine, 2 * BAR - 999));
    // B finishes bar 1 (its hit at step 4), then bar 2 is A (hit at its step 0), bar 3 is A again.
    assert_onsets(
        &onsets(&mono),
        &[4 * SPP - 1001, BAR - 1001, 2 * BAR - 1001],
    );
    assert_eq!(handle.position().pattern, 0);
    assert_eq!(handle.position().queued, None);
}

// (i) Block-size independence holds across pattern switches, chains and length changes.
#[test]
fn song_output_is_block_size_independent() {
    let mut results: Vec<(usize, Vec<f32>)> = Vec::new();
    for block in [1_usize, 7, 512] {
        let (mut handle, mut engine) = seq_engine();
        single_note(&mut engine, 0, 0, PatternLength::Bar1);
        single_note(&mut engine, 1, 20, PatternLength::Bar2);
        engine.set_chain_len(0);
        engine.set_chain_entry(0, 0, 1);
        engine.set_chain_entry(1, 1, 1);
        engine.set_play_mode(PlayMode::Song);
        engine.play();
        let mut mono = left(&render_in_blocks(&mut engine, BAR + 10, block));
        assert!(handle.send(Message::SetPatternLength {
            pattern: 1,
            steps: 16
        }));
        mono.extend(left(&render_in_blocks(&mut engine, 3 * BAR - 10, block)));
        results.push((block, mono));
    }
    // Bar 0: A (hit at 0). Bar 1: B's first bar; B is shortened to one bar during it, so the
    // chain advances at the boundary and B's step-20 note is never reached. Bar 2: A (hit at
    // 2 x BAR). Bar 3: B again (no hit in its first bar).
    let reference = onsets(&results[0].1);
    assert_onsets(&reference, &[0, 2 * BAR]);
    for (block, mono) in &results[1..] {
        assert_eq!(onsets(mono), reference, "block size {block}");
        assert_eq!(mono, &results[0].1, "block size {block}: sample-wise");
    }
}
