//! Offline tests of the M8 effect modules (lo-fi, chorus, sidechain ducker) and the
//! `FxChain::process_synth_insert` wiring, at 48 kHz, with no engine involved.
//!
//! The binary installs a counting global allocator (per thread, so parallel tests do not
//! disturb each other) to prove the per-sample paths never allocate after construction.

mod common;

use core::cell::Cell;
use core::f32::consts::TAU;
use std::alloc::{GlobalAlloc, Layout, System};

use common::{all_finite, brightness, estimate_pitch_hz, peak, per_window, rms};
use engine::ParamId;
use engine::SynthInsert;
use engine::drums::noise::Noise;
use engine::fx::{Chorus, Ducker, FxChain, Lofi};

const SR: f32 = 48_000.0;

// --- allocation counting -------------------------------------------------------------------

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

/// Counts every allocation and reallocation made by the calling thread.
struct CountingAllocator;

impl CountingAllocator {
    fn bump() {
        // `try_with` so a thread tearing down its TLS cannot panic inside the allocator.
        let _ = ALLOCATIONS.try_with(|c| c.set(c.get() + 1));
    }
}

// SAFETY: every method forwards to `System` unchanged; the counter is a plain TLS `Cell`
// that itself never allocates.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        Self::bump();
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        Self::bump();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        Self::bump();
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn allocations() -> usize {
    ALLOCATIONS.with(Cell::get)
}

// --- helpers --------------------------------------------------------------------------------

fn samples(seconds: f32) -> usize {
    (SR * seconds).round() as usize
}

fn sine(n: usize, hz: f32, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| (i as f32 * TAU * hz / SR).sin() * amp)
        .collect()
}

/// Naive (aliased) sawtooth: plenty of highs, which is what the tone/rate tests want.
fn saw(n: usize, hz: f32, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let phase = (i as f32 * hz / SR).fract();
            (2.0 * phase - 1.0) * amp
        })
        .collect()
}

fn white(n: usize, amp: f32) -> Vec<f32> {
    let mut rng = Noise::new(0xC0FF_EE11);
    (0..n).map(|_| rng.next_f32() * amp).collect()
}

/// Amplitude of the `hz` component of `buf` (Goertzel).
fn amplitude_at(buf: &[f32], hz: f32) -> f32 {
    let w = f64::from(TAU * hz / SR);
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0_f64, 0.0_f64);
    for &x in buf {
        let s0 = f64::from(x) + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    let power = (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0);
    (2.0 * power.sqrt() / buf.len() as f64) as f32
}

/// Pearson correlation of two equally long buffers.
fn correlation(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len() as f64;
    let ma = a.iter().map(|&v| f64::from(v)).sum::<f64>() / n;
    let mb = b.iter().map(|&v| f64::from(v)).sum::<f64>() / n;
    let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
    for (&x, &y) in a.iter().zip(b) {
        let (dx, dy) = (f64::from(x) - ma, f64::from(y) - mb);
        sab += dx * dy;
        saa += dx * dx;
        sbb += dy * dy;
    }
    (sab / (saa * sbb).sqrt()) as f32
}

/// A lo-fi stage fully wet with every other control at its (transparent) default.
fn wet_lofi() -> Lofi {
    let mut l = Lofi::new(SR);
    l.set_param(ParamId::LofiMix, 1.0);
    l.reset();
    l
}

fn run_lofi(l: &mut Lofi, input: &[f32]) -> Vec<f32> {
    input.iter().map(|&x| l.process(x)).collect()
}

/// A per-track insert (M10: `SynthInsert`, formerly part of `FxChain`) with every mix at
/// zero and the ducker off.
fn dry_insert() -> SynthInsert {
    let mut ins = SynthInsert::new(SR);
    for id in [
        ParamId::DriveMix,
        ParamId::DriveAmount,
        ParamId::LofiMix,
        ParamId::ChorusMix,
        ParamId::SidechainAmount,
    ] {
        ins.set_param(id, 0.0);
    }
    ins.set_param(ParamId::SidechainRelease, 0.25);
    ins.reset();
    ins
}

/// Sets the ducker through the two sidechain ids.
fn set_ducker(ins: &mut SynthInsert, amount: f32, release: f32) {
    ins.set_param(ParamId::SidechainAmount, amount);
    ins.set_param(ParamId::SidechainRelease, release);
}

// --- lo-fi ------------------------------------------------------------------------------------

#[test]
fn bit_depth_four_adds_quantisation_noise_and_sixteen_is_transparent() {
    let input = sine(samples(0.5), 440.0, 0.5);
    let error = |bits: f32| {
        let mut l = wet_lofi();
        l.set_param(ParamId::LofiBits, bits);
        l.reset();
        let out = run_lofi(&mut l, &input);
        assert!(all_finite(&out));
        let diff: Vec<f32> = out.iter().zip(&input).map(|(y, x)| y - x).collect();
        rms(&diff)
    };
    let coarse = error(4.0);
    let fine = error(16.0);
    assert!(coarse > 1e-3, "4-bit error rms {coarse}");
    assert!(fine < 1e-3, "16-bit error rms {fine}");
    assert!(coarse > 10.0 * fine);
}

#[test]
fn rate_reduction_removes_highs_and_keeps_the_fundamental() {
    let n = samples(1.0);
    let input: Vec<f32> = sine(n, 440.0, 0.5)
        .iter()
        .zip(sine(n, 15_000.0, 0.25))
        .map(|(a, b)| a + b)
        .collect();
    let mut l = wet_lofi();
    l.set_param(ParamId::LofiRate, 8.0);
    l.reset();
    let out = run_lofi(&mut l, &input);
    assert!(all_finite(&out));
    let (fund_in, high_in) = (amplitude_at(&input, 440.0), amplitude_at(&input, 15_000.0));
    let (fund_out, high_out) = (amplitude_at(&out, 440.0), amplitude_at(&out, 15_000.0));
    assert!((fund_in - 0.5).abs() < 0.01 && (high_in - 0.25).abs() < 0.01);
    assert!(
        fund_out > 0.8 * fund_in,
        "fundamental {fund_out} vs {fund_in}"
    );
    assert!(high_out < 0.5 * high_in, "15 kHz {high_out} vs {high_in}");
    // The highs-to-fundamental ratio (spectral brightness) drops.
    assert!(high_out / fund_out < 0.5 * (high_in / fund_in));
    assert_eq!(l.param(ParamId::LofiRate), 8.0);
}

#[test]
fn wobble_modulates_pitch_and_zero_is_transparent() {
    let input = sine(samples(3.0), 440.0, 0.5);
    let mut l = wet_lofi();
    l.set_param(ParamId::LofiWobble, 1.0);
    l.reset();
    let out = run_lofi(&mut l, &input);
    assert!(all_finite(&out));
    let pitches: Vec<f32> = per_window(&out, samples(0.05), |w| {
        estimate_pitch_hz(w, SR).expect("a windowed sine has crossings")
    });
    let lo = pitches.iter().copied().fold(f32::INFINITY, f32::min);
    let hi = pitches.iter().copied().fold(0.0_f32, f32::max);
    let spread = hi / lo - 1.0;
    assert!(spread > 0.005, "pitch spread {spread} ({lo}..{hi})");
    // Never more than the +-20 cent design range (plus estimator slack).
    assert!(spread < 0.03, "pitch spread {spread}");
    // Wobble 0: the delay line is read at distance zero, so only the 16-bit crusher remains.
    let mut l = wet_lofi();
    l.set_param(ParamId::LofiWobble, 0.0);
    l.reset();
    let out = run_lofi(&mut l, &input);
    for (i, (y, x)) in out.iter().zip(&input).enumerate() {
        assert!((y - x).abs() <= 1.0 / 32_768.0, "sample {i}: {y} vs {x}");
    }
}

#[test]
fn vinyl_noise_is_about_minus_forty_dbfs_with_sparse_crackle() {
    let n = samples(2.0);
    let mut l = wet_lofi();
    l.set_param(ParamId::LofiNoise, 1.0);
    l.reset();
    let out: Vec<f32> = (0..n).map(|_| l.process(0.0)).collect();
    assert!(all_finite(&out));
    let level = rms(&out);
    let dbfs = 20.0 * level.log10();
    eprintln!("noise bed {dbfs} dBFS, peak {}", peak(&out));
    assert!((-46.0..=-34.0).contains(&dbfs), "noise bed {dbfs} dBFS");
    // Crackle: isolated peaks well above the hiss, but rare.
    let floor = 4.0 * level;
    let loud = out.iter().filter(|v| v.abs() > floor).count();
    let fraction = loud as f32 / n as f32;
    assert!(peak(&out) > 6.0 * level, "peak {} rms {level}", peak(&out));
    assert!(
        fraction > 0.0005 && fraction < 0.03,
        "{loud} loud samples ({fraction})"
    );
    // Level scales with the control, and 0 is exactly silent.
    let mut l = wet_lofi();
    l.set_param(ParamId::LofiNoise, 0.25);
    l.reset();
    let quiet: Vec<f32> = (0..n).map(|_| l.process(0.0)).collect();
    assert!(rms(&quiet) < 0.5 * level);
    let mut l = wet_lofi();
    l.set_param(ParamId::LofiNoise, 0.0);
    l.reset();
    for _ in 0..n {
        assert_eq!(l.process(0.0), 0.0);
    }
}

#[test]
fn tone_two_khz_darkens() {
    let input = saw(samples(0.5), 110.0, 0.5);
    let bright = |hz: f32| {
        let mut l = wet_lofi();
        l.set_param(ParamId::LofiTone, hz);
        l.reset();
        let out = run_lofi(&mut l, &input);
        assert!(all_finite(&out));
        brightness(&out)
    };
    let open = bright(20_000.0);
    let dark = bright(2_000.0);
    assert!(dark < 0.7 * open, "tone 2 kHz {dark} vs open {open}");
    assert!(
        (open - brightness(&input)).abs() < 1e-3,
        "20 kHz is bypassed"
    );
}

#[test]
fn lofi_mix_zero_is_bit_transparent_in_the_chain() {
    let mut fx = dry_insert();
    for (id, v) in [
        (ParamId::LofiBits, 4.0),
        (ParamId::LofiRate, 16.0),
        (ParamId::LofiWobble, 1.0),
        (ParamId::LofiNoise, 1.0),
        (ParamId::LofiTone, 1_000.0),
    ] {
        fx.set_param(id, v);
    }
    fx.reset();
    assert_eq!(fx.lofi().param(ParamId::LofiBits), 4.0);
    assert_eq!(fx.param(ParamId::LofiTone), 1_000.0);
    for (i, &x) in saw(samples(0.5), 220.0, 0.6).iter().enumerate() {
        assert_eq!(fx.process(x), (x, x), "sample {i}");
    }
}

// --- chorus -----------------------------------------------------------------------------------

#[test]
fn chorus_widens_at_full_mix_and_is_identity_at_zero() {
    let input = white(samples(1.0), 0.5);
    let mut c = Chorus::new(SR);
    c.set_param(ParamId::ChorusMix, 1.0);
    c.set_param(ParamId::ChorusDepth, 1.0);
    c.reset();
    let (mut l, mut r) = (
        Vec::with_capacity(input.len()),
        Vec::with_capacity(input.len()),
    );
    for &x in &input {
        let (a, b) = c.process(x);
        l.push(a);
        r.push(b);
    }
    assert!(all_finite(&l) && all_finite(&r));
    assert!(l != r);
    let corr = correlation(&l, &r);
    assert!(corr < 0.95, "L/R correlation {corr}");
    // Both sides keep the dry signal (an equal blend), so they stay related to the input.
    assert!(correlation(&l, &input) > 0.5 && correlation(&r, &input) > 0.5);
    // Also with a tonal input: pitch-modulated taps make the two sides differ.
    let tone = saw(samples(1.0), 110.0, 0.5);
    c.reset();
    let mut differ = 0;
    for &x in &tone {
        let (a, b) = c.process(x);
        differ += usize::from(a != b);
    }
    assert!(differ > tone.len() / 2);
    // Mix 0 (the default): exactly (x, x).
    let mut c = Chorus::new(SR);
    c.set_param(ParamId::ChorusDepth, 1.0);
    c.reset();
    for &x in &input {
        assert_eq!(c.process(x), (x, x));
    }
}

// --- ducker -----------------------------------------------------------------------------------

#[test]
fn ducker_dips_within_two_ms_and_recovers_by_release() {
    let mut d = Ducker::new(SR);
    d.set(0.7, 0.2);
    assert_eq!(d.tick(), 1.0);
    d.trigger();
    let attack: Vec<f32> = (0..samples(0.002)).map(|_| d.tick()).collect();
    assert!(attack.windows(2).all(|w| w[1] <= w[0]), "monotonic attack");
    let bottom = *attack.last().unwrap();
    assert!((bottom - 0.3).abs() < 1e-4, "bottom {bottom}");
    let release: Vec<f32> = (0..samples(0.2)).map(|_| d.tick()).collect();
    assert!(
        release.windows(2).all(|w| w[1] >= w[0]),
        "monotonic release"
    );
    let at_release = *release.last().unwrap();
    assert!(
        at_release > 0.95 && at_release < 0.98,
        "at release {at_release}"
    );
    assert!(d.is_active());
    // Retrigger from a partly recovered state: back to the floor, again within 2 ms.
    d.trigger();
    let again: Vec<f32> = (0..samples(0.002)).map(|_| d.tick()).collect();
    assert!(again[0] < at_release);
    assert!((again.last().unwrap() - 0.3).abs() < 1e-4);
    for _ in 0..samples(1.0) {
        d.tick();
    }
    assert!(!d.is_active());
    assert_eq!(d.gain(), 1.0);
}

#[test]
fn chain_ducker_pumps_the_synth_insert() {
    let mut fx = dry_insert();
    set_ducker(&mut fx, 0.6, 0.25);
    assert_eq!(fx.ducker().amount(), 0.6);
    assert_eq!(fx.process(0.5), (0.5, 0.5));
    fx.trigger_ducker();
    let mut out = Vec::new();
    for _ in 0..samples(0.25) {
        let (l, r) = fx.process(0.5);
        assert_eq!(l, r);
        out.push(l);
    }
    let bottom = out[samples(0.002) - 1];
    assert!((bottom - 0.2).abs() < 1e-4, "bottom {bottom}");
    assert!(out[..samples(0.002)].iter().all(|&v| v >= bottom - 1e-6));
    let end = *out.last().unwrap();
    assert!(end > 0.95 * 0.5, "at release {end}");
    // Retrigger resets.
    fx.trigger_ducker();
    let (l, _) = fx.process(0.5);
    assert!(l < end);
    assert!(fx.ducker().is_active());
}

// --- insert wiring ----------------------------------------------------------------------------

#[test]
fn synth_insert_with_all_mixes_zero_is_the_drive_output_on_both_channels() {
    let mut fx = dry_insert();
    fx.set_param(ParamId::DriveAmount, 1.0);
    fx.set_param(ParamId::DriveMix, 1.0);
    fx.reset();
    let drive = *fx.drive();
    assert_eq!(drive.amount(), 1.0);
    for (i, &x) in saw(samples(0.25), 220.0, 0.8).iter().enumerate() {
        let expected = drive.process(x);
        assert_eq!(fx.process(x), (expected, expected), "sample {i}");
    }
    // The two insert stages are routed through `set_param` (and nothing else changes).
    fx.set_param(ParamId::ChorusRate, 2.5);
    fx.set_param(ParamId::LofiBits, 6.0);
    assert_eq!(fx.chorus().param(ParamId::ChorusRate), 2.5);
    assert_eq!(fx.lofi().param(ParamId::LofiBits), 6.0);
    assert_eq!(fx.param(ParamId::ChorusRate), 2.5);
    assert_eq!(fx.param(ParamId::LofiBits), 6.0);
}

#[test]
fn everything_is_finite_under_extreme_settings_and_junk_input() {
    let mut fx = FxChain::new(SR);
    let mut ins = SynthInsert::new(SR);
    for id in ParamId::ALL.iter().filter(|id| id.is_fx()) {
        fx.set_param(*id, *id.range().end());
    }
    for id in ParamId::ALL.iter().filter(|id| id.is_synth_insert()) {
        ins.set_param(*id, *id.range().end());
    }
    ins.set_param(ParamId::LofiBits, 4.0);
    ins.set_param(ParamId::LofiTone, 1_000.0);
    set_ducker(&mut ins, 1.0, 0.05);
    fx.reset();
    ins.reset();
    let mut rng = Noise::new(7);
    for i in 0..samples(2.0) {
        if i % 4_800 == 0 {
            ins.trigger_ducker();
        }
        let x = match i % 1_000 {
            0 => f32::NAN,
            1 => f32::INFINITY,
            2 => f32::NEG_INFINITY,
            _ => rng.next_f32() * 4.0,
        };
        let (l, r) = ins.process(x);
        assert!(l.is_finite() && r.is_finite(), "sample {i}: {l} {r}");
        let (bl, br) = fx.process_bus((l, r), (l, r), (l, r));
        assert!(bl.is_finite() && br.is_finite(), "sample {i}");
    }
}

#[test]
fn processing_never_allocates_after_construction() {
    let mut fx = FxChain::new(SR);
    let mut ins = SynthInsert::new(SR);
    let mut lofi = Lofi::new(SR);
    let mut chorus = Chorus::new(SR);
    let mut ducker = Ducker::new(SR);
    for (id, v) in [
        (ParamId::LofiMix, 1.0),
        (ParamId::LofiWobble, 1.0),
        (ParamId::LofiNoise, 1.0),
        (ParamId::LofiRate, 4.0),
        (ParamId::LofiBits, 8.0),
        (ParamId::ChorusMix, 1.0),
        (ParamId::ChorusDepth, 1.0),
        (ParamId::DriveMix, 1.0),
        (ParamId::DelayMix, 0.5),
        (ParamId::ReverbMix, 0.5),
    ] {
        fx.set_param(id, v);
        ins.set_param(id, v);
        lofi.set_param(id, v);
        chorus.set_param(id, v);
    }
    set_ducker(&mut ins, 0.8, 0.1);
    ducker.set(0.8, 0.1);
    fx.reset();
    ins.reset();
    let input = white(samples(1.0), 0.5);

    let before = allocations();
    let mut acc = 0.0_f32;
    for (i, &x) in input.iter().enumerate() {
        if i % 12_000 == 0 {
            ins.trigger_ducker();
            ducker.trigger();
            ins.set_param(ParamId::LofiWobble, 0.5);
            ins.set_param(ParamId::ChorusRate, 1.5);
            ins.set_param(ParamId::LofiTone, 5_000.0);
            set_ducker(&mut ins, 0.5, 0.2);
            fx.set_bpm(100.0);
        }
        let (l, r) = ins.process(x);
        let (bl, br) = fx.process_bus((l, r), (l, r), (l, r));
        let (cl, cr) = chorus.process(lofi.process(x));
        acc += bl + br + cl + cr + ducker.tick();
    }
    let after = allocations();
    assert!(acc.is_finite());
    assert_eq!(after, before, "allocations during processing");
    // Sanity check that the counter works at all.
    let v: Vec<f32> = vec![0.0; 16];
    assert!(allocations() > after);
    drop(v);
}
