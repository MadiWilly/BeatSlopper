//! M0 offline-render tests: the engine renders into buffers at 48 kHz with no audio device.

mod common;

use common::*;
use engine::engine::{MAX_MASTER_GAIN, TEST_TONE_DB, TEST_TONE_FADE_MS, TEST_TONE_HZ};
use engine::{Message, QUEUE_CAPACITY, new_engine};

#[test]
fn silent_when_tone_off() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    assert_eq!(engine.sample_rate(), SAMPLE_RATE);
    let out = render(&mut engine, ms_to_frames(500.0));
    assert!(is_silent(&out));
    assert!(!engine.is_active());
}

#[test]
fn not_silent_when_tone_on() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    engine.set_test_tone(true);
    let out = render(&mut engine, ms_to_frames(500.0));
    assert!(rms(&out) > 0.1, "rms = {}", rms(&out));
    assert!(engine.is_active());
    // Both channels carry the tone.
    assert!(rms(&left(&out)) > 0.1);
    assert!(rms(&right(&out)) > 0.1);
}

#[test]
fn tone_level_is_minus_12_dbfs_at_unity_gain() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    engine.set_test_tone(true);
    let _warmup = render(&mut engine, ms_to_frames(100.0));
    let out = render(&mut engine, ms_to_frames(500.0));
    let expected_peak = 10.0_f32.powf(TEST_TONE_DB / 20.0);
    let p = peak(&out);
    assert!(
        (p - expected_peak).abs() < expected_peak * 0.02,
        "peak {p}, expected about {expected_peak}"
    );
    // A sine's RMS is peak / sqrt(2).
    let r = rms(&left(&out));
    assert!((r - expected_peak / 2.0_f32.sqrt()).abs() < 0.01, "rms {r}");
}

#[test]
fn tone_fades_in_without_a_click() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    engine.set_test_tone(true);
    let out = left(&render(&mut engine, ms_to_frames(TEST_TONE_FADE_MS)));
    // The very first samples are tiny; the fade ramps the level up over 5 ms.
    assert!(out[0].abs() < 1e-3, "first sample {}", out[0]);
    let first_ms = &out[..ms_to_frames(1.0)];
    let last_ms = &out[out.len() - ms_to_frames(1.0)..];
    assert!(peak(first_ms) < peak(last_ms));
    // No sample-to-sample jump larger than a full-level 440 Hz sine's maximum slope.
    let max_slope = 0.26 * core::f32::consts::TAU * TEST_TONE_HZ / SAMPLE_RATE;
    for w in out.windows(2) {
        assert!(
            (w[1] - w[0]).abs() <= max_slope * 1.5,
            "jump {}",
            (w[1] - w[0]).abs()
        );
    }
}

#[test]
fn tone_off_decays_to_silence_within_fade_time() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    engine.set_test_tone(true);
    let _warmup = render(&mut engine, ms_to_frames(200.0));

    engine.set_test_tone(false);
    let out = left(&render(&mut engine, ms_to_frames(20.0)));

    // Still sounding at the start of the fade...
    assert!(!is_silent(&out[..ms_to_frames(1.0)]));
    // ...and exactly silent once the fade has run its course (plus a little rounding slack).
    let fade_end = ms_to_frames(TEST_TONE_FADE_MS) + 8;
    assert!(
        is_silent(&out[fade_end..]),
        "tail peak {}",
        peak(&out[fade_end..])
    );
    assert!(!engine.is_active());

    // No jump in the fade-out either.
    let max_slope = 0.26 * core::f32::consts::TAU * TEST_TONE_HZ / SAMPLE_RATE;
    for w in out.windows(2) {
        assert!((w[1] - w[0]).abs() <= max_slope * 1.5);
    }
}

#[test]
fn pitch_is_440_hz_within_one_percent() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    engine.set_test_tone(true);
    let _warmup = render(&mut engine, ms_to_frames(100.0));
    let out = left(&render(&mut engine, ms_to_frames(1000.0)));
    let hz = estimate_pitch_hz(&out, SAMPLE_RATE).expect("tone has zero crossings");
    assert!(
        (hz - TEST_TONE_HZ).abs() < TEST_TONE_HZ * 0.01,
        "estimated {hz} Hz"
    );
}

#[test]
fn pitch_does_not_drift_over_a_long_render() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    engine.set_test_tone(true);
    let _warmup = render(&mut engine, ms_to_frames(100.0));
    // Ten minutes of audio, rendered in device-sized blocks.
    let frames = 600 * SAMPLE_RATE as usize;
    let out = left(&render_in_blocks(&mut engine, frames, 512));
    let hz = estimate_pitch_hz(&out, SAMPLE_RATE).expect("tone has zero crossings");
    assert!((hz - TEST_TONE_HZ).abs() < 0.01, "estimated {hz} Hz");
}

#[test]
fn output_is_finite_and_peak_bounded_at_max_gain() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    engine.set_test_tone(true);
    engine.set_master_gain(MAX_MASTER_GAIN);
    let out = render(&mut engine, ms_to_frames(1000.0));
    assert!(all_finite(&out));
    let p = peak(&out);
    assert!(p <= 1.0, "peak {p}");
    // The limiter is actually working: -12 dBFS * 4 = 0 dBFS pre-limiter, so we must be
    // well above the untouched level but below full scale.
    assert!(p > 0.6, "peak {p}");
    assert!(p < 1.0, "peak {p}");
}

#[test]
fn master_gain_is_clamped_and_rejects_non_finite() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    engine.set_master_gain(100.0);
    assert_eq!(engine.master_gain(), MAX_MASTER_GAIN);
    engine.set_master_gain(-1.0);
    assert_eq!(engine.master_gain(), 0.0);
    engine.set_master_gain(f32::NAN);
    assert_eq!(engine.master_gain(), 0.0);
    engine.set_master_gain(f32::INFINITY);
    assert_eq!(engine.master_gain(), 0.0);
    engine.set_master_gain(0.5);
    assert_eq!(engine.master_gain(), 0.5);
}

#[test]
fn master_gain_changes_are_smoothed() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    engine.set_test_tone(true);
    let settled = render(&mut engine, ms_to_frames(200.0));
    let settled_rms = rms(&settled[settled.len() / 2..]);

    engine.set_master_gain(0.0);
    // Two milliseconds later the gain has only started to move (one-pole, 10 ms).
    let early = render(&mut engine, ms_to_frames(2.0));
    let early_rms = rms(&early);
    assert!(
        early_rms > settled_rms * 0.5,
        "early {early_rms} vs {settled_rms}"
    );
    assert!(
        early_rms < settled_rms,
        "early {early_rms} vs {settled_rms}"
    );

    // Well after five time constants the output is exactly silent.
    let _tail = render(&mut engine, ms_to_frames(150.0));
    let late = render(&mut engine, ms_to_frames(50.0));
    assert!(is_silent(&late), "late peak {}", peak(&late));
}

#[test]
fn messages_through_handle_are_applied_by_render() {
    let (mut handle, mut engine) = new_engine(SAMPLE_RATE);
    assert!(is_silent(&render(&mut engine, 256)));

    assert!(handle.send(Message::TestTone(true)));
    let out = render(&mut engine, ms_to_frames(100.0));
    assert!(!is_silent(&out));
    assert!(engine.test_tone());

    assert!(handle.send(Message::MasterGain(0.0)));
    let _settle = render(&mut engine, ms_to_frames(200.0));
    assert_eq!(engine.master_gain(), 0.0);
    assert!(is_silent(&render(&mut engine, ms_to_frames(50.0))));

    assert!(handle.send(Message::MasterGain(1.0)));
    assert!(handle.send(Message::TestTone(false)));
    let _settle = render(&mut engine, ms_to_frames(200.0));
    assert!(!engine.test_tone());
    assert!(is_silent(&render(&mut engine, ms_to_frames(50.0))));
}

#[test]
fn messages_are_applied_in_order_within_one_render() {
    let (mut handle, mut engine) = new_engine(SAMPLE_RATE);
    assert!(handle.send(Message::TestTone(true)));
    assert!(handle.send(Message::TestTone(false)));
    assert!(handle.send(Message::MasterGain(2.0)));
    assert!(handle.send(Message::MasterGain(0.25)));
    let out = render(&mut engine, ms_to_frames(100.0));
    assert!(is_silent(&out));
    assert!(!engine.test_tone());
    assert_eq!(engine.master_gain(), 0.25);
}

#[test]
fn queue_reports_full_and_drops_overflow() {
    let (mut handle, mut engine) = new_engine(SAMPLE_RATE);
    assert_eq!(handle.free_slots(), QUEUE_CAPACITY);
    assert!(!handle.is_disconnected());
    for _ in 0..QUEUE_CAPACITY {
        assert!(handle.send(Message::MasterGain(0.5)));
    }
    assert_eq!(handle.free_slots(), 0);
    assert!(
        !handle.send(Message::TestTone(true)),
        "queue should be full"
    );
    engine.process_messages();
    assert_eq!(handle.free_slots(), QUEUE_CAPACITY);
    assert_eq!(engine.master_gain(), 0.5);
    assert!(
        !engine.test_tone(),
        "the dropped message must not be applied"
    );

    drop(engine);
    assert!(handle.is_disconnected());
}

#[test]
fn short_output_buffer_does_not_panic() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    engine.set_test_tone(true);
    let _warmup = render(&mut engine, ms_to_frames(100.0));

    // Odd length, fewer samples than requested: three frames rendered, trailing sample zeroed.
    let mut out = [f32::NAN; 7];
    engine.render(&mut out, 100);
    assert!(all_finite(&out));
    assert!(!is_silent(&out[..6]));
    assert_eq!(out[6], 0.0);

    // A single sample cannot hold a stereo frame: it is zeroed, nothing else happens.
    let mut one = [f32::NAN; 1];
    engine.render(&mut one, 10);
    assert_eq!(one[0], 0.0);

    // Empty buffer and zero frames are both no-ops.
    let mut empty: [f32; 0] = [];
    engine.render(&mut empty, 64);
    let mut buf = [f32::NAN; 8];
    engine.render(&mut buf, 0);
    assert!(is_silent(&buf));

    // Buffer longer than frames * 2: the unused tail is zeroed.
    let mut long = [f32::NAN; 16];
    engine.render(&mut long, 2);
    assert!(all_finite(&long));
    assert!(!is_silent(&long[..4]));
    assert!(is_silent(&long[4..]));
}

#[test]
fn rendering_is_independent_of_block_size() {
    let (_h1, mut single) = new_engine(SAMPLE_RATE);
    let (_h2, mut blocked) = new_engine(SAMPLE_RATE);
    single.set_test_tone(true);
    blocked.set_test_tone(true);
    single.set_master_gain(3.0);
    blocked.set_master_gain(3.0);
    let frames = ms_to_frames(500.0);
    let a = render(&mut single, frames);
    let b = render_in_blocks(&mut blocked, frames, 37);
    assert_eq!(a, b);
}

#[test]
fn works_at_other_sample_rates() {
    for sr in [22_050.0, 44_100.0, 96_000.0] {
        let (_handle, mut engine) = new_engine(sr);
        engine.set_test_tone(true);
        let frames = sr as usize; // one second
        let mut out = vec![0.0_f32; frames * 2];
        engine.render(&mut out, frames);
        assert!(all_finite(&out));
        assert!(peak(&out) <= 1.0);
        let mono = left(&out[frames / 10 * 2..]);
        let hz = estimate_pitch_hz(&mono, sr).expect("tone has zero crossings");
        assert!(
            (hz - TEST_TONE_HZ).abs() < TEST_TONE_HZ * 0.01,
            "{sr} Hz: {hz}"
        );
    }
}
