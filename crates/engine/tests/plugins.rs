//! Offline tests of the plugins milestone: the master parametric EQ (`fx::eq`), the reverb
//! extensions (`fx::reverb`), the spectrum tap and the project round trip, at 48 kHz.

mod common;

use common::*;
use engine::fx::eq::{self, EqKind, EqParam, EqState, ParametricEq};
use engine::fx::reverb::{self, Reverb, ReverbParam, ReverbState};
use engine::project::FORMAT_VERSION;
use engine::spectrum::{Analyzer, SPECTRUM_SIZE, bin_hz};
use engine::{Message, ParamId, Project};

const SR: f32 = SAMPLE_RATE;

/// A sine at `hz`, `amplitude`, `seconds` long.
fn sine(hz: f32, amplitude: f32, seconds: f32) -> Vec<f32> {
    let n = (SR * seconds) as usize;
    (0..n)
        .map(|i| (2.0 * core::f32::consts::PI * hz * i as f32 / SR).sin() * amplitude)
        .collect()
}

/// Runs `input` (mono, both channels) through `eq` and returns the left output.
fn run_eq(eq: &mut ParametricEq, input: &[f32]) -> Vec<f32> {
    input.iter().map(|&x| eq.process(x, x).0).collect()
}

/// RMS gain in dB of `eq` for a sine at `hz`, measured over the second half of two seconds
/// (the smoothers and the filter transients have long settled).
fn gain_db_at(eq: &mut ParametricEq, hz: f32) -> f32 {
    eq.reset();
    let input = sine(hz, 0.5, 1.0);
    let out = run_eq(eq, &input);
    let half = input.len() / 2;
    20.0 * (rms(&out[half..]) / rms(&input[half..])).log10()
}

fn eq_with(band: usize, kind: EqKind, freq: f32, gain: f32, q: f32) -> ParametricEq {
    let mut eq = ParametricEq::new(SR);
    eq.set(band, EqParam::Kind, kind.index() as f32);
    eq.set(band, EqParam::Freq, freq);
    eq.set(band, EqParam::Gain, gain);
    eq.set(band, EqParam::Q, q);
    eq.set(band, EqParam::On, 1.0);
    eq
}

// (1) A +12 dB peak at 1 kHz raises a 1 kHz sine by about 12 dB and leaves 100 Hz alone.
#[test]
fn peak_boost_raises_its_frequency_and_leaves_others() {
    let mut eq = eq_with(3, EqKind::Peak, 1000.0, 12.0, 1.0);
    let at_1k = gain_db_at(&mut eq, 1000.0);
    assert!((at_1k - 12.0).abs() < 0.5, "1 kHz gain {at_1k} dB");
    let at_100 = gain_db_at(&mut eq, 100.0);
    assert!(at_100.abs() < 0.5, "100 Hz gain {at_100} dB");
    let out = run_eq(&mut eq, &sine(1000.0, 0.5, 0.5));
    assert!(all_finite(&out));
}

// (2) A low cut at 200 Hz attenuates 50 Hz by more than 12 dB.
#[test]
fn low_cut_attenuates_below_its_corner() {
    let mut eq = eq_with(0, EqKind::LowCut, 200.0, 0.0, 0.707);
    let at_50 = gain_db_at(&mut eq, 50.0);
    assert!(at_50 < -12.0, "50 Hz gain {at_50} dB");
    let at_2k = gain_db_at(&mut eq, 2000.0);
    assert!(at_2k.abs() < 0.5, "2 kHz gain {at_2k} dB");
    let mut hc = eq_with(6, EqKind::HighCut, 2000.0, 0.0, 0.707);
    assert!(gain_db_at(&mut hc, 8000.0) < -12.0);
    assert!(gain_db_at(&mut hc, 200.0).abs() < 0.5);
}

// (3) `response_db` matches the rendered gains within 1 dB at several frequencies.
#[test]
fn response_curve_matches_rendered_gains() {
    let mut state = EqState::FLAT;
    state.set(0, EqParam::On, 1.0);
    state.set(0, EqParam::Freq, 60.0);
    state.set(1, EqParam::Gain, 4.0);
    state.set(2, EqParam::Gain, -6.0);
    state.set(2, EqParam::Q, 2.0);
    state.set(4, EqParam::Gain, 5.0);
    state.set(5, EqParam::Gain, -3.0);
    state.set_gain_db(1.5);
    let mut eq = ParametricEq::new(SR);
    eq.set_state(&state);
    assert_eq!(eq.state(), &state);
    for hz in [40.0, 100.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0] {
        let predicted = state.response_db(hz, SR);
        let measured = gain_db_at(&mut eq, hz);
        assert!(
            (predicted - measured).abs() < 1.0,
            "{hz} Hz: predicted {predicted} dB, measured {measured} dB"
        );
    }
}

// (4) Changing a band while rendering never jumps more than twice the signal's own step.
#[test]
fn band_changes_are_smoothed_without_jumps() {
    let mut eq = eq_with(3, EqKind::Peak, 1000.0, 0.0, 1.0);
    let input = sine(300.0, 0.5, 0.6);
    let mut out = Vec::with_capacity(input.len());
    let max_step = {
        // The largest sample-to-sample step of the dry signal itself.
        input
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0_f32, f32::max)
    };
    for (i, &x) in input.iter().enumerate() {
        if i == 6000 {
            eq.set(3, EqParam::Gain, 18.0);
            eq.set(3, EqParam::Freq, 320.0);
            eq.set(3, EqParam::Q, 4.0);
        }
        if i == 15_000 {
            eq.set(3, EqParam::On, 0.0);
        }
        if i == 20_000 {
            eq.set_on(false);
        }
        if i == 24_000 {
            eq.set_on(true);
            eq.set_gain_db(-12.0);
        }
        out.push(eq.process(x, x).0);
    }
    assert!(all_finite(&out));
    // The boosted tone legitimately has larger steps than the dry one: the bound is twice
    // the largest step the output takes in its loudest steady state.
    let steady_step = out[9000..14_000]
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0_f32, f32::max);
    let bound = 2.0 * max_step.max(steady_step);
    let worst = out
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        worst <= bound,
        "largest step {worst} vs dry step {max_step}, steady step {steady_step}"
    );
    // The boost did arrive: the 300 Hz tone near the band is louder after the change.
    assert!(rms(&out[9000..14_000]) > rms(&input[9000..14_000]) * 1.5);
}

// (5) A flat EQ is bit-transparent within 1e-5 and everything stays finite.
#[test]
fn flat_eq_is_transparent_and_finite() {
    let mut eq = ParametricEq::new(SR);
    let input = sine(440.0, 0.9, 0.5);
    let out = run_eq(&mut eq, &input);
    for (i, (a, b)) in input.iter().zip(&out).enumerate() {
        assert!((a - b).abs() <= 1e-5, "sample {i}: {a} vs {b}");
    }
    assert!(all_finite(&out));
    // Every preset renders finite output at every band and never blows up on noise.
    let mut x = 0x1234_5678_u32;
    let noise: Vec<f32> = (0..24_000)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            (x as f32 / u32::MAX as f32) * 2.0 - 1.0
        })
        .collect();
    for preset in eq::PRESETS {
        eq.set_state(&preset.state);
        let out = run_eq(&mut eq, &noise);
        assert!(all_finite(&out), "{}", preset.name);
        assert!(peak(&out) < 20.0, "{} peak {}", preset.name, peak(&out));
    }
}

// (6) The engine runs the EQ on the master: messages route and a high cut dulls the synth.
#[test]
fn engine_eq_messages_route_and_shape_the_master() {
    let (mut handle, mut engine) = engine::new_engine(SR);
    handle.send(Message::SetEq {
        band: 6,
        param: EqParam::Freq,
        value: 150.0,
    });
    handle.send(Message::SetEq {
        band: 6,
        param: EqParam::On,
        value: 1.0,
    });
    handle.send(Message::SetEqGain(-3.0));
    engine.process_messages();
    assert!(engine.eq().bands[6].on);
    assert_eq!(engine.eq().bands[6].freq, 150.0);
    assert_eq!(engine.eq().gain_db, -3.0);

    // The same note with and without the EQ (the limiter off so levels compare directly).
    engine.set_param(ParamId::LimiterOn, 0.0);
    engine.set_param(ParamId::ReverbMix, 0.0);
    engine.set_param(ParamId::DelayMix, 0.0);
    engine.note_on(81, 1.0); // A5 = 880 Hz, well above the 150 Hz cut
    let cut = render(&mut engine, ms_to_frames(500.0));
    engine.all_notes_off();
    render(&mut engine, ms_to_frames(1500.0));
    handle.send(Message::SetEqOn(false));
    engine.process_messages();
    engine.note_on(81, 1.0);
    let flat = render(&mut engine, ms_to_frames(500.0));
    assert!(all_finite(&cut) && all_finite(&flat));
    let (cut_rms, flat_rms) = (rms(&cut[4800..]), rms(&flat[4800..]));
    assert!(
        cut_rms < flat_rms * 0.25,
        "high cut at 150 Hz: {cut_rms} vs flat {flat_rms}"
    );
    // The spectrum tap saw the output: the ring is non-silent and finite.
    let reader = handle.spectrum();
    let mut snap = [0.0_f32; SPECTRUM_SIZE];
    reader.snapshot(&mut snap);
    assert!(all_finite(&snap));
    assert!(peak(&snap) > 0.01);
    let mut an = Analyzer::new();
    let shown = an.analyse(&snap, 1.0 / 30.0);
    let (peak_bin, _) = shown
        .iter()
        .enumerate()
        .skip(1)
        .fold((0, f32::MIN), |a, (i, &v)| if v > a.1 { (i, v) } else { a });
    let peak_hz = bin_hz(peak_bin, SR);
    assert!(
        (peak_hz - 880.0).abs() < 60.0,
        "analyser peak at {peak_hz} Hz"
    );
}

// (7) Reverb: pre-delay 100 ms moves the first wet sample by 4800 samples.
#[test]
fn pre_delay_shifts_the_wet_onset() {
    fn first_wet(pre_delay_ms: f32) -> usize {
        let mut r = Reverb::new(SR);
        r.apply_state(&ReverbState::new(
            0.5,
            0.3,
            pre_delay_ms,
            1.0,
            20.0,
            20_000.0,
            1.0,
        ));
        r.reset();
        (0..20_000)
            .map(|i| {
                r.process(
                    if i == 0 { 1.0 } else { 0.0 },
                    if i == 0 { 1.0 } else { 0.0 },
                )
                .0
            })
            .position(|v| v.abs() > 1e-9)
            .expect("the reverb produces a tail")
    }
    let dry = first_wet(0.0);
    let delayed = first_wet(100.0);
    assert_eq!(
        delayed - dry,
        4800,
        "dry onset {dry}, delayed onset {delayed}"
    );
}

// (8) Width 0 makes the two channels identical; width 1 keeps them apart.
#[test]
fn width_zero_is_mono() {
    let mut mono = Reverb::new(SR);
    mono.apply_state(&ReverbState::new(0.7, 0.3, 0.0, 0.0, 20.0, 20_000.0, 1.0));
    mono.reset();
    let mut wide = Reverb::new(SR);
    wide.apply_state(&ReverbState::new(0.7, 0.3, 0.0, 1.0, 20.0, 20_000.0, 1.0));
    wide.reset();
    let mut differ = 0;
    for i in 0..24_000 {
        let x = if i == 0 { 1.0 } else { 0.0 };
        let (l, r) = mono.process(x, x);
        assert!((l - r).abs() < 1e-6, "sample {i}: {l} vs {r}");
        let (wl, wr) = wide.process(x, x);
        if (wl - wr).abs() > 1e-4 {
            differ += 1;
        }
    }
    assert!(
        differ > 1000,
        "full width keeps the channels apart ({differ})"
    );
}

/// Energy of `buf` near `hz` (Goertzel over the whole buffer), normalised by its length.
fn tone_energy(buf: &[f32], hz: f32) -> f64 {
    let w = 2.0 * std::f64::consts::PI * f64::from(hz) / f64::from(SR);
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0_f64, 0.0_f64);
    for &x in buf {
        let s0 = f64::from(x) + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (s1 * s1 + s2 * s2 - coeff * s1 * s2) / buf.len() as f64
}

// (9) The wet low cut removes a 50 Hz component from the tail.
#[test]
fn wet_low_cut_removes_rumble() {
    fn tail_with_low_cut(low_cut: f32) -> Vec<f32> {
        let mut r = Reverb::new(SR);
        r.apply_state(&ReverbState::new(
            0.8, 0.0, 0.0, 1.0, low_cut, 20_000.0, 1.0,
        ));
        r.reset();
        // Excite the reverb with a 50 Hz burst, then listen to the tail.
        let burst = sine(50.0, 0.8, 0.4);
        for &x in &burst {
            r.process(x, x);
        }
        (0..48_000).map(|_| r.process(0.0, 0.0).0).collect()
    }
    let open = tail_with_low_cut(20.0);
    let cut = tail_with_low_cut(400.0);
    assert!(all_finite(&open) && all_finite(&cut));
    let (e_open, e_cut) = (tone_energy(&open, 50.0), tone_energy(&cut, 50.0));
    assert!(
        e_open > 1e-9,
        "the open reverb carries the 50 Hz tail ({e_open})"
    );
    assert!(
        e_cut < e_open * 0.01,
        "low cut at 400 Hz leaves {e_cut} of {e_open} at 50 Hz"
    );
    // The high cut works the same way on a 6 kHz tail.
    let mut bright = Reverb::new(SR);
    bright.apply_state(&ReverbState::new(0.8, 0.0, 0.0, 1.0, 20.0, 20_000.0, 1.0));
    let mut dark = Reverb::new(SR);
    dark.apply_state(&ReverbState::new(0.8, 0.0, 0.0, 1.0, 20.0, 1000.0, 1.0));
    for x in sine(6000.0, 0.8, 0.4) {
        bright.process(x, x);
        dark.process(x, x);
    }
    let tb: Vec<f32> = (0..48_000).map(|_| bright.process(0.0, 0.0).0).collect();
    let td: Vec<f32> = (0..48_000).map(|_| dark.process(0.0, 0.0).0).collect();
    assert!(tone_energy(&td, 6000.0) < tone_energy(&tb, 6000.0) * 0.01);
}

// (10) The impulse envelope decreases after its peak and RT60 grows with size.
#[test]
fn impulse_envelope_decays_and_rt60_grows_with_size() {
    let small = ReverbState::new(0.2, 0.3, 0.0, 1.0, 20.0, 20_000.0, 1.0);
    let big = ReverbState::new(0.9, 0.3, 0.0, 1.0, 20.0, 20_000.0, 1.0);
    assert!(big.decay_time_60db() > small.decay_time_60db() * 2.0);
    let env = big.impulse_envelope(SR, 2.0);
    assert_eq!(env.len(), 200);
    assert!(env.iter().all(|v| v.is_finite() && *v >= 0.0));
    let (peak_i, peak_v) = env
        .iter()
        .enumerate()
        .fold((0, 0.0_f32), |a, (i, &v)| if v > a.1 { (i, v) } else { a });
    assert!(peak_v > 0.0 && peak_i < 20, "peak at bin {peak_i}");
    // Monotonic-ish: a 50 ms moving average never rises by more than 5 % after the peak.
    let avg: Vec<f32> = env[peak_i..]
        .windows(5)
        .map(|w| w.iter().sum::<f32>() / 5.0)
        .collect();
    for (i, w) in avg.windows(1).enumerate().skip(1) {
        assert!(
            w[0] <= avg[i - 1] * 1.05 + 1e-9,
            "envelope rises at bin {}: {} -> {}",
            peak_i + i,
            avg[i - 1],
            w[0]
        );
    }
    assert!(avg[avg.len() - 1] < peak_v * 0.5, "the tail has decayed");
    // A short request still yields at least one bin; junk is tolerated.
    assert!(!small.impulse_envelope(SR, 0.0).is_empty());
    assert!(!small.impulse_envelope(f32::NAN, f32::NAN).is_empty());
}

// (11) The engine keeps the two reverb views in sync and routes `SetReverb`.
#[test]
fn engine_reverb_views_agree() {
    let (mut handle, mut engine) = engine::new_engine(SR);
    handle.send(Message::SetReverb {
        param: ReverbParam::Size,
        value: 0.9,
    });
    handle.send(Message::SetReverb {
        param: ReverbParam::PreDelay,
        value: 50.0,
    });
    handle.send(Message::SetParam(ParamId::ReverbMix, 0.7));
    engine.process_messages();
    assert_eq!(engine.reverb().size, 0.9);
    assert_eq!(engine.param(ParamId::ReverbSize), 0.9);
    assert_eq!(engine.reverb().mix, 0.7);
    assert_eq!(engine.param(ParamId::ReverbMix), 0.7);
    assert_eq!(engine.reverb().pre_delay_ms, 50.0);
    assert_eq!(engine.fx().reverb().pre_delay_ms(), 50.0);
    // Size reaches the DSP through the chain's smoother once audio runs.
    render(&mut engine, ms_to_frames(100.0));
    assert!((engine.fx().reverb().size() - 0.9).abs() < 1e-3);
    assert_eq!(engine.fx().reverb().state().pre_delay_ms, 50.0);
    // Clamping goes through the state.
    assert_eq!(engine.set_reverb(ReverbParam::Width, 7.0), 1.0);
    assert_eq!(engine.set_reverb(ReverbParam::LowCut, f32::NAN), 20.0);
    // The reverb level meter reports the wet return while a note rings.
    engine.set_param(ParamId::ReverbMix, 0.8);
    engine.note_on(60, 1.0);
    render(&mut engine, ms_to_frames(200.0));
    assert!(handle.reverb_level() > 0.0);
    assert!(handle.reverb_level().is_finite());
}

// (12) Project round trip with both plugin states; version-5 files migrate.
#[test]
fn project_round_trips_plugin_state_and_migrates_v5() {
    let mut p = Project::default();
    assert_eq!(p.eq, EqState::FLAT);
    assert_eq!(p.reverb, ReverbState::DEFAULT);
    p.eq = eq::PRESETS[3].state;
    p.reverb = reverb::PRESETS[4].state;
    p.sync_reverb_params();
    let text = p.to_json_pretty();
    assert!(text.contains("\"eq\""));
    assert!(text.contains("\"pre_delay_ms\""));
    let back = Project::from_json(&text).expect("round trip");
    assert_eq!(back, p);
    assert_eq!(
        back.param(ParamId::ReverbSize),
        reverb::PRESETS[4].state.size
    );

    // Apply to an engine and read it back.
    let (_h, mut engine) = engine::new_engine(SR);
    p.apply_to(&mut engine);
    assert_eq!(engine.eq(), &p.eq);
    assert_eq!(engine.reverb(), &p.reverb);
    let again = Project::from_engine(&engine, 0, 4, 0.8);
    assert_eq!(again.eq, p.eq);
    assert_eq!(again.reverb, p.reverb);

    // A version-5 file (no eq / reverb) loads flat with the reverb built from its params.
    let mut v5 = serde_json::to_value(Project::default()).unwrap();
    v5["version"] = serde_json::json!(5);
    let obj = v5.as_object_mut().unwrap();
    obj.remove("eq");
    obj.remove("reverb");
    v5["params"]["reverb_size"] = serde_json::json!(0.33);
    v5["params"]["reverb_mix"] = serde_json::json!(0.55);
    let old = Project::from_json(&v5.to_string()).expect("v5 loads");
    assert_eq!(old.version, FORMAT_VERSION);
    assert_eq!(old.eq, EqState::FLAT);
    assert_eq!(old.reverb.size, 0.33);
    assert_eq!(old.reverb.mix, 0.55);
    assert_eq!(old.reverb.pre_delay_ms, 0.0);
    // In a version-6 file the reverb state wins over a stale param.
    let mut v6 = serde_json::to_value(&p).unwrap();
    v6["params"]["reverb_size"] = serde_json::json!(0.01);
    let fresh = Project::from_json(&v6.to_string()).unwrap();
    assert_eq!(fresh.param(ParamId::ReverbSize), p.reverb.size);
}
