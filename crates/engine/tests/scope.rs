//! The master-output tap and the drum-hit bitmask (visualizers): what `render` writes is what
//! the UI reads back, the ring never panics when the UI stalls, and the kick step sets its bit.

mod common;

use common::{SAMPLE_RATE, peak, render, render_in_blocks};
use engine::{
    DrumRow, DrumStep, Pattern, SCOPE_CAPACITY_FRAMES, SCOPE_WINDOW_FRAMES, ScopeWindow, new_engine,
};

#[test]
fn tap_delivers_exactly_what_render_produced() {
    let (mut handle, mut engine) = new_engine(SAMPLE_RATE);
    let mut reader = handle.take_scope_reader().expect("reader available once");
    assert!(handle.take_scope_reader().is_none());
    engine.set_test_tone(true);

    let out = render_in_blocks(&mut engine, 1024, 256);
    assert!(peak(&out) > 0.05, "the test tone must be audible");
    assert_eq!(reader.available_frames(), 1024);

    let mut window = ScopeWindow::new();
    assert_eq!(reader.drain_into(&mut window), 1024);
    // The window holds 2048 frames; the first 1024 are still silence, the last 1024 are the
    // rendered block, sample for sample.
    let left = window.left();
    let right = window.right();
    assert!(left[..SCOPE_WINDOW_FRAMES - 1024].iter().all(|s| *s == 0.0));
    for (i, frame) in out.as_chunks::<2>().0.iter().enumerate() {
        let w = SCOPE_WINDOW_FRAMES - 1024 + i;
        assert_eq!(left[w], frame[0], "left sample {i}");
        assert_eq!(right[w], frame[1], "right sample {i}");
        assert_eq!(window.mono()[w], 0.5 * (frame[0] + frame[1]));
    }
    assert!(!window.is_silent(0.01));
}

#[test]
fn overflow_skips_without_panicking_and_recovers_after_a_drain() {
    let (mut handle, mut engine) = new_engine(SAMPLE_RATE);
    let mut reader = handle.take_scope_reader().unwrap();
    engine.set_test_tone(true);
    // Nobody drains: render far more than the ring holds.
    let total = SCOPE_CAPACITY_FRAMES * 3;
    let out = render_in_blocks(&mut engine, total, 480);
    assert!(
        peak(&out) > 0.05,
        "rendering must not be affected by a full tap"
    );
    assert_eq!(reader.available_frames(), SCOPE_CAPACITY_FRAMES);

    // Draining everything gives the oldest audio (the ring keeps what arrived first) and
    // frees the ring, so the next block flows again.
    let mut window = ScopeWindow::new();
    assert_eq!(reader.drain_into(&mut window), SCOPE_CAPACITY_FRAMES);
    assert_eq!(reader.available_frames(), 0);
    let block = render(&mut engine, 512);
    assert_eq!(reader.available_frames(), 512);
    let mut fresh = ScopeWindow::new();
    reader.drain_into(&mut fresh);
    let tail = &fresh.left()[SCOPE_WINDOW_FRAMES - 512..];
    for (i, frame) in block.as_chunks::<2>().0.iter().enumerate() {
        assert_eq!(tail[i], frame[0]);
    }
}

#[test]
fn hit_bits_latch_the_rows_that_triggered_until_taken() {
    let (handle, mut engine) = new_engine(SAMPLE_RATE);
    let mut pattern = Pattern::default();
    pattern.clear_all();
    let on = DrumStep {
        on: true,
        accent: false,
    };
    pattern.drums[DrumRow::Kick.index()][0] = on;
    pattern.drums[DrumRow::Snare.index()][4] = on;
    pattern.drums[DrumRow::ClosedHat.index()][4] = on;
    engine.set_pattern(&pattern);
    engine.play();

    assert_eq!(handle.hits(), 0, "nothing rendered yet");
    // 120 BPM at 48 kHz: 6000 samples per step. The first block holds step 0.
    render(&mut engine, 512);
    assert_eq!(handle.hits(), 1 << DrumRow::Kick.index());
    assert_eq!(handle.take_hits(), 1 << DrumRow::Kick.index());
    assert_eq!(handle.take_hits(), 0, "taking clears the latch");

    // Steps 1..=3 hold no hits; two blocks across step 4 set snare and closed hat together.
    render(&mut engine, 6000 * 4 - 512 - 100);
    assert_eq!(handle.take_hits(), 0);
    render(&mut engine, 200);
    let expected = (1 << DrumRow::Snare.index()) | (1 << DrumRow::ClosedHat.index());
    assert_eq!(handle.hits(), expected);
    // Hits stay latched across blocks that trigger nothing, until taken.
    render(&mut engine, 200);
    assert_eq!(handle.take_hits(), expected);
    assert_eq!(handle.take_hits(), 0);
}
