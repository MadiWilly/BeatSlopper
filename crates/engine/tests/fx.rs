//! Offline tests of the M2 effect chain, directly on `FxChain` at 48 kHz.

use engine::ParamId;
use engine::fx::{FxChain, division_seconds};

const SR: f32 = 48_000.0;

fn samples(seconds: f32) -> usize {
    (SR * seconds).round() as usize
}

/// A chain with every mix at zero (fully dry) and no feedback, ready to enable one effect.
fn dry_chain() -> FxChain {
    let mut fx = FxChain::new(SR);
    fx.set_param(ParamId::DriveMix, 0.0);
    fx.set_param(ParamId::DriveAmount, 0.0);
    fx.set_param(ParamId::DelayMix, 0.0);
    fx.set_param(ParamId::DelayFeedback, 0.0);
    fx.set_param(ParamId::ReverbMix, 0.0);
    fx.reset();
    fx
}

/// Left-channel response to a unit impulse on both channels.
fn impulse_response(fx: &mut FxChain, len: usize) -> Vec<f32> {
    (0..len)
        .map(|i| {
            let x = if i == 0 { 1.0 } else { 0.0 };
            fx.process(x, x).0
        })
        .collect()
}

fn peak_index(buf: &[f32]) -> (usize, f32) {
    buf.iter().enumerate().fold(
        (0, 0.0_f32),
        |a, (i, &v)| if v.abs() > a.1 { (i, v.abs()) } else { a },
    )
}

fn energy(buf: &[f32]) -> f64 {
    buf.iter().map(|&v| f64::from(v) * f64::from(v)).sum()
}

fn rms(buf: &[f32]) -> f32 {
    (energy(buf) / buf.len() as f64).sqrt() as f32
}

fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0_f32, |m, &v| m.max(v.abs()))
}

fn all_finite(buf: &[f32]) -> bool {
    buf.iter().all(|v| v.is_finite())
}

/// Delay-only chain whose echo index we can measure.
fn delay_echo_index(bpm: f32, division: f32) -> (usize, f32) {
    let mut fx = dry_chain();
    fx.set_bpm(bpm);
    fx.set_param(ParamId::DelayTime, division);
    fx.set_param(ParamId::DelayMix, 1.0);
    fx.reset();
    let out = impulse_response(&mut fx, samples(1.5));
    assert_eq!(out[0], 0.0, "wet-only delay must not pass the dry impulse");
    peak_index(&out)
}

#[test]
fn delay_time_matches_bpm_sync() {
    // 120 BPM: 1/16 = 0.125 s, 1/8 = 0.25 s, 1/4 = 0.5 s.
    for (division, expected) in [(0.0, 6_000_usize), (1.0, 12_000), (2.0, 24_000)] {
        let (idx, level) = delay_echo_index(120.0, division);
        assert!(
            idx.abs_diff(expected) <= 1,
            "division {division}: echo at {idx}, expected {expected}"
        );
        assert!((level - 1.0).abs() < 1e-5, "echo level {level}");
    }
    // 90 BPM, 1/8 = 1/3 s.
    let (idx, _) = delay_echo_index(90.0, 1.0);
    assert!(idx.abs_diff(16_000) <= 1, "90 BPM echo at {idx}");

    // The reported time agrees with the measured one.
    let mut fx = FxChain::new(SR);
    fx.set_bpm(90.0);
    fx.set_param(ParamId::DelayTime, 1.0);
    assert_eq!(fx.delay_samples(), 16_000);
    assert!((fx.delay_seconds() - division_seconds(1.0, 90.0)).abs() < 1e-6);
    assert!((division_seconds(3.0, 120.0) - 1.0).abs() < 1e-6);
}

#[test]
fn delay_feedback_produces_a_second_echo_at_half_level() {
    let mut fx = dry_chain();
    fx.set_bpm(120.0);
    fx.set_param(ParamId::DelayTime, 0.0); // 6000 samples
    fx.set_param(ParamId::DelayMix, 1.0);
    fx.set_param(ParamId::DelayFeedback, 0.5);
    fx.reset();
    let out = impulse_response(&mut fx, samples(0.5));
    assert!(all_finite(&out));
    // The feedback loop has a gentle lowpass, so compare the absolute sum in a short window
    // around each echo (the filter's impulse response sums to one).
    let window =
        |centre: usize| -> f32 { out[centre - 10..centre + 100].iter().map(|v| v.abs()).sum() };
    let first = window(6_000);
    let second = window(12_000);
    let third = window(18_000);
    assert!((first - 1.0).abs() < 1e-4, "first echo {first}");
    assert!((second - 0.5).abs() < 0.05, "second echo {second}");
    assert!((third - 0.25).abs() < 0.05, "third echo {third}");
    // Nothing between the echoes.
    assert!(peak(&out[6_200..11_900]) < 1e-3);
}

#[test]
fn delay_time_change_is_click_free() {
    let mut fx = dry_chain();
    fx.set_bpm(120.0);
    fx.set_param(ParamId::DelayTime, 1.0);
    fx.set_param(ParamId::DelayMix, 0.5);
    fx.set_param(ParamId::DelayFeedback, 0.4);
    fx.reset();

    const AMP: f32 = 0.8;
    let mut prev = 0.0_f32;
    let mut max_step = 0.0_f32;
    let mut out = Vec::with_capacity(samples(3.0));
    for i in 0..samples(3.0) {
        match i {
            48_000 => fx.set_param(ParamId::DelayTime, 0.0),
            48_200 => fx.set_param(ParamId::DelayTime, 3.0), // mid-crossfade
            96_000 => fx.set_bpm(70.0),
            _ => {}
        }
        let t = i as f32 / SR;
        let x = AMP * (core::f32::consts::TAU * 220.0 * t).sin();
        let (l, r) = fx.process(x, x);
        assert!(l.is_finite() && r.is_finite(), "sample {i}");
        if i > 0 {
            max_step = max_step.max((l - prev).abs());
        }
        prev = l;
        out.push(l);
    }
    assert!(all_finite(&out));
    assert!(
        max_step < AMP,
        "sample-to-sample jump {max_step} exceeds the input peak {AMP}"
    );
    // Sanity: a hard tap switch on this signal would jump by up to 2 * AMP * mix.
    assert!(
        max_step < 0.2,
        "jump {max_step} is far above a sine's slope"
    );
    assert!(rms(&out[samples(2.5)..]) > 0.1, "signal still flowing");
    assert_eq!(fx.bpm(), 70.0);
    assert!((fx.delay_seconds() - 120.0 / 70.0).abs() < 1e-5);
}

#[test]
fn reverb_tail_grows_with_size_and_dies_out() {
    let after = samples(0.5);
    let len = samples(2.0);
    let tail_for = |size: f32| {
        let mut fx = dry_chain();
        fx.set_param(ParamId::ReverbMix, 1.0);
        fx.set_param(ParamId::ReverbSize, size);
        fx.set_param(ParamId::ReverbDamping, 0.4);
        fx.reset();
        impulse_response(&mut fx, len)
    };
    let small = tail_for(0.1);
    let big = tail_for(0.9);
    assert!(all_finite(&small) && all_finite(&big));
    assert!(peak(&big) > 0.0, "reverb produced no output");
    let (es, eb) = (energy(&small[after..]), energy(&big[after..]));
    assert!(
        eb > es * 10.0,
        "tail energy after 0.5 s: size 0.9 = {eb}, size 0.1 = {es}"
    );

    // Size 1 is the longest tail; at the default damping it must be effectively silent
    // (< 1e-4, about -80 dBFS) after 10 s.
    let size_one = |damping: f32| {
        let mut fx = dry_chain();
        fx.set_param(ParamId::ReverbMix, 1.0);
        fx.set_param(ParamId::ReverbSize, 1.0);
        fx.set_param(ParamId::ReverbDamping, damping);
        fx.reset();
        let out = impulse_response(&mut fx, samples(10.0));
        assert!(all_finite(&out));
        assert!(
            peak(&out[samples(0.1)..samples(1.0)]) > 1e-3,
            "tail present"
        );
        out
    };
    let out = size_one(ParamId::ReverbDamping.default_value());
    let end = peak(&out[samples(9.5)..]);
    assert!(end < 1e-4, "tail still at {end} after 10 s");
    // With no damping at all (comb feedback 0.98 at every frequency) the tail is longer but
    // still finite and clearly decaying, never stuck or growing.
    let out = size_one(0.0);
    let early = peak(&out[samples(1.0)..samples(2.0)]);
    let late = peak(&out[samples(9.0)..]);
    assert!(late < 1e-3, "undamped tail still at {late} after 10 s");
    assert!(
        late < early * 0.05,
        "undamped tail decays: {early} -> {late}"
    );
}

#[test]
fn drive_lowers_crest_factor_and_stays_in_bounds() {
    let n = samples(0.5);
    let shape = |amount: f32| {
        let mut fx = dry_chain();
        fx.set_param(ParamId::DriveMix, 1.0);
        fx.set_param(ParamId::DriveAmount, amount);
        fx.reset();
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f32 / SR;
            let x = (core::f32::consts::TAU * 110.0 * t).sin();
            out.push(fx.process(x, x).0);
        }
        out
    };
    let clean = shape(0.0);
    let hot = shape(1.0);
    assert!(all_finite(&clean) && all_finite(&hot));
    let crest = |b: &[f32]| peak(b) / rms(b);
    let (cc, ch) = (crest(&clean), crest(&hot));
    assert!(ch < cc, "crest factor hot {ch} should be below clean {cc}");
    assert!(ch < 1.1, "fully driven sine is nearly square, crest {ch}");
    assert!(peak(&hot) <= 1.1, "peak {}", peak(&hot));
    assert!(peak(&clean) <= 1.1);
}

#[test]
fn silence_in_silence_out() {
    let mut fx = FxChain::new(SR); // default settings: everything engaged
    fx.set_param(ParamId::DelayFeedback, 0.95);
    fx.set_param(ParamId::ReverbSize, 1.0);
    for i in 0..samples(10.0) {
        let (l, r) = fx.process(0.0, 0.0);
        assert_eq!((l, r), (0.0, 0.0), "sample {i}");
    }
    let mut buf = vec![0.0_f32; 4096];
    fx.process_buffer(&mut buf);
    assert!(buf.iter().all(|&v| v == 0.0));
}

#[test]
fn set_param_ignores_non_fx_ids_and_clamps_fx_ids() {
    let mut fx = FxChain::new(SR);
    let before = fx.clone();
    for id in ParamId::ALL.iter().filter(|id| !id.is_fx()) {
        fx.set_param(*id, 0.123);
        assert_eq!(fx.param(*id), 0.0, "{id:?}");
    }
    assert_eq!(fx, before, "non-FX ids must not touch the chain");

    for id in ParamId::ALL.iter().copied().filter(|id| id.is_fx()) {
        let r = id.range();
        fx.set_param(id, 1.0e9);
        assert_eq!(fx.param(id), *r.end(), "{id:?}");
        fx.set_param(id, -1.0e9);
        assert_eq!(fx.param(id), *r.start(), "{id:?}");
        fx.set_param(id, f32::NAN);
        assert_eq!(fx.param(id), id.default_value(), "{id:?}");
    }
    fx.set_param(ParamId::DelayFeedback, 0.99);
    assert_eq!(fx.param(ParamId::DelayFeedback), 0.95);
    fx.set_param(ParamId::DelayTime, 7.0);
    assert_eq!(fx.param(ParamId::DelayTime), 3.0);
    assert_eq!(fx.delay_samples(), 48_000);
}

#[test]
fn reset_clears_tails() {
    let mut fx = FxChain::new(SR);
    fx.set_param(ParamId::DelayMix, 1.0);
    fx.set_param(ParamId::DelayFeedback, 0.95);
    fx.set_param(ParamId::ReverbMix, 1.0);
    fx.set_param(ParamId::ReverbSize, 1.0);
    fx.reset();
    // Excite everything.
    for i in 0..samples(0.5) {
        let x = if i < 100 { 1.0 } else { 0.0 };
        fx.process(x, x);
    }
    let mut ringing = false;
    for _ in 0..samples(0.5) {
        let (l, r) = fx.process(0.0, 0.0);
        ringing |= l != 0.0 || r != 0.0;
    }
    assert!(ringing, "tails should be audible before reset");
    fx.reset();
    for i in 0..samples(5.0) {
        assert_eq!(fx.process(0.0, 0.0), (0.0, 0.0), "sample {i}");
    }
    // Parameters survive a reset.
    assert_eq!(fx.param(ParamId::DelayFeedback), 0.95);
    assert_eq!(fx.param(ParamId::ReverbSize), 1.0);
}

#[test]
fn process_buffer_handles_odd_lengths() {
    let mut fx = FxChain::new(SR);
    for len in [0, 1, 2, 3, 7, 255, 1001] {
        let mut buf: Vec<f32> = (0..len)
            .map(|i| ((i * 7919) % 200) as f32 / 200.0 - 0.5)
            .collect();
        let last = buf.last().copied();
        fx.process_buffer(&mut buf);
        assert!(all_finite(&buf), "len {len}");
        if len % 2 == 1 {
            assert_eq!(
                buf.last().copied(),
                last,
                "trailing odd sample is left alone"
            );
        }
    }
    // Interleaved and per-frame processing agree.
    let mut a = FxChain::new(SR);
    let mut b = FxChain::new(SR);
    let input: Vec<f32> = (0..2000).map(|i| (i as f32 * 0.37).sin() * 0.5).collect();
    let mut buf = input.clone();
    a.process_buffer(&mut buf);
    let (frames, _) = buf.as_chunks::<2>();
    let (sources, _) = input.as_chunks::<2>();
    for (&[fl, fr], &[sl, sr]) in frames.iter().zip(sources) {
        assert_eq!((fl, fr), b.process(sl, sr));
    }
}

#[test]
fn non_finite_input_never_escapes() {
    let mut fx = FxChain::new(SR);
    fx.set_param(ParamId::DelayMix, 1.0);
    fx.set_param(ParamId::DelayFeedback, 0.9);
    fx.set_param(ParamId::ReverbMix, 1.0);
    let (l, r) = fx.process(f32::NAN, f32::INFINITY);
    assert!(l.is_finite() && r.is_finite());
    for _ in 0..samples(1.0) {
        let (l, r) = fx.process(0.0, 0.0);
        assert!(l.is_finite() && r.is_finite());
    }
}

mod engine_integration {
    //! The chain inside the engine: BPM sync through `SetBpm` and an audible echo.
    use engine::{Message, ParamId, new_engine};

    const SR: f32 = 48_000.0;

    #[test]
    fn engine_delay_follows_bpm_and_division() {
        let (mut handle, mut engine) = new_engine(SR);
        engine.set_param(ParamId::DelayTime, 1.0); // 1/8
        engine.set_bpm(120.0);
        assert_eq!(engine.delay_samples(), 12_000);
        assert!(handle.send(Message::SetBpm(90.0)));
        assert!(handle.send(Message::SetParam(ParamId::DelayTime, 0.0)));
        let mut buf = vec![0.0f32; 64];
        engine.render(&mut buf, 32);
        assert_eq!(engine.bpm(), 90.0);
        assert_eq!(engine.delay_samples(), 8_000); // 1/16 at 90 BPM
    }

    #[test]
    fn engine_renders_an_echo_of_a_note() {
        let (_handle, mut engine) = new_engine(SR);
        engine.set_param(ParamId::DriveMix, 0.0);
        engine.set_param(ParamId::ReverbMix, 0.0);
        engine.set_param(ParamId::DelayMix, 1.0);
        engine.set_param(ParamId::DelayFeedback, 0.0);
        engine.set_param(ParamId::DelayTime, 0.0); // 1/16 = 6000 samples at 120 BPM
        engine.set_bpm(120.0);
        engine.set_param(ParamId::AmpAttack, 0.001);
        engine.set_param(ParamId::AmpRelease, 0.001);
        engine.set_param(ParamId::LfoDepth, 0.0);
        let mut buf = vec![0.0f32; 2 * 24_000];
        // 20 ms note, then silence.
        engine.note_on(69, 1.0);
        engine.render(&mut buf[..2 * 960], 960);
        engine.note_off(69);
        engine.render(&mut buf[2 * 960..], 24_000 - 960);
        let rms = |from: usize, to: usize| {
            let s: f32 = buf[2 * from..2 * to].iter().map(|x| x * x).sum();
            (s / (2 * (to - from)) as f32).sqrt()
        };
        // Fully wet: nothing before the echo, energy around 6000 samples later, silence after.
        assert!(rms(2000, 5500) < 1e-3, "early region should be dry-free");
        assert!(rms(6000, 6960) > 0.02, "echo missing");
        assert!(rms(9000, 23_000) < 1e-3, "no second echo expected");
        assert!(buf.iter().all(|x| x.is_finite()));
    }
}
