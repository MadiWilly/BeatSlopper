//! M8 synth offline-render tests: pulse width, FM, noise, pluck mode, filter types, the
//! filter ADSR, key tracking, velocity, the pitch envelope, unison and the sub oscillator.

mod common;

use common::*;
use engine::synth::{FilterType, NoiseColor, Wave};
use engine::{Engine, ParamId, new_engine};

/// MIDI A3 (220 Hz) and A4 (440 Hz).
const A3: u8 = 57;
const A4: u8 = 69;

/// Sets a fast, flat amp envelope so level measurements are not coloured by attack/decay.
fn flat_envelope(engine: &mut Engine, release_s: f32) {
    engine.set_param(ParamId::AmpAttack, 0.001);
    engine.set_param(ParamId::AmpDecay, 0.001);
    engine.set_param(ParamId::AmpSustain, 1.0);
    engine.set_param(ParamId::AmpRelease, release_s);
}

/// One oscillator of `wave` at full mix, filter wide open, every M8 modulation off, flat
/// envelope, dry FX.
fn clean_engine(wave: Wave) -> Engine {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    dry_fx(&mut engine);
    engine.set_param(ParamId::Osc1Wave, wave.index() as f32);
    engine.set_param(ParamId::Osc1Detune, 0.0);
    engine.set_param(ParamId::Osc1Mix, 1.0);
    engine.set_param(ParamId::Osc2Mix, 0.0);
    engine.set_param(ParamId::FilterCutoff, 20_000.0);
    engine.set_param(ParamId::FilterResonance, 0.0);
    engine.set_param(ParamId::FilterEnvAmount, 0.0);
    engine.set_param(ParamId::FilterKeyTrack, 0.0);
    engine.set_param(ParamId::VelToCutoff, 0.0);
    engine.set_param(ParamId::LfoDepth, 0.0);
    engine.set_param(ParamId::Glide, 0.0);
    engine.set_param(ParamId::PitchEnvAmount, 0.0);
    flat_envelope(&mut engine, 0.1);
    engine
}

/// Plays `note` at full velocity, discards `warmup_ms`, returns the next `ms` (left channel).
fn play(engine: &mut Engine, note: u8, warmup_ms: f32, ms: f32) -> Vec<f32> {
    play_vel(engine, note, 1.0, warmup_ms, ms)
}

fn play_vel(engine: &mut Engine, note: u8, velocity: f32, warmup_ms: f32, ms: f32) -> Vec<f32> {
    engine.note_on(note, velocity);
    let _ = render(engine, ms_to_frames(warmup_ms));
    left(&render(engine, ms_to_frames(ms)))
}

/// Autocorrelation pitch estimate searching lags between `lo_hz` and `hi_hz` (robust against
/// the extra zero crossings of a harmonic-rich or noisy signal).
fn autocorr_pitch_hz(buf: &[f32], lo_hz: f32, hi_hz: f32) -> f32 {
    let min_lag = (SAMPLE_RATE / hi_hz).floor() as usize;
    let max_lag = (SAMPLE_RATE / lo_hz).ceil() as usize;
    assert!(
        buf.len() > max_lag + 2,
        "buffer too short for the lag range"
    );
    let n = buf.len() - max_lag - 1;
    let corr = |lag: usize| -> f64 {
        (0..n)
            .map(|i| f64::from(buf[i]) * f64::from(buf[i + lag]))
            .sum()
    };
    let (mut best_lag, mut best) = (min_lag, f64::MIN);
    for lag in min_lag..=max_lag {
        let c = corr(lag);
        if c > best {
            best = c;
            best_lag = lag;
        }
    }
    let (c0, c1, c2) = (corr(best_lag - 1), best, corr(best_lag + 1));
    let denom = c0 - 2.0 * c1 + c2;
    let delta = if denom.abs() > 1e-12 && best_lag > min_lag && best_lag < max_lag {
        (0.5 * (c0 - c2) / denom).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    (f64::from(SAMPLE_RATE) / (best_lag as f64 + delta)) as f32
}

fn spread(values: &[f32]) -> (f32, f32) {
    let max = values.iter().cloned().fold(f32::MIN, f32::max);
    let min = values.iter().cloned().fold(f32::MAX, f32::min);
    (min, max)
}

#[test]
fn pulse_width_changes_brightness_but_not_pitch() {
    let measure = |pw: f32| {
        let mut engine = clean_engine(Wave::Square);
        engine.set_param(ParamId::Osc1Pulse, pw);
        let out = play(&mut engine, A4, 100.0, 1000.0);
        assert!(all_finite(&out));
        assert!(peak(&out) <= 1.0);
        let hz = estimate_pitch_hz(&out, SAMPLE_RATE).expect("crossings");
        assert!((hz - 440.0).abs() < 4.4, "pw {pw}: {hz} Hz");
        brightness(&out)
    };
    let narrow = measure(0.1);
    let square = measure(0.5);
    assert!(narrow > square * 1.2, "narrow {narrow} square {square}");
    // Osc 2 has its own pulse width.
    let mut engine = clean_engine(Wave::Sine);
    engine.set_param(ParamId::Osc1Mix, 0.0);
    engine.set_param(ParamId::Osc2Wave, Wave::Square.index() as f32);
    engine.set_param(ParamId::Osc2Mix, 1.0);
    engine.set_param(ParamId::Osc2Pulse, 0.15);
    let out = play(&mut engine, A4, 100.0, 500.0);
    assert!(brightness(&out) > square * 1.2);
}

#[test]
fn fm_amount_zero_is_bit_identical_and_fm_adds_sidebands() {
    let render_fm = |amount: f32, ratio: f32| {
        let mut engine = clean_engine(Wave::Sine);
        engine.set_param(ParamId::FmAmount, amount);
        engine.set_param(ParamId::FmRatio, ratio);
        play(&mut engine, A4, 0.0, 500.0)
    };
    let plain = render_fm(0.0, 1.0);
    assert_eq!(plain, render_fm(0.0, 4.0), "amount 0: ratio is irrelevant");
    let mut reference = clean_engine(Wave::Sine);
    assert_eq!(
        plain,
        play(&mut reference, A4, 0.0, 500.0),
        "amount 0 = no FM"
    );

    let fm = render_fm(0.5, 1.0);
    assert!(all_finite(&fm));
    assert!(peak(&fm) <= 1.0);
    assert!(
        brightness(&fm) > brightness(&plain) * 2.0,
        "fm {} plain {}",
        brightness(&fm),
        brightness(&plain)
    );
    // Ratio 1 keeps the fundamental (the modulated wave has extra zero crossings, so use the
    // autocorrelation estimate); ratio 2 gives a different spectrum.
    let hz = autocorr_pitch_hz(&fm[ms_to_frames(100.0)..], 300.0, 600.0);
    assert!((hz - 440.0).abs() < 4.4, "{hz}");
    let fm2 = render_fm(0.5, 2.0);
    assert_ne!(fm, fm2);
    assert!((brightness(&fm2) - brightness(&fm)).abs() > 0.02);
}

#[test]
fn noise_is_broadband_and_pink_is_darker_than_white() {
    let measure = |color: NoiseColor| {
        let mut engine = clean_engine(Wave::Sine);
        engine.set_param(ParamId::Osc1Mix, 0.0);
        engine.set_param(ParamId::NoiseLevel, 1.0);
        engine.set_param(ParamId::NoiseColor, color.index() as f32);
        let out = play(&mut engine, A4, 50.0, 1000.0);
        assert!(all_finite(&out));
        assert!(peak(&out) <= 1.0);
        assert!(rms(&out) > 0.02, "{color:?} rms {}", rms(&out));
        brightness(&out)
    };
    let mut sine = clean_engine(Wave::Sine);
    let sine_bright = brightness(&play(&mut sine, A4, 50.0, 1000.0));
    let white = measure(NoiseColor::White);
    let pink = measure(NoiseColor::Pink);
    assert!(
        white > sine_bright * 10.0,
        "white {white} sine {sine_bright}"
    );
    assert!(pink > sine_bright * 3.0, "pink {pink} sine {sine_bright}");
    assert!(pink < white * 0.6, "pink {pink} white {white}");
    // Noise level 0 (default) adds nothing: bit-identical to the plain sine.
    let mut a = clean_engine(Wave::Sine);
    let mut b = clean_engine(Wave::Sine);
    b.set_param(ParamId::NoiseColor, 1.0);
    assert_eq!(play(&mut a, A4, 0.0, 200.0), play(&mut b, A4, 0.0, 200.0));
}

fn pluck_engine(damping: f32, bright: f32) -> Engine {
    let mut engine = clean_engine(Wave::Saw);
    engine.set_param(ParamId::OscMode, 1.0);
    engine.set_param(ParamId::PluckDamping, damping);
    engine.set_param(ParamId::PluckBright, bright);
    engine.set_param(ParamId::AmpRelease, 4.0);
    engine
}

#[test]
fn pluck_mode_is_in_tune_and_decays() {
    for (note, hz) in [(A3, 220.0_f32), (A4, 440.0)] {
        let mut engine = pluck_engine(0.5, 0.7);
        engine.note_on(note, 1.0);
        let out = left(&render(&mut engine, ms_to_frames(3000.0)));
        assert!(all_finite(&out));
        assert!(peak(&out) <= 1.0);
        let early = &out[..ms_to_frames(100.0)];
        assert!(rms(early) > 0.01, "note {note} early rms {}", rms(early));
        let window = &out[ms_to_frames(100.0)..ms_to_frames(600.0)];
        let est = autocorr_pitch_hz(window, hz * 0.8, hz * 1.25);
        assert!((est - hz).abs() < hz * 0.01, "note {note}: {est} Hz");
        let tail = &out[ms_to_frames(2900.0)..];
        let db = 20.0 * (rms(tail) / rms(early)).log10();
        assert!(db < -40.0, "note {note}: tail at {db} dB");
    }
}

#[test]
fn pluck_damping_shortens_and_brightness_brightens() {
    let decay_ratio = |damping: f32| {
        let mut engine = pluck_engine(damping, 0.7);
        engine.note_on(A3, 1.0);
        let out = left(&render(&mut engine, ms_to_frames(1000.0)));
        rms(&out[ms_to_frames(800.0)..]) / rms(&out[..ms_to_frames(100.0)])
    };
    let dull = decay_ratio(0.9);
    let open = decay_ratio(0.1);
    assert!(dull < open * 0.3, "damping 0.9 {dull} vs 0.1 {open}");

    let bright_of = |bright: f32| {
        let mut engine = pluck_engine(0.5, bright);
        engine.note_on(A3, 1.0);
        let out = left(&render(&mut engine, ms_to_frames(100.0)));
        brightness(&out)
    };
    let dark = bright_of(0.0);
    let raw = bright_of(1.0);
    assert!(dark < raw * 0.6, "dark {dark} raw {raw}");

    // The amp envelope still shapes the string: a short release cuts the ring.
    let mut engine = pluck_engine(0.1, 0.7);
    engine.set_param(ParamId::AmpRelease, 0.01);
    engine.note_on(A3, 1.0);
    let _ = render(&mut engine, ms_to_frames(200.0));
    engine.note_off(A3);
    let _ = render(&mut engine, ms_to_frames(100.0));
    assert!(
        !engine.is_active(),
        "released string is silent after a short release"
    );
}

#[test]
fn filter_types_shape_the_spectrum() {
    // HP at 1 kHz removes a 50 Hz tone.
    let measure = |ty: FilterType, note: u8, cutoff: f32| {
        let mut engine = clean_engine(Wave::Sine);
        engine.set_param(ParamId::FilterType, ty.index() as f32);
        engine.set_param(ParamId::FilterCutoff, cutoff);
        let out = play(&mut engine, note, 300.0, 500.0);
        assert!(all_finite(&out));
        rms(&out)
    };
    let open = measure(FilterType::Lowpass, 31, 20_000.0); // G1, about 49 Hz
    let hp = measure(FilterType::Highpass, 31, 1_000.0);
    assert!(hp < open * 0.01, "hp {hp} open {open}");
    let hp_high = measure(FilterType::Highpass, 105, 1_000.0); // A7, about 3.5 kHz
    let open_high = measure(FilterType::Lowpass, 105, 20_000.0);
    assert!(
        hp_high > open_high * 0.8,
        "hp passes highs: {hp_high} vs {open_high}"
    );

    // BP at 1 kHz passes 1 kHz but attenuates 100 Hz and 10 kHz.
    let bp_mid = measure(FilterType::Bandpass, 83, 1_000.0); // B5, about 988 Hz
    let bp_low = measure(FilterType::Bandpass, 43, 1_000.0); // G2, about 98 Hz
    let bp_high = measure(FilterType::Bandpass, 123, 1_000.0); // about 9.96 kHz
    let open_mid = measure(FilterType::Lowpass, 83, 20_000.0);
    assert!(
        bp_mid > open_mid * 0.8,
        "bp passes centre: {bp_mid} vs {open_mid}"
    );
    assert!(bp_low < bp_mid * 0.3, "bp low {bp_low} mid {bp_mid}");
    assert!(bp_high < bp_mid * 0.3, "bp high {bp_high} mid {bp_mid}");
}

#[test]
fn filter_adsr_is_independent_of_the_amp_adsr() {
    // Instant amp attack, one-second filter attack.
    let slow_filter = |wave: Wave, cutoff: f32| {
        let mut engine = clean_engine(wave);
        engine.set_param(ParamId::FilterCutoff, cutoff);
        engine.set_param(ParamId::FilterEnvAmount, 1.0);
        engine.set_param(ParamId::FilterAttack, 1.0);
        engine.set_param(ParamId::FilterDecay, 0.001);
        engine.set_param(ParamId::FilterSustain, 1.0);
        engine.note_on(A3, 1.0);
        let out = left(&render(&mut engine, ms_to_frames(1200.0)));
        assert!(all_finite(&out));
        out
    };
    // A saw shows the filter opening: brightness keeps rising for the whole second.
    let out = slow_filter(Wave::Saw, 200.0);
    let bright = per_window(&out, ms_to_frames(100.0), brightness);
    assert!(
        bright[9] > bright[0] * 2.0,
        "brightness rises: {:?}",
        bright
    );
    assert!(bright[11] > bright[5], "still rising after 1 s: {bright:?}");
    // A sine far below the cutoff shows the amp envelope alone: flat (after the engine's
    // gain smoothers settle) while the filter is still opening.
    let out = slow_filter(Wave::Sine, 2_000.0);
    let levels = per_window(&out[ms_to_frames(100.0)..], ms_to_frames(100.0), peak);
    let (min, max) = spread(&levels);
    assert!(
        max < min * 1.05,
        "amp level flat while the filter opens: {levels:?}"
    );

    // Filter release longer than amp release: the voice ends with the amp.
    let mut engine = clean_engine(Wave::Saw);
    engine.set_param(ParamId::FilterRelease, 4.0);
    engine.set_param(ParamId::AmpRelease, 0.05);
    engine.note_on(A3, 1.0);
    let _ = render(&mut engine, ms_to_frames(100.0));
    engine.note_off(A3);
    let _ = render(&mut engine, ms_to_frames(300.0));
    assert!(!engine.is_active());
}

#[test]
fn key_tracking_raises_the_cutoff_for_high_notes() {
    let measure = |track: f32, note: u8| {
        let mut engine = clean_engine(Wave::Saw);
        engine.set_param(ParamId::FilterCutoff, 200.0);
        engine.set_param(ParamId::FilterKeyTrack, track);
        let out = play(&mut engine, note, 100.0, 500.0);
        let cutoff = engine
            .voices()
            .voices()
            .iter()
            .find(|v| v.note() == note)
            .map(|v| v.filter().cutoff())
            .unwrap();
        (rms(&out), brightness(&out), cutoff)
    };
    // C6 (1046 Hz): with the cutoff stuck at 200 Hz the note is buried; full tracking lifts
    // the cutoff two octaves (800 Hz) and the note comes through louder and brighter.
    let (l_flat, b_flat, c_flat) = measure(0.0, 84);
    let (l_track, b_track, c_track) = measure(1.0, 84);
    assert!((c_flat - 200.0).abs() < 1.0, "no tracking: {c_flat}");
    assert!((c_track - 800.0).abs() < 2.0, "two octaves up: {c_track}");
    assert!(l_track > l_flat * 2.0, "tracked {l_track} vs flat {l_flat}");
    assert!(b_track > b_flat, "tracked {b_track} vs flat {b_flat}");
    let (_, _, c_below) = measure(1.0, 48);
    assert!((c_below - 100.0).abs() < 1.0, "one octave down: {c_below}");
    let (_, _, c_half) = measure(0.5, 84);
    assert!((c_half - 400.0).abs() < 1.0, "half tracking: {c_half}");
}

#[test]
fn velocity_scales_amplitude_and_cutoff() {
    let peak_at = |vel_to_amp: f32, velocity: f32| {
        let mut engine = clean_engine(Wave::Sine);
        engine.set_param(ParamId::VelToAmp, vel_to_amp);
        let out = play_vel(&mut engine, A4, velocity, 100.0, 300.0);
        assert!(all_finite(&out));
        peak(&out)
    };
    let full = peak_at(1.0, 1.0);
    let half = peak_at(1.0, 0.5);
    assert!(
        (half - full * 0.5).abs() < full * 0.03,
        "half {half} full {full}"
    );
    let insensitive = peak_at(0.0, 0.5);
    assert!(
        (insensitive - full).abs() < full * 0.03,
        "VelToAmp 0: {insensitive} vs {full}"
    );

    // Velocity -> cutoff: a soft note closes the filter; a full one leaves it as set.
    let cutoff_at = |amount: f32, velocity: f32| {
        let mut engine = clean_engine(Wave::Saw);
        engine.set_param(ParamId::FilterCutoff, 1_000.0);
        engine.set_param(ParamId::VelToCutoff, amount);
        let _ = play_vel(&mut engine, A4, velocity, 50.0, 50.0);
        engine.voices().voices()[0].filter().cutoff()
    };
    assert!((cutoff_at(1.0, 1.0) - 1_000.0).abs() < 1.0);
    assert!((cutoff_at(1.0, 0.5) - 500.0).abs() < 2.0);
    assert!((cutoff_at(0.0, 0.5) - 1_000.0).abs() < 1.0);
}

#[test]
fn pitch_envelope_sweeps_down_from_plus_twelve_semitones() {
    let mut engine = clean_engine(Wave::Sine);
    engine.set_param(ParamId::PitchEnvAmount, 12.0);
    engine.set_param(ParamId::PitchEnvDecay, 0.1);
    engine.note_on(A3, 1.0);
    let out = left(&render(&mut engine, ms_to_frames(600.0)));
    assert!(all_finite(&out));
    let early = estimate_pitch_hz(&out[..ms_to_frames(20.0)], SAMPLE_RATE).unwrap();
    let late = estimate_pitch_hz(&out[ms_to_frames(500.0)..], SAMPLE_RATE).unwrap();
    assert!(early > 330.0 && early < 445.0, "early {early} Hz");
    assert!((late - 220.0).abs() < 2.2, "late {late} Hz");
    assert!(early > late * 1.4);

    // Negative amount sweeps up; amount 0 is bit-identical to no envelope.
    let mut engine = clean_engine(Wave::Sine);
    engine.set_param(ParamId::PitchEnvAmount, -12.0);
    engine.note_on(A3, 1.0);
    let out = left(&render(&mut engine, ms_to_frames(600.0)));
    let early = estimate_pitch_hz(&out[..ms_to_frames(20.0)], SAMPLE_RATE).unwrap();
    assert!(early < 170.0, "early {early} Hz");
    let mut a = clean_engine(Wave::Sine);
    let mut b = clean_engine(Wave::Sine);
    b.set_param(ParamId::PitchEnvDecay, 2.0);
    assert_eq!(play(&mut a, A3, 0.0, 200.0), play(&mut b, A3, 0.0, 200.0));
}

#[test]
fn unison_beats_and_keeps_its_level() {
    let render_unison = |voices: f32| {
        let mut engine = clean_engine(Wave::Saw);
        engine.set_param(ParamId::Osc1Unison, voices);
        engine.set_param(ParamId::Osc1Spread, 30.0);
        let out = play(&mut engine, A4, 100.0, 2000.0);
        assert!(all_finite(&out));
        out
    };
    let single = render_unison(1.0);
    let seven = render_unison(7.0);
    let (s_min, s_max) = spread(&per_window(&single, ms_to_frames(20.0), rms));
    let (u_min, u_max) = spread(&per_window(&seven, ms_to_frames(20.0), rms));
    assert!(s_max < s_min * 1.1, "single is steady {s_min}..{s_max}");
    assert!(u_max > u_min * 1.3, "unison beats {u_min}..{u_max}");
    let (r1, r7) = (rms(&single), rms(&seven));
    assert!(r7 > r1 * 0.7 && r7 < r1 * 1.4, "rms 1: {r1}, 7: {r7}");
    assert!(peak(&seven) <= 1.0);
    // Unison 1 is bit-identical to the plain oscillator whatever the spread.
    let mut a = clean_engine(Wave::Saw);
    let mut b = clean_engine(Wave::Saw);
    b.set_param(ParamId::Osc1Spread, 50.0);
    assert_eq!(play(&mut a, A4, 0.0, 200.0), play(&mut b, A4, 0.0, 200.0));
}

#[test]
fn sub_oscillator_adds_energy_an_octave_below() {
    let energy_below = |sub_level: f32| {
        let mut engine = clean_engine(Wave::Sine);
        engine.set_param(ParamId::SubLevel, sub_level);
        let out = play(&mut engine, A4, 100.0, 1000.0);
        assert!(all_finite(&out));
        // The sub at 220 Hz makes every other 440 Hz period differ: the autocorrelation
        // peak moves to the 220 Hz lag.
        let hz = autocorr_pitch_hz(&out, 200.0, 500.0);
        (rms(&out), hz)
    };
    let (plain_rms, plain_hz) = energy_below(0.0);
    let (sub_rms, sub_hz) = energy_below(1.0);
    assert!((plain_hz - 440.0).abs() < 4.4, "{plain_hz}");
    assert!((sub_hz - 220.0).abs() < 2.2, "with sub: {sub_hz}");
    assert!(sub_rms > plain_rms * 1.2, "sub {sub_rms} plain {plain_rms}");
}

#[test]
fn everything_at_once_is_finite_and_limited() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    engine.set_param(ParamId::Osc1Wave, Wave::Square.index() as f32);
    engine.set_param(ParamId::Osc1Pulse, 0.05);
    engine.set_param(ParamId::Osc1Unison, 7.0);
    engine.set_param(ParamId::Osc1Spread, 50.0);
    engine.set_param(ParamId::SubLevel, 1.0);
    engine.set_param(ParamId::NoiseLevel, 1.0);
    engine.set_param(ParamId::NoiseColor, 1.0);
    engine.set_param(ParamId::FmAmount, 1.0);
    engine.set_param(ParamId::FmRatio, 8.0);
    engine.set_param(ParamId::FilterType, 1.0);
    engine.set_param(ParamId::FilterResonance, 1.0);
    engine.set_param(ParamId::FilterEnvAmount, 1.0);
    engine.set_param(ParamId::FilterKeyTrack, 1.0);
    engine.set_param(ParamId::VelToCutoff, 1.0);
    engine.set_param(ParamId::PitchEnvAmount, 24.0);
    engine.set_param(ParamId::PitchEnvDecay, 0.001);
    engine.set_param(ParamId::LfoDepth, 1.0);
    engine.set_param(ParamId::LfoTarget, 0.0);
    for n in 0..16 {
        engine.note_on(24 + n * 6, 1.0);
    }
    let out = render(&mut engine, ms_to_frames(500.0));
    assert!(all_finite(&out));
    assert!(peak(&out) <= 1.0, "peak {}", peak(&out));
    assert!(!is_silent(&out));
    // Switch to pluck mode mid-note, retrigger, and back.
    engine.set_param(ParamId::OscMode, 1.0);
    for n in 0..16 {
        engine.note_on(24 + n * 6, 0.5);
    }
    let out = render(&mut engine, ms_to_frames(500.0));
    assert!(all_finite(&out));
    assert!(peak(&out) <= 1.0);
    engine.set_param(ParamId::OscMode, 0.0);
    engine.all_notes_off();
    let out = render(&mut engine, ms_to_frames(500.0));
    assert!(all_finite(&out));
}
