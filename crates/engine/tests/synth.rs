//! M2 synth offline-render tests: band-limited oscillators, filter, filter envelope, LFO,
//! glide, presets, limiter bypass and `SetParam` routing.

mod common;

use common::*;
use engine::engine::MAX_MASTER_GAIN;
use engine::presets;
use engine::synth::{Oscillator, Wave};
use engine::{Engine, LfoTarget, Message, ParamId, Preset, new_engine};

/// MIDI A4.
const A4: u8 = 69;

/// Sets a fast, flat envelope so level measurements are not coloured by attack/decay.
fn flat_envelope(engine: &mut Engine, release_s: f32) {
    engine.set_param(ParamId::AmpAttack, 0.001);
    engine.set_param(ParamId::AmpDecay, 0.001);
    engine.set_param(ParamId::AmpSustain, 1.0);
    engine.set_param(ParamId::AmpRelease, release_s);
}

/// One oscillator of `wave` at full mix, filter wide open, no modulation, flat envelope.
fn clean_patch(engine: &mut Engine, wave: Wave) {
    engine.set_param(ParamId::Osc1Wave, wave.index() as f32);
    engine.set_param(ParamId::Osc1Detune, 0.0);
    engine.set_param(ParamId::Osc1Mix, 1.0);
    engine.set_param(ParamId::Osc2Mix, 0.0);
    engine.set_param(ParamId::FilterCutoff, 20_000.0);
    engine.set_param(ParamId::FilterResonance, 0.0);
    engine.set_param(ParamId::FilterEnvAmount, 0.0);
    engine.set_param(ParamId::LfoDepth, 0.0);
    engine.set_param(ParamId::Glide, 0.0);
    flat_envelope(engine, 0.1);
}

fn clean_engine(wave: Wave) -> Engine {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    dry_fx(&mut engine);
    clean_patch(&mut engine, wave);
    engine
}

/// Plays `note`, discards `warmup_ms`, returns the next `ms` of the left channel.
fn play(engine: &mut Engine, note: u8, warmup_ms: f32, ms: f32) -> Vec<f32> {
    engine.note_on(note, 1.0);
    let _ = render(engine, ms_to_frames(warmup_ms));
    left(&render(engine, ms_to_frames(ms)))
}

#[test]
fn every_wave_sounds_at_the_right_pitch() {
    for wave in Wave::ALL {
        let mut engine = clean_engine(wave);
        let out = play(&mut engine, A4, 100.0, 1000.0);
        assert!(all_finite(&out), "{wave:?}");
        assert!(rms(&out) > 0.05, "{wave:?} rms {}", rms(&out));
        let hz = estimate_pitch_hz(&out, SAMPLE_RATE).expect("zero crossings");
        assert!((hz - 440.0).abs() < 4.4, "{wave:?} estimated {hz} Hz");
    }
}

#[test]
fn saw_is_bounded_and_finite_at_one_kilohertz() {
    let mut osc = Oscillator::new(Wave::Saw, 1_000.0, SAMPLE_RATE);
    let out: Vec<f32> = (0..48_000).map(|_| osc.tick()).collect();
    assert!(all_finite(&out));
    assert!(peak(&out) <= 1.1, "peak {}", peak(&out));
    assert!(peak(&out) > 0.9, "peak {}", peak(&out));

    // Through the engine as well (level -12 dBFS for one oscillator at full mix).
    let mut engine = clean_engine(Wave::Saw);
    let out = play(&mut engine, 83, 50.0, 500.0);
    let ideal = 10.0_f32.powf(-12.0 / 20.0);
    assert!(all_finite(&out));
    assert!(peak(&out) <= ideal * 1.1, "peak {}", peak(&out));
}

#[test]
fn polyblep_saw_aliases_less_than_a_naive_saw() {
    let hz = 5_000.0;
    let n = 48_000;
    let mut osc = Oscillator::new(Wave::Saw, hz, SAMPLE_RATE);
    let blep: Vec<f32> = (0..n).map(|_| osc.tick()).collect();

    let mut phase = 0.5_f64;
    let inc = f64::from(hz) / f64::from(SAMPLE_RATE);
    let naive: Vec<f32> = (0..n)
        .map(|_| {
            let s = (2.0 * phase - 1.0) as f32;
            phase += inc;
            if phase >= 1.0 {
                phase -= 1.0;
            }
            s
        })
        .collect();

    let b = brightness(&blep);
    let nv = brightness(&naive);
    assert!(b < nv * 0.9, "polyblep {b} vs naive {nv}");
    // Same fundamental, though.
    let hz_blep = estimate_pitch_hz(&blep, SAMPLE_RATE).unwrap();
    assert!((hz_blep - hz).abs() < hz * 0.01, "{hz_blep}");
}

#[test]
fn lowpass_cutoff_reduces_level_and_resonance_adds_it_back_near_cutoff() {
    let measure = |note: u8, cutoff: f32, res: f32| {
        let mut engine = clean_engine(Wave::Saw);
        engine.set_param(ParamId::FilterCutoff, cutoff);
        engine.set_param(ParamId::FilterResonance, res);
        let out = play(&mut engine, note, 200.0, 500.0);
        (rms(&out), brightness(&out))
    };
    // A4: the fundamental sits two octaves above a 200 Hz cutoff, so the level drops hard.
    let (open, open_bright) = measure(A4, 20_000.0, 0.0);
    let (closed, closed_bright) = measure(A4, 200.0, 0.0);
    assert!(closed < open * 0.3, "closed {closed} open {open}");
    assert!(
        closed_bright < open_bright * 0.5,
        "{closed_bright} vs {open_bright}"
    );
    // A2 (110 Hz): a 200 Hz cutoff keeps the fundamental but strips the harmonics.
    let (open, open_bright) = measure(45, 20_000.0, 0.0);
    let (closed, closed_bright) = measure(45, 200.0, 0.0);
    assert!(closed < open * 0.8, "closed {closed} open {open}");
    assert!(closed_bright < open_bright * 0.5);

    // Resonance at the 8th harmonic of A2 lifts the level near the cutoff.
    let (flat, _) = measure(45, 880.0, 0.0);
    let (peaky, _) = measure(45, 880.0, 0.9);
    assert!(peaky > flat * 1.2, "resonant {peaky} vs flat {flat}");
    assert!(peaky.is_finite());
}

#[test]
fn filter_envelope_makes_the_attack_brighter_than_the_sustain() {
    let mut engine = clean_engine(Wave::Saw);
    engine.set_param(ParamId::FilterCutoff, 300.0);
    engine.set_param(ParamId::FilterEnvAmount, 1.0);
    engine.set_param(ParamId::AmpAttack, 0.001);
    engine.set_param(ParamId::AmpDecay, 0.3);
    engine.set_param(ParamId::AmpSustain, 0.2);
    engine.note_on(45, 1.0);
    let out = left(&render(&mut engine, ms_to_frames(1500.0)));
    let early = &out[..ms_to_frames(20.0)];
    let late = &out[ms_to_frames(1200.0)..];
    assert!(!is_silent(late));
    let (eb, lb) = (brightness(early), brightness(late));
    assert!(eb > lb * 1.5, "early {eb} late {lb}");

    // Negative amount: the attack is darker instead.
    let mut engine = clean_engine(Wave::Saw);
    engine.set_param(ParamId::FilterCutoff, 3000.0);
    engine.set_param(ParamId::FilterEnvAmount, -1.0);
    engine.set_param(ParamId::AmpDecay, 0.3);
    engine.set_param(ParamId::AmpSustain, 0.2);
    engine.note_on(45, 1.0);
    let out = left(&render(&mut engine, ms_to_frames(1500.0)));
    let early = &out[..ms_to_frames(20.0)];
    let late = &out[ms_to_frames(1200.0)..];
    assert!(brightness(early) < brightness(late) * 0.8);
}

#[test]
fn lfo_on_pitch_wobbles_the_pitch() {
    let mut engine = clean_engine(Wave::Sine);
    engine.set_param(ParamId::LfoRate, 3.0);
    engine.set_param(ParamId::LfoDepth, 1.0);
    engine.set_param(ParamId::LfoTarget, LfoTarget::Pitch.index() as f32);
    let out = play(&mut engine, A4, 50.0, 1000.0);
    let pitches: Vec<f32> = per_window(&out, ms_to_frames(25.0), |w| {
        estimate_pitch_hz(w, SAMPLE_RATE).unwrap_or(0.0)
    });
    let max = pitches.iter().cloned().fold(0.0_f32, f32::max);
    let min = pitches.iter().cloned().fold(f32::MAX, f32::min);
    // +-1 semitone is about +-6 %; short windows blur it, so ask for a clear 4 % spread.
    assert!(max - min > 440.0 * 0.04, "spread {min}..{max}");
    assert!(max < 440.0 * 1.08 && min > 440.0 * 0.92, "{min}..{max}");

    // Depth 0: steady.
    let mut engine = clean_engine(Wave::Sine);
    engine.set_param(ParamId::LfoTarget, LfoTarget::Pitch.index() as f32);
    let out = play(&mut engine, A4, 50.0, 1000.0);
    let pitches: Vec<f32> = per_window(&out, ms_to_frames(25.0), |w| {
        estimate_pitch_hz(w, SAMPLE_RATE).unwrap_or(0.0)
    });
    let max = pitches.iter().cloned().fold(0.0_f32, f32::max);
    let min = pitches.iter().cloned().fold(f32::MAX, f32::min);
    assert!(max - min < 440.0 * 0.01, "spread {min}..{max}");
}

#[test]
fn lfo_on_cutoff_changes_the_spectrum_over_time() {
    let mut engine = clean_engine(Wave::Saw);
    engine.set_param(ParamId::FilterCutoff, 500.0);
    engine.set_param(ParamId::LfoRate, 2.0);
    engine.set_param(ParamId::LfoDepth, 1.0);
    engine.set_param(ParamId::LfoTarget, LfoTarget::Cutoff.index() as f32);
    let out = play(&mut engine, 45, 50.0, 1000.0);
    let bright = per_window(&out, ms_to_frames(50.0), brightness);
    let max = bright.iter().cloned().fold(0.0_f32, f32::max);
    let min = bright.iter().cloned().fold(f32::MAX, f32::min);
    assert!(max > min * 1.5, "brightness {min}..{max}");
    assert!(all_finite(&out));
}

#[test]
fn glide_passes_through_intermediate_pitches() {
    let mut engine = clean_engine(Wave::Sine);
    engine.set_param(ParamId::Glide, 0.2);
    engine.set_param(ParamId::AmpRelease, 0.001);
    engine.note_on(60, 1.0); // C4
    let _ = render(&mut engine, ms_to_frames(300.0));
    engine.note_on(72, 1.0); // C5, legato
    engine.note_off(60);
    let out = left(&render(&mut engine, ms_to_frames(1000.0)));
    let mid = &out[ms_to_frames(40.0)..ms_to_frames(60.0)];
    let hz = estimate_pitch_hz(mid, SAMPLE_RATE).expect("crossings");
    assert!(hz > 290.0 && hz < 490.0, "pitch at 50 ms: {hz} Hz");
    let end = &out[ms_to_frames(800.0)..];
    let hz = estimate_pitch_hz(end, SAMPLE_RATE).expect("crossings");
    assert!((hz - 523.25).abs() < 5.3, "arrived at {hz} Hz");

    // Glide 0: immediate.
    let mut engine = clean_engine(Wave::Sine);
    engine.set_param(ParamId::AmpRelease, 0.001);
    engine.note_on(60, 1.0);
    let _ = render(&mut engine, ms_to_frames(300.0));
    engine.note_on(72, 1.0);
    engine.note_off(60);
    let out = left(&render(&mut engine, ms_to_frames(100.0)));
    let mid = &out[ms_to_frames(40.0)..ms_to_frames(60.0)];
    let hz = estimate_pitch_hz(mid, SAMPLE_RATE).expect("crossings");
    assert!((hz - 523.25).abs() < 5.3, "{hz}");
}

#[test]
fn presets_apply_in_range_and_sound() {
    assert_eq!(presets::ALL.len(), 8);
    let names = [
        "Bass", "Sub", "Lead", "Pluck", "Pad", "Keys", "Acid", "Bell",
    ];
    for (preset, name) in presets::ALL.iter().zip(names) {
        assert_eq!(preset.name, name);
        assert!(presets::find(name).is_some());
        let (_handle, mut engine) = new_engine(SAMPLE_RATE);
        dry_fx(&mut engine);
        engine.set_param(ParamId::MasterGain, 2.0);
        preset.apply(&mut engine);
        for &id in ParamId::ALL {
            let v = engine.param(id);
            assert!(id.range().contains(&v), "{name} {id:?} = {v}");
            assert_eq!(v, preset.value(id), "{name} {id:?}");
        }
        assert_eq!(
            engine.param(ParamId::MasterGain),
            1.0,
            "{name} resets master"
        );
        engine.note_on(57, 1.0);
        let out = render(&mut engine, ms_to_frames(600.0));
        assert!(all_finite(&out), "{name}");
        assert!(rms(&out) > 1.0e-3, "{name} rms {}", rms(&out));
        assert!(peak(&out) <= 1.0, "{name} peak {}", peak(&out));
        engine.note_off(57);
        let tail = render(&mut engine, ms_to_frames(5000.0));
        assert!(all_finite(&tail), "{name}");
        assert!(!engine.is_active(), "{name} still sounding after 5 s");
    }
    assert!(presets::find("bass").is_some());
    assert!(presets::find("Organ").is_none());
    let p: &Preset = &presets::ALL[0];
    assert_eq!(p.name, "Bass");
}

#[test]
fn limiter_can_be_bypassed() {
    let measure = |limiter_on: f32| {
        let mut engine = clean_engine(Wave::Sine);
        engine.set_param(ParamId::LimiterOn, limiter_on);
        engine.set_param(ParamId::MasterGain, MAX_MASTER_GAIN);
        assert_eq!(engine.limiter_on(), limiter_on >= 0.5);
        let out = play(&mut engine, A4, 100.0, 500.0);
        assert!(all_finite(&out));
        peak(&out)
    };
    // -12 dBFS * 4 = 0 dBFS before the limiter.
    let limited = measure(1.0);
    let raw = measure(0.0);
    assert!(limited < 0.95 && limited > 0.6, "limited {limited}");
    assert!(raw > 0.95 && raw <= 1.05, "raw {raw}");
    assert!(raw > limited);

    // With a hotter signal the bypassed output exceeds full scale.
    let mut engine = clean_engine(Wave::Saw);
    engine.set_param(ParamId::LimiterOn, 0.0);
    engine.set_param(ParamId::MasterGain, MAX_MASTER_GAIN);
    for n in 0..8 {
        engine.note_on(48 + n * 3, 1.0);
    }
    let out = render(&mut engine, ms_to_frames(500.0));
    assert!(all_finite(&out));
    assert!(peak(&out) > 1.0, "peak {}", peak(&out));
}

#[test]
fn set_param_messages_reach_the_synth() {
    let (mut handle, mut engine) = new_engine(SAMPLE_RATE);
    dry_fx(&mut engine);
    assert!(handle.send(Message::SetParam(ParamId::Osc1Wave, 2.0)));
    assert!(handle.send(Message::SetParam(ParamId::FilterCutoff, 123.0)));
    assert!(handle.send(Message::SetParam(ParamId::LfoTarget, 0.0)));
    assert!(handle.send(Message::SetParam(ParamId::Glide, 0.5)));
    assert!(handle.send(Message::NoteOn {
        note: A4,
        velocity: 1.0
    }));
    let out = render(&mut engine, ms_to_frames(100.0));
    assert!(!is_silent(&out));
    let p = engine.synth_params();
    assert_eq!(p.osc1_wave, Wave::Square);
    assert_eq!(p.cutoff, 123.0);
    assert_eq!(p.lfo_target, LfoTarget::Pitch);
    assert_eq!(p.glide, 0.5);
    assert_eq!(engine.param(ParamId::Osc1Wave), 2.0);

    // Both mixes to zero through the queue: the voice goes exactly silent.
    assert!(handle.send(Message::SetParam(ParamId::Osc1Mix, 0.0)));
    assert!(handle.send(Message::SetParam(ParamId::Osc2Mix, 0.0)));
    let _settle = render(&mut engine, ms_to_frames(200.0));
    let out = render(&mut engine, ms_to_frames(100.0));
    assert!(is_silent(&out), "peak {}", peak(&out));
    assert!(engine.is_active(), "the voice is still held");

    // And back up: sound again, with a wider filter now.
    assert!(handle.send(Message::SetParam(ParamId::Osc1Mix, 1.0)));
    assert!(handle.send(Message::SetParam(ParamId::FilterCutoff, 8000.0)));
    let out = render(&mut engine, ms_to_frames(100.0));
    assert!(!is_silent(&out));
    assert_eq!(engine.synth_params().cutoff, 8000.0);
}

#[test]
fn synth_renders_identically_regardless_of_block_size() {
    let (_h1, mut single) = new_engine(SAMPLE_RATE);
    dry_fx(&mut single);
    let (_h2, mut blocked) = new_engine(SAMPLE_RATE);
    dry_fx(&mut blocked);
    for e in [&mut single, &mut blocked] {
        presets::find("Lead").unwrap().apply(e);
        e.set_param(ParamId::LfoTarget, 0.0);
        e.note_on(60, 1.0);
        e.note_on(67, 0.7);
    }
    let frames = ms_to_frames(300.0);
    let a = render(&mut single, frames);
    let b = render_in_blocks(&mut blocked, frames, 37);
    assert_eq!(a, b);
}

#[test]
fn detune_and_second_oscillator_beat_against_each_other() {
    // Two saws 20 cents apart beat; the envelope of the sum rises and falls over time.
    let mut engine = clean_engine(Wave::Saw);
    engine.set_param(ParamId::Osc1Mix, 0.5);
    engine.set_param(ParamId::Osc2Wave, Wave::Saw.index() as f32);
    engine.set_param(ParamId::Osc2Mix, 0.5);
    engine.set_param(ParamId::Osc2Detune, 20.0);
    let out = play(&mut engine, A4, 50.0, 2000.0);
    let levels = per_window(&out, ms_to_frames(20.0), rms);
    let max = levels.iter().cloned().fold(0.0_f32, f32::max);
    let min = levels.iter().cloned().fold(f32::MAX, f32::min);
    assert!(max > min * 1.3, "beating {min}..{max}");

    // Without detune the two saws are phase-locked: a steady level.
    let mut engine = clean_engine(Wave::Saw);
    engine.set_param(ParamId::Osc1Mix, 0.5);
    engine.set_param(ParamId::Osc2Wave, Wave::Saw.index() as f32);
    engine.set_param(ParamId::Osc2Mix, 0.5);
    let out = play(&mut engine, A4, 50.0, 2000.0);
    let levels = per_window(&out, ms_to_frames(20.0), rms);
    let max = levels.iter().cloned().fold(0.0_f32, f32::max);
    let min = levels.iter().cloned().fold(f32::MAX, f32::min);
    assert!(max < min * 1.1, "steady {min}..{max}");
}
