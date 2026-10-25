//! The effect chain: drive -> delay -> reverb (M2), the M6 bus topology, and the M8 synth
//! channel insert (drive -> lo-fi -> chorus -> sidechain ducker), driven by [`ParamId`]
//! values.
//!
//! [`FxChain`] owns every effect plus one [`Smoother`] per continuous parameter, so a knob
//! move glides over about 10 ms instead of stepping. The delay time is the one discrete
//! parameter: it is a note division synced to the BPM (see [`division_seconds`]) and the
//! [`StereoDelay`] crossfades between the old and new time itself. The lo-fi and chorus
//! stages smooth their own parameters ([`Lofi`], [`Chorus`]).
//!
//! The master gain and the limiter are *not* part of the chain; the engine applies them after
//! [`FxChain::process`] / [`FxChain::process_bus`].
//!
//! # Engine wiring (M8)
//!
//! Per sample the engine:
//!
//! 1. sums the synth voices to mono and feeds that through
//!    [`FxChain::process_synth_insert`], which returns the **stereo** synth channel:
//!    `drive -> lo-fi -> chorus -> x ducker gain`. (Before M8 the engine applied
//!    [`FxChain::drive`] itself; `process_synth_insert` replaces that call and the synth
//!    strip's pan/level/sends then operate on the stereo pair.)
//! 2. feeds the drum rows through their own strips as before (no insert);
//! 3. sums the post-fader dry mix and sends and calls [`FxChain::process_bus`];
//! 4. calls [`FxChain::ducker_trigger`] whenever the kick fires, and
//!    [`FxChain::set_ducker`] when `SidechainAmount` / `SidechainRelease` change (those two
//!    ids are not FX ids; `Engine::set_param` routes them here directly).
//!
//! Three entry points share the same delay and reverb:
//!
//! - [`FxChain::process`] (M2): the serial insert, `drive -> delay -> reverb`, each with its
//!   dry/wet mix (`x + (wet - x) * mix`). Equivalent to sending one signal through the whole
//!   chain, the way the pre-mixer engine did.
//! - [`FxChain::process_bus`] (M6): the mixer topology. The engine applies the drive to the
//!   synth channel itself (via [`FxChain::drive`]) and sums per-channel post-fader sends into
//!   a stereo delay bus and a stereo reverb bus; `process_bus` returns
//!   `dry + delay(delay_in) * DelayMix + reverb(reverb_in) * ReverbMix`, so the two mix
//!   parameters act as *return levels* and the dry signal passes untouched.
//! - [`FxChain::process_synth_insert`] (M8): the synth channel insert described above. With
//!   `LofiMix`, `ChorusMix` and the ducker amount at zero it returns the drive output on both
//!   channels bit for bit.
//!
//! Real-time safe after [`FxChain::new`]: nothing here allocates, locks, logs or panics.
//! Every heap buffer (delay, reverb, lo-fi wobble line, chorus line) is allocated in `new`
//! and only indexed afterwards; `tests/fx_m8.rs` checks this with a counting allocator.

use crate::params::{DEFAULT_SMOOTH_MS, ParamId, Smoother};

use super::chorus::Chorus;
use super::delay::StereoDelay;
use super::drive::Drive;
use super::ducker::Ducker;
use super::lofi::Lofi;
use super::reverb::Reverb;

/// Longest delay the chain can produce, in seconds (the pre-allocated buffer length).
pub const MAX_DELAY_SECONDS: f32 = 4.0;

/// Smallest BPM accepted by [`FxChain::set_bpm`]; smaller values clamp.
pub const MIN_BPM: f32 = 1.0;

/// Largest BPM accepted by [`FxChain::set_bpm`]; larger values clamp.
pub const MAX_BPM: f32 = 999.0;

/// BPM a fresh chain starts with (and the fallback for non-finite input).
pub const DEFAULT_BPM: f32 = 120.0;

/// Beats per note division, indexed like `ParamId::DelayTime`: 1/16, 1/8, 1/4, 1/2.
const DIVISION_BEATS: [f32; 4] = [0.25, 0.5, 1.0, 2.0];

/// Length in seconds of the delay division `index` at `bpm`.
///
/// `index` is the `ParamId::DelayTime` value: 0 = 1/16 (`15 / bpm`), 1 = 1/8 (`30 / bpm`),
/// 2 = 1/4 (`60 / bpm`), 3 = 1/2 (`120 / bpm`). Fractional indices round to the nearest
/// division and out-of-range ones clamp; a non-positive or non-finite `bpm` is treated as
/// [`DEFAULT_BPM`].
#[must_use]
pub fn division_seconds(index: f32, bpm: f32) -> f32 {
    let bpm = if bpm.is_finite() && bpm > 0.0 {
        bpm
    } else {
        DEFAULT_BPM
    };
    let i = if index.is_finite() {
        // `as usize` saturates at 0 for negatives; the clamp handles the top.
        (index.round() as usize).min(DIVISION_BEATS.len() - 1)
    } else {
        1
    };
    DIVISION_BEATS[i] * 60.0 / bpm
}

/// Drive -> delay -> reverb plus the lo-fi / chorus / ducker synth insert, with smoothed
/// parameters and BPM-synced delay time.
#[derive(Debug, Clone, PartialEq)]
pub struct FxChain {
    sample_rate: f32,
    bpm: f32,
    /// Clamped target values, indexed by `ParamId::index()`; only FX ids are meaningful.
    values: [f32; ParamId::COUNT],
    drive: Drive,
    delay: StereoDelay,
    reverb: Reverb,
    lofi: Lofi,
    chorus: Chorus,
    ducker: Ducker,
    drive_amount: Smoother,
    drive_mix: Smoother,
    delay_feedback: Smoother,
    delay_mix: Smoother,
    reverb_size: Smoother,
    reverb_damping: Smoother,
    reverb_mix: Smoother,
    delay_seconds: f32,
    /// The reverb return of the last `process_bus` frame (post return level), for the
    /// plugins' wet-level meter.
    last_reverb_wet: (f32, f32),
}

impl FxChain {
    /// Pre-allocates the chain for `sample_rate` (non-finite or non-positive rates fall back
    /// to 48 kHz). Every FX parameter starts at its [`ParamId::default_value`] and the BPM at
    /// [`DEFAULT_BPM`].
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let mut values = [0.0; ParamId::COUNT];
        for id in ParamId::ALL {
            values[id.index()] = id.default_value();
        }
        let smoother =
            |id: ParamId| Smoother::new(sample_rate, DEFAULT_SMOOTH_MS, id.default_value());
        let mut chain = Self {
            sample_rate,
            bpm: DEFAULT_BPM,
            values,
            drive: Drive::new(),
            delay: StereoDelay::new(sample_rate, MAX_DELAY_SECONDS),
            reverb: Reverb::new(sample_rate),
            lofi: Lofi::new(sample_rate),
            chorus: Chorus::new(sample_rate),
            ducker: Ducker::new(sample_rate),
            drive_amount: smoother(ParamId::DriveAmount),
            drive_mix: smoother(ParamId::DriveMix),
            delay_feedback: smoother(ParamId::DelayFeedback),
            delay_mix: smoother(ParamId::DelayMix),
            reverb_size: smoother(ParamId::ReverbSize),
            reverb_damping: smoother(ParamId::ReverbDamping),
            reverb_mix: smoother(ParamId::ReverbMix),
            delay_seconds: 0.0,
            last_reverb_wet: (0.0, 0.0),
        };
        chain.update_delay_time();
        chain.reset();
        chain
    }

    /// Sample rate the chain was built for.
    #[must_use]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Current tempo used for the delay sync.
    #[must_use]
    pub fn bpm(&self) -> f32 {
        self.bpm
    }

    /// Sets the tempo (clamped to `MIN_BPM..=MAX_BPM`; non-finite values are ignored) and
    /// re-syncs the delay time.
    pub fn set_bpm(&mut self, bpm: f32) {
        if !bpm.is_finite() {
            return;
        }
        self.bpm = bpm.clamp(MIN_BPM, MAX_BPM);
        self.update_delay_time();
    }

    /// Current synced delay time in seconds (the time the delay is at or fading towards).
    #[must_use]
    pub fn delay_seconds(&self) -> f32 {
        self.delay_seconds
    }

    /// Current synced delay time in whole samples.
    #[must_use]
    pub fn delay_samples(&self) -> usize {
        self.delay.delay_samples()
    }

    fn update_delay_time(&mut self) {
        let index = self.values[ParamId::DelayTime.index()];
        self.delay_seconds = division_seconds(index, self.bpm).min(MAX_DELAY_SECONDS);
        self.delay.set_delay_seconds(self.delay_seconds);
    }

    /// Sets an effect parameter. The value is clamped with [`ParamId::clamp`]; non-FX ids are
    /// ignored. Continuous parameters glide to the new value; the delay time crossfades; the
    /// `Lofi*` and `Chorus*` ids are forwarded to [`Lofi::set_param`] / [`Chorus::set_param`].
    pub fn set_param(&mut self, id: ParamId, value: f32) {
        if !id.is_fx() {
            return;
        }
        let v = id.clamp(value);
        self.values[id.index()] = v;
        match id {
            ParamId::DriveAmount => self.drive_amount.set_target(v),
            ParamId::DriveMix => self.drive_mix.set_target(v),
            ParamId::DelayTime => self.update_delay_time(),
            ParamId::DelayFeedback => self.delay_feedback.set_target(v),
            ParamId::DelayMix => self.delay_mix.set_target(v),
            ParamId::ReverbSize => self.reverb_size.set_target(v),
            ParamId::ReverbDamping => self.reverb_damping.set_target(v),
            ParamId::ReverbMix => self.reverb_mix.set_target(v),
            ParamId::LofiBits
            | ParamId::LofiRate
            | ParamId::LofiWobble
            | ParamId::LofiNoise
            | ParamId::LofiTone
            | ParamId::LofiMix => self.lofi.set_param(id, v),
            ParamId::ChorusRate | ParamId::ChorusDepth | ParamId::ChorusMix => {
                self.chorus.set_param(id, v);
            }
            _ => {}
        }
    }

    /// Sets the sidechain ducker's dip depth (`0.0..=1.0`) and release time in seconds; see
    /// [`Ducker::set`]. These are the `SidechainAmount` / `SidechainRelease` ids, which the
    /// engine routes here rather than through [`set_param`](Self::set_param).
    pub fn set_ducker(&mut self, amount: f32, release: f32) {
        self.ducker.set(amount, release);
    }

    /// Starts a sidechain dip (the engine calls this when the kick fires).
    pub fn ducker_trigger(&mut self) {
        self.ducker.trigger();
    }

    /// The lo-fi stage of the synth insert.
    #[must_use]
    pub fn lofi(&self) -> &Lofi {
        &self.lofi
    }

    /// The chorus stage of the synth insert.
    #[must_use]
    pub fn chorus(&self) -> &Chorus {
        &self.chorus
    }

    /// The sidechain ducker of the synth insert.
    #[must_use]
    pub fn ducker(&self) -> &Ducker {
        &self.ducker
    }

    /// Current (clamped target) value of an effect parameter. Non-FX ids return `0.0`.
    #[must_use]
    pub fn param(&self, id: ParamId) -> f32 {
        if id.is_fx() {
            self.values[id.index()]
        } else {
            0.0
        }
    }

    /// Clears every delay line and reverb tail, snaps all smoothers to their targets and
    /// completes any pending delay-time crossfade. The one call here that may click; use it
    /// on project load or when the stream (re)starts, not while audible.
    pub fn reset(&mut self) {
        self.delay.reset();
        self.reverb.reset();
        self.lofi.reset();
        self.chorus.reset();
        self.ducker.reset();
        for s in [
            &mut self.drive_amount,
            &mut self.drive_mix,
            &mut self.delay_feedback,
            &mut self.delay_mix,
            &mut self.reverb_size,
            &mut self.reverb_damping,
            &mut self.reverb_mix,
        ] {
            s.snap(s.target());
        }
        self.apply_smoothed();
    }

    /// Snaps the reverb's size / damping / mix smoothers and its pre-delay / width glides to
    /// their targets, leaving every delay line and tail alone (project load, see
    /// `Engine::set_reverb_state`). Unlike [`reset`](Self::reset) this never clears audio.
    pub fn snap_reverb(&mut self) {
        for s in [
            &mut self.reverb_size,
            &mut self.reverb_damping,
            &mut self.reverb_mix,
        ] {
            s.snap(s.target());
        }
        self.apply_smoothed();
        self.reverb.snap();
    }

    /// Pushes the smoothers' current values into the effects.
    #[inline]
    fn apply_smoothed(&mut self) {
        self.drive.set_amount(self.drive_amount.current());
        self.drive.set_mix(self.drive_mix.current());
        self.delay.set_feedback(self.delay_feedback.current());
        self.delay.set_mix(self.delay_mix.current());
        self.reverb.set_size(self.reverb_size.current());
        self.reverb.set_damping(self.reverb_damping.current());
        self.reverb.set_mix(self.reverb_mix.current());
    }

    /// Advances every parameter smoother one sample and pushes the values into the effects.
    #[inline]
    fn tick_smoothers(&mut self) {
        self.drive_amount.tick();
        self.drive_mix.tick();
        self.delay_feedback.tick();
        self.delay_mix.tick();
        self.reverb_size.tick();
        self.reverb_damping.tick();
        self.reverb_mix.tick();
        self.apply_smoothed();
    }

    /// The drive stage with its current (smoothed) amount and mix, for use as a channel
    /// insert: the engine applies it to the synth channel before the mixer. It reflects the
    /// smoother state as of the last `process` / `process_bus` call.
    #[must_use]
    pub fn drive(&self) -> &Drive {
        &self.drive
    }

    /// The M8 synth channel insert: `drive -> lo-fi -> chorus -> x ducker gain`, mono in,
    /// stereo out. The drive uses the smoothed amount/mix as of the last
    /// [`process`](Self::process) / [`process_bus`](Self::process_bus) call (like
    /// [`drive`](Self::drive)); lo-fi and chorus tick their own smoothers here, and the
    /// ducker advances one sample. Call it once per sample, before `process_bus`.
    ///
    /// With `LofiMix = 0`, `ChorusMix = 0` and no dip running, the result is the drive output
    /// on both channels exactly. Non-finite input is treated as `0.0`; the output is finite.
    #[inline]
    pub fn process_synth_insert(&mut self, x: f32) -> (f32, f32) {
        let x = self.drive.process(x);
        let x = self.lofi.process(x);
        let (l, r) = self.chorus.process(x);
        let g = self.ducker.tick();
        (l * g, r * g)
    }

    /// The reverb (plugins milestone hook): pre-delay, width and the wet filters are set
    /// directly on it by the engine; size, damping and mix keep going through
    /// [`set_param`](Self::set_param).
    #[must_use]
    pub fn reverb(&self) -> &Reverb {
        &self.reverb
    }

    /// The reverb, mutably (see [`reverb`](Self::reverb)).
    pub fn reverb_mut(&mut self) -> &mut Reverb {
        &mut self.reverb
    }

    /// The reverb return `(left, right)` of the last [`process_bus`](Self::process_bus)
    /// frame, after the return level (the plugins' wet-level meter reads its block peak).
    #[must_use]
    pub fn last_reverb_wet(&self) -> (f32, f32) {
        self.last_reverb_wet
    }

    /// Processes one stereo frame through drive, delay and reverb in series, each with its
    /// dry/wet mix (the M2 insert topology). Non-finite inputs are treated as `0.0`; the
    /// output is always finite.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        self.tick_smoothers();
        let l = if l.is_finite() { l } else { 0.0 };
        let r = if r.is_finite() { r } else { 0.0 };
        let (l, r) = (self.drive.process(l), self.drive.process(r));
        let (l, r) = self.delay.process(l, r);
        self.reverb.process(l, r)
    }

    /// Processes one frame of the mixer buses: returns
    /// `dry + delay(delay_in) * DelayMix + reverb(reverb_in) * ReverbMix`.
    ///
    /// `dry` is the summed post-fader, panned mix; `delay_in` / `reverb_in` are the summed
    /// post-fader sends. The drive is **not** applied here (see [`drive`](Self::drive)); the
    /// delay feedback, reverb size and damping work as in [`process`](Self::process). With a
    /// return level of `0.0` an effect contributes nothing; with silent sends the effects stay
    /// exactly silent. Non-finite inputs are treated as `0.0`; the output is always finite.
    #[inline]
    pub fn process_bus(
        &mut self,
        dry: (f32, f32),
        delay_in: (f32, f32),
        reverb_in: (f32, f32),
    ) -> (f32, f32) {
        #[inline]
        fn clean(x: f32) -> f32 {
            if x.is_finite() { x } else { 0.0 }
        }
        self.tick_smoothers();
        // The buses want the pure wet signal; the return level is applied below.
        self.delay.set_mix(1.0);
        self.reverb.set_mix(1.0);
        let (dl, dr) = self.delay.process(clean(delay_in.0), clean(delay_in.1));
        let (rl, rr) = self.reverb.process(clean(reverb_in.0), clean(reverb_in.1));
        let delay_level = self.delay_mix.current();
        let reverb_level = self.reverb_mix.current();
        self.last_reverb_wet = (rl * reverb_level, rr * reverb_level);
        (
            clean(dry.0) + dl * delay_level + rl * reverb_level,
            clean(dry.1) + dr * delay_level + rr * reverb_level,
        )
    }

    /// Processes an interleaved stereo buffer in place. A trailing odd sample (an incomplete
    /// frame) is left untouched.
    pub fn process_buffer(&mut self, buf: &mut [f32]) {
        let (frames, _odd_tail) = buf.as_chunks_mut::<2>();
        for [l, r] in frames {
            (*l, *r) = self.process(*l, *r);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    #[test]
    fn division_table() {
        assert!((division_seconds(0.0, 120.0) - 0.125).abs() < 1e-6);
        assert!((division_seconds(1.0, 120.0) - 0.25).abs() < 1e-6);
        assert!((division_seconds(2.0, 120.0) - 0.5).abs() < 1e-6);
        assert!((division_seconds(3.0, 120.0) - 1.0).abs() < 1e-6);
        assert!((division_seconds(1.0, 90.0) - 1.0 / 3.0).abs() < 1e-6);
        // Rounding, clamping and junk.
        assert_eq!(division_seconds(1.4, 120.0), division_seconds(1.0, 120.0));
        assert_eq!(division_seconds(9.0, 120.0), division_seconds(3.0, 120.0));
        assert_eq!(division_seconds(-4.0, 120.0), division_seconds(0.0, 120.0));
        assert_eq!(division_seconds(1.0, 0.0), 0.25);
        assert_eq!(division_seconds(1.0, f32::NAN), 0.25);
        assert_eq!(division_seconds(f32::NAN, 120.0), 0.25);
    }

    #[test]
    fn defaults_and_bpm_sync() {
        let fx = FxChain::new(SR);
        assert_eq!(fx.bpm(), 120.0);
        assert_eq!(fx.param(ParamId::DelayTime), 1.0);
        assert_eq!(fx.delay_samples(), 12_000);
        assert!((fx.delay_seconds() - 0.25).abs() < 1e-6);
        for id in ParamId::ALL.iter().filter(|id| id.is_fx()) {
            assert_eq!(fx.param(*id), id.default_value(), "{id:?}");
        }
        assert_eq!(fx.param(ParamId::MasterGain), 0.0);
    }

    #[test]
    fn set_bpm_clamps_and_resyncs() {
        let mut fx = FxChain::new(SR);
        fx.set_bpm(90.0);
        assert_eq!(fx.delay_samples(), 16_000);
        fx.set_bpm(f32::INFINITY);
        assert_eq!(fx.bpm(), 90.0);
        fx.set_bpm(-5.0);
        assert_eq!(fx.bpm(), MIN_BPM);
        // 1/2 note at 1 BPM is 120 s, clamped to the 4 s buffer.
        fx.set_param(ParamId::DelayTime, 3.0);
        assert_eq!(fx.delay_seconds(), MAX_DELAY_SECONDS);
        assert_eq!(fx.delay_samples(), 192_000);
    }

    #[test]
    fn set_param_filters_and_clamps() {
        let mut fx = FxChain::new(SR);
        fx.set_param(ParamId::DelayFeedback, 3.0);
        assert_eq!(fx.param(ParamId::DelayFeedback), 0.95);
        fx.set_param(ParamId::DriveMix, -1.0);
        assert_eq!(fx.param(ParamId::DriveMix), 0.0);
        fx.set_param(ParamId::ReverbSize, f32::NAN);
        assert_eq!(
            fx.param(ParamId::ReverbSize),
            ParamId::ReverbSize.default_value()
        );
        let before = fx.clone();
        fx.set_param(ParamId::MasterGain, 0.1);
        fx.set_param(ParamId::FilterCutoff, 100.0);
        assert_eq!(fx, before);
    }

    #[test]
    fn process_bus_sums_dry_and_wet_returns() {
        // Return levels: dry passes untouched, a silent send contributes nothing.
        let mut fx = FxChain::new(SR);
        fx.set_param(ParamId::DelayMix, 1.0);
        fx.set_param(ParamId::ReverbMix, 1.0);
        fx.set_param(ParamId::DelayFeedback, 0.0);
        fx.reset();
        for i in 0..24_000 {
            let x = (i as f32 * 0.01).sin() * 0.5;
            let (l, r) = fx.process_bus((x, -x), (0.0, 0.0), (0.0, 0.0));
            assert_eq!((l, r), (x, -x), "sample {i}");
        }
        // A delay-bus impulse comes back once, at the delay time, at the return level.
        fx.set_param(ParamId::DelayMix, 0.5);
        fx.reset();
        let n = fx.delay_samples();
        let mut out = Vec::with_capacity(n + 10);
        for i in 0..n + 10 {
            let x = if i == 0 { 1.0 } else { 0.0 };
            out.push(fx.process_bus((0.0, 0.0), (x, x), (0.0, 0.0)).0);
        }
        assert_eq!(out[0], 0.0, "no dry leak from the delay bus");
        assert!((out[n] - 0.5).abs() < 1e-5, "echo {}", out[n]);
        assert!(out[1..n].iter().all(|&v| v.abs() < 1e-6));
        // A reverb-bus impulse produces a tail; mix 0 silences the return entirely.
        fx.set_param(ParamId::ReverbMix, 1.0);
        fx.reset();
        let tail: Vec<f32> = (0..24_000)
            .map(|i| {
                let x = if i == 0 { 1.0 } else { 0.0 };
                fx.process_bus((0.0, 0.0), (0.0, 0.0), (x, x)).0
            })
            .collect();
        assert!(tail.iter().any(|&v| v.abs() > 1e-3));
        assert!(tail.iter().all(|v| v.is_finite()));
        fx.set_param(ParamId::ReverbMix, 0.0);
        fx.reset();
        for i in 0..24_000 {
            let x = if i == 0 { 1.0 } else { 0.0 };
            assert_eq!(
                fx.process_bus((0.0, 0.0), (0.0, 0.0), (x, x)),
                (0.0, 0.0),
                "{i}"
            );
        }
        // Non-finite input never escapes.
        let (l, r) = fx.process_bus((f32::NAN, 0.0), (f32::INFINITY, 0.0), (0.0, f32::NAN));
        assert!(l.is_finite() && r.is_finite());
    }

    #[test]
    fn process_and_process_bus_share_parameters() {
        let mut fx = FxChain::new(SR);
        fx.set_param(ParamId::DriveAmount, 1.0);
        fx.set_param(ParamId::DriveMix, 1.0);
        fx.reset();
        // `drive()` reflects the smoothed state after a tick.
        let _ = fx.process_bus((0.0, 0.0), (0.0, 0.0), (0.0, 0.0));
        assert_eq!(fx.drive().amount(), 1.0);
        assert_eq!(fx.drive().mix(), 1.0);
        assert!(fx.drive().process(0.5) > 0.9);
        // The serial path still mixes dry/wet the M2 way after a bus call set the mixes to 1.
        fx.set_param(ParamId::DriveMix, 0.0);
        fx.set_param(ParamId::DelayMix, 0.0);
        fx.set_param(ParamId::ReverbMix, 0.0);
        fx.set_param(ParamId::DelayFeedback, 0.0);
        fx.reset();
        let _ = fx.process_bus((0.0, 0.0), (0.0, 0.0), (0.0, 0.0));
        assert_eq!(fx.process(0.25, -0.25), (0.25, -0.25));
    }

    #[test]
    fn weird_sample_rate_falls_back() {
        let fx = FxChain::new(0.0);
        assert_eq!(fx.sample_rate(), 48_000.0);
    }
}
