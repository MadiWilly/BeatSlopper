//! M1 offline-render tests: polyphonic voices, ADSR, voice stealing and `SetParam`.

mod common;

use common::*;
use engine::engine::MAX_MASTER_GAIN;
use engine::voice::{MAX_VOICES, VOICE_LEVEL_DB};
use engine::{Engine, Message, ParamId, new_engine};

/// MIDI A4.
const A4: u8 = 69;

/// Sets a fast, flat envelope so level measurements are not coloured by attack/decay.
fn flat_envelope(engine: &mut Engine, release_s: f32) {
    engine.set_param(ParamId::AmpAttack, 0.001);
    engine.set_param(ParamId::AmpDecay, 0.001);
    engine.set_param(ParamId::AmpSustain, 1.0);
    engine.set_param(ParamId::AmpRelease, release_s);
}

/// The M1 voice: one plain sine at full mix, filter wide open, no modulation. The M2 default
/// patch is a filtered saw + sine mix, so level and waveform-shape assertions use this.
fn sine_patch(engine: &mut Engine) {
    engine.set_param(ParamId::Osc1Wave, 0.0);
    engine.set_param(ParamId::Osc1Mix, 1.0);
    engine.set_param(ParamId::Osc2Mix, 0.0);
    engine.set_param(ParamId::FilterCutoff, 20_000.0);
    engine.set_param(ParamId::FilterResonance, 0.0);
    engine.set_param(ParamId::FilterEnvAmount, 0.0);
    engine.set_param(ParamId::LfoDepth, 0.0);
}

#[test]
fn note_on_sounds_and_note_off_goes_silent_after_release() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    dry_fx(&mut engine);
    assert!(!engine.is_active());
    engine.set_param(ParamId::AmpRelease, 0.050);

    engine.note_on(A4, 1.0);
    let out = render(&mut engine, ms_to_frames(300.0));
    assert!(rms(&out) > 0.05, "rms {}", rms(&out));
    assert!(rms(&left(&out)) > 0.05);
    assert!(rms(&right(&out)) > 0.05);
    assert!(engine.is_active());
    assert_eq!(engine.active_voices(), 1);

    engine.note_off(A4);
    let tail = render(&mut engine, ms_to_frames(400.0));
    // Still sounding right after release...
    assert!(!is_silent(&tail[..ms_to_frames(5.0) * 2]));
    // ...and exactly silent 200 ms later (release is 50 ms).
    let after = &tail[ms_to_frames(200.0) * 2..];
    assert!(is_silent(after), "tail peak {}", peak(after));
    assert!(!engine.is_active());
    assert_eq!(engine.active_voices(), 0);
}

#[test]
fn a4_pitch_is_440_hz_within_one_percent() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    dry_fx(&mut engine);
    sine_patch(&mut engine);
    flat_envelope(&mut engine, 0.1);
    engine.note_on(A4, 1.0);
    let _warmup = render(&mut engine, ms_to_frames(100.0));
    let out = left(&render(&mut engine, ms_to_frames(1000.0)));
    let hz = estimate_pitch_hz(&out, SAMPLE_RATE).expect("voice has zero crossings");
    assert!((hz - 440.0).abs() < 4.4, "estimated {hz} Hz");

    // One octave up.
    engine.all_notes_off();
    let _settle = render(&mut engine, ms_to_frames(300.0));
    engine.note_on(A4 + 12, 1.0);
    let _warmup = render(&mut engine, ms_to_frames(100.0));
    let out = left(&render(&mut engine, ms_to_frames(1000.0)));
    let hz = estimate_pitch_hz(&out, SAMPLE_RATE).expect("voice has zero crossings");
    assert!((hz - 880.0).abs() < 8.8, "estimated {hz} Hz");
}

#[test]
fn voice_level_is_minus_12_dbfs_at_full_velocity() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    dry_fx(&mut engine);
    sine_patch(&mut engine);
    flat_envelope(&mut engine, 0.1);
    engine.note_on(A4, 1.0);
    let _warmup = render(&mut engine, ms_to_frames(100.0));
    let out = render(&mut engine, ms_to_frames(500.0));
    let expected = 10.0_f32.powf(VOICE_LEVEL_DB / 20.0);
    let p = peak(&out);
    assert!(
        (p - expected).abs() < expected * 0.02,
        "peak {p}, expected {expected}"
    );
}

#[test]
fn lower_velocity_is_quieter() {
    let measure = |velocity: f32| {
        let (_handle, mut engine) = new_engine(SAMPLE_RATE);
        dry_fx(&mut engine);
        flat_envelope(&mut engine, 0.1);
        engine.note_on(A4, velocity);
        let _warmup = render(&mut engine, ms_to_frames(50.0));
        rms(&render(&mut engine, ms_to_frames(500.0)))
    };
    let loud = measure(1.0);
    let soft = measure(0.5);
    assert!(soft < loud, "soft {soft} loud {loud}");
    // Linear velocity: half velocity is half the level (within rounding).
    assert!(
        (soft - loud * 0.5).abs() < loud * 0.02,
        "soft {soft} loud {loud}"
    );
}

#[test]
fn voice_stealing_keeps_max_polyphony_and_finite_output() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    dry_fx(&mut engine);
    for n in 0..MAX_VOICES as u8 {
        engine.note_on(48 + n, 1.0);
    }
    assert_eq!(engine.active_voices(), MAX_VOICES);
    let _ = render(&mut engine, ms_to_frames(20.0));

    // The 17th distinct note steals a voice instead of growing the pool or panicking.
    engine.note_on(48 + MAX_VOICES as u8, 1.0);
    assert_eq!(engine.active_voices(), MAX_VOICES);
    let out = render(&mut engine, ms_to_frames(200.0));
    assert!(all_finite(&out));
    assert!(peak(&out) <= 1.0);
    assert!(!is_silent(&out));

    // A whole glissando over the keyboard never panics either.
    for n in 0..128_u8 {
        engine.note_on(n, 0.8);
    }
    assert_eq!(engine.active_voices(), MAX_VOICES);
    let out = render(&mut engine, ms_to_frames(100.0));
    assert!(all_finite(&out));
    assert!(peak(&out) <= 1.0);
}

#[test]
fn held_note_retriggers_instead_of_taking_a_new_voice() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    dry_fx(&mut engine);
    engine.note_on(60, 1.0);
    engine.note_on(64, 1.0);
    let _ = render(&mut engine, ms_to_frames(50.0));
    assert_eq!(engine.active_voices(), 2);

    engine.note_on(60, 0.9);
    engine.note_on(60, 0.9);
    let _ = render(&mut engine, ms_to_frames(50.0));
    assert_eq!(engine.active_voices(), 2, "retrigger must reuse the voice");

    // Releasing the note frees exactly one voice.
    engine.set_param(ParamId::AmpRelease, 0.010);
    engine.note_off(60);
    let _ = render(&mut engine, ms_to_frames(100.0));
    assert_eq!(engine.active_voices(), 1);
}

#[test]
fn retrigger_does_not_click() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    dry_fx(&mut engine);
    sine_patch(&mut engine);
    engine.set_param(ParamId::AmpAttack, 0.005);
    engine.set_param(ParamId::AmpRelease, 0.200);
    engine.note_on(A4, 1.0);
    let _ = render(&mut engine, ms_to_frames(100.0));
    engine.note_off(A4);
    let _ = render(&mut engine, ms_to_frames(50.0));
    // Retrigger mid-release: the level must continue, not jump.
    engine.note_on(A4, 1.0);
    let out = left(&render(&mut engine, ms_to_frames(10.0)));
    let max_slope = 0.26 * core::f32::consts::TAU * 440.0 / SAMPLE_RATE;
    for w in out.windows(2) {
        assert!(
            (w[1] - w[0]).abs() <= max_slope * 1.5,
            "jump {}",
            (w[1] - w[0]).abs()
        );
    }
}

#[test]
fn all_notes_off_silences_everything_within_release_time() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    dry_fx(&mut engine);
    engine.set_param(ParamId::AmpRelease, 0.050);
    for n in 0..MAX_VOICES as u8 {
        engine.note_on(48 + n, 1.0);
    }
    let _ = render(&mut engine, ms_to_frames(200.0));
    assert!(engine.is_active());

    engine.all_notes_off();
    let out = render(&mut engine, ms_to_frames(100.0));
    assert!(!is_silent(&out[..ms_to_frames(2.0) * 2]));
    let after = &out[ms_to_frames(60.0) * 2..];
    assert!(is_silent(after), "peak {}", peak(after));
    assert_eq!(engine.active_voices(), 0);
    assert!(!engine.is_active());
}

#[test]
fn messages_through_the_queue_drive_voices() {
    let (mut handle, mut engine) = new_engine(SAMPLE_RATE);
    dry_fx(&mut engine);
    assert!(handle.send(Message::SetParam(ParamId::AmpRelease, 0.020)));
    assert!(handle.send(Message::NoteOn {
        note: A4,
        velocity: 0.8
    }));
    let out = render(&mut engine, ms_to_frames(100.0));
    assert!(!is_silent(&out));
    assert_eq!(engine.active_voices(), 1);
    assert_eq!(engine.param(ParamId::AmpRelease), 0.020);

    assert!(handle.send(Message::NoteOff { note: A4 }));
    let _ = render(&mut engine, ms_to_frames(100.0));
    assert_eq!(engine.active_voices(), 0);

    assert!(handle.send(Message::NoteOn {
        note: 60,
        velocity: 1.0
    }));
    assert!(handle.send(Message::NoteOn {
        note: 64,
        velocity: 1.0
    }));
    assert!(handle.send(Message::NoteOn {
        note: 67,
        velocity: 1.0
    }));
    let _ = render(&mut engine, ms_to_frames(50.0));
    assert_eq!(engine.active_voices(), 3);
    assert!(handle.send(Message::AllNotesOff));
    let out = render(&mut engine, ms_to_frames(100.0));
    assert!(is_silent(&out[ms_to_frames(50.0) * 2..]));
    assert_eq!(engine.active_voices(), 0);
}

#[test]
fn set_param_attack_changes_the_envelope() {
    let first_10ms_rms = |attack_s: f32| {
        let (mut handle, mut engine) = new_engine(SAMPLE_RATE);
        dry_fx(&mut engine);
        assert!(handle.send(Message::SetParam(ParamId::AmpAttack, attack_s)));
        assert!(handle.send(Message::NoteOn {
            note: A4,
            velocity: 1.0
        }));
        let out = render(&mut engine, ms_to_frames(10.0));
        assert_eq!(engine.param(ParamId::AmpAttack), attack_s);
        rms(&out)
    };
    let short = first_10ms_rms(0.001);
    let long = first_10ms_rms(1.0);
    assert!(long < short * 0.1, "long {long} short {short}");
    assert!(long > 0.0, "a long attack still makes sound");
}

#[test]
fn set_param_retimes_running_voices() {
    // A note held with a very long release, then the release is shortened while it fades.
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    dry_fx(&mut engine);
    engine.set_param(ParamId::AmpRelease, 4.0);
    engine.note_on(A4, 1.0);
    let _ = render(&mut engine, ms_to_frames(100.0));
    engine.note_off(A4);
    let _ = render(&mut engine, ms_to_frames(100.0));
    assert!(engine.is_active());
    engine.set_param(ParamId::AmpRelease, 0.010);
    let out = render(&mut engine, ms_to_frames(100.0));
    assert!(!is_silent(&out[..ms_to_frames(1.0) * 2]));
    assert!(is_silent(&out[ms_to_frames(20.0) * 2..]));
    assert!(!engine.is_active());
}

#[test]
fn set_param_clamps_and_reads_back() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    for &id in ParamId::ALL {
        assert_eq!(engine.param(id), id.default_value(), "{id:?}");
        engine.set_param(id, 1.0e9);
        assert_eq!(engine.param(id), *id.range().end(), "{id:?}");
        engine.set_param(id, -1.0e9);
        assert_eq!(engine.param(id), *id.range().start(), "{id:?}");
        engine.set_param(id, f32::NAN);
        assert_eq!(engine.param(id), id.default_value(), "{id:?}");
    }
    // Master gain is shared between the parameter and the M0 setter.
    engine.set_param(ParamId::MasterGain, 2.0);
    assert_eq!(engine.master_gain(), 2.0);
    engine.set_master_gain(0.5);
    assert_eq!(engine.param(ParamId::MasterGain), 0.5);
    assert_eq!(*ParamId::MasterGain.range().end(), MAX_MASTER_GAIN);
}

#[test]
fn full_chord_at_max_gain_is_finite_and_limited() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    dry_fx(&mut engine);
    flat_envelope(&mut engine, 0.1);
    engine.set_master_gain(MAX_MASTER_GAIN);
    engine.set_test_tone(true);
    for n in 0..MAX_VOICES as u8 {
        engine.note_on(36 + n * 5, 1.0);
    }
    let out = render_in_blocks(&mut engine, ms_to_frames(2000.0), 480);
    assert!(all_finite(&out));
    let p = peak(&out);
    assert!(p <= 1.0, "peak {p}");
    assert!(p > 0.9, "the limiter should be working hard, peak {p}");
}

#[test]
fn voices_render_identically_regardless_of_block_size() {
    let (_h1, mut single) = new_engine(SAMPLE_RATE);
    dry_fx(&mut single);
    let (_h2, mut blocked) = new_engine(SAMPLE_RATE);
    dry_fx(&mut blocked);
    for e in [&mut single, &mut blocked] {
        e.note_on(60, 1.0);
        e.note_on(67, 0.7);
    }
    let frames = ms_to_frames(300.0);
    let a = render(&mut single, frames);
    let b = render_in_blocks(&mut blocked, frames, 37);
    assert_eq!(a, b);
}
