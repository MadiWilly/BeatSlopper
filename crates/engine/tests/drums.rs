//! M3 drum kit tests, directly on [`DrumKit`] at 48 kHz (no engine, no audio device).

use engine::drums::{DrumKit, DrumRow};

const SR: f32 = 48_000.0;

fn frames(seconds: f32) -> usize {
    (SR * seconds).round() as usize
}

fn render(kit: &mut DrumKit, n: usize) -> Vec<f32> {
    (0..n).map(|_| kit.tick()).collect()
}

/// Lets the volume smoothers settle (they start at unity, so this only matters after
/// `set_volume`).
fn settle(kit: &mut DrumKit) {
    let _ = render(kit, frames(0.1));
}

/// A fresh kit, one hit of `row` at `velocity`, rendered for `seconds`.
fn hit(row: DrumRow, velocity: f32, seconds: f32) -> Vec<f32> {
    let mut kit = DrumKit::new(SR);
    kit.trigger(row, velocity);
    render(&mut kit, frames(seconds))
}

fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0_f32, |m, &x| m.max(x.abs()))
}

fn db(gain: f32) -> f32 {
    20.0 * gain.log10()
}

fn all_finite(buf: &[f32]) -> bool {
    buf.iter().all(|x| x.is_finite())
}

/// Largest absolute difference between consecutive samples.
fn max_step(buf: &[f32]) -> f32 {
    buf.windows(2)
        .fold(0.0_f32, |m, w| m.max((w[1] - w[0]).abs()))
}

/// Number of strict sign changes (zeros never count).
fn zero_crossings(buf: &[f32]) -> usize {
    buf.windows(2)
        .filter(|w| (w[0] < 0.0 && w[1] > 0.0) || (w[0] > 0.0 && w[1] < 0.0))
        .count()
}

fn energy(buf: &[f32]) -> f32 {
    buf.iter().map(|x| x * x).sum()
}

/// Index one past the last sample whose magnitude is at or above `threshold`.
fn last_loud(buf: &[f32], threshold: f32) -> usize {
    buf.iter()
        .rposition(|x| x.abs() >= threshold)
        .map_or(0, |i| i + 1)
}

fn window(buf: &[f32], from_s: f32, to_s: f32) -> &[f32] {
    &buf[frames(from_s)..frames(to_s)]
}

const MINUS_60_DB: f32 = 0.001;

#[test]
fn every_drum_sounds_finite_at_about_minus_6_dbfs_and_decays() {
    for row in DrumRow::ALL {
        let out = hit(row, 1.0, 2.0);
        assert!(all_finite(&out), "{row:?} not finite");
        let p = peak(&out);
        assert!(p > 0.01, "{row:?} is silent");
        let level = db(p);
        assert!(
            (-12.0..=-3.0).contains(&level),
            "{row:?} peaks at {level:.1} dBFS"
        );
        let limit = match row {
            DrumRow::ClosedHat => 0.3,
            DrumRow::OpenHat => 1.5,
            _ => 2.0,
        };
        let quiet_after = last_loud(&out, MINUS_60_DB) as f32 / SR;
        assert!(
            quiet_after < limit,
            "{row:?} still above -60 dBFS at {quiet_after:.3} s (limit {limit} s)"
        );
    }
}

#[test]
fn is_active_follows_the_envelopes() {
    let mut kit = DrumKit::new(SR);
    assert!(!kit.is_active());
    kit.trigger(DrumRow::Snare, 1.0);
    assert!(kit.is_active());
    let _ = render(&mut kit, frames(0.05));
    assert!(kit.is_active());
    let _ = render(&mut kit, frames(2.0));
    assert!(!kit.is_active());
}

#[test]
fn kick_is_darker_than_snare_which_is_darker_than_hat() {
    let kick = hit(DrumRow::Kick, 1.0, 0.2);
    let snare = hit(DrumRow::Snare, 1.0, 0.2);
    let hat = hit(DrumRow::ClosedHat, 1.0, 0.2);
    // 10-60 ms: past the kick click, while all three still sound.
    let zk = zero_crossings(window(&kick, 0.01, 0.06));
    let zs = zero_crossings(window(&snare, 0.01, 0.06));
    let zh = zero_crossings(window(&hat, 0.01, 0.06));
    assert!(zk * 4 < zs, "kick {zk} snare {zs}");
    assert!(zs * 3 < zh * 2, "snare {zs} hat {zh}");
}

#[test]
fn tune_plus_12_doubles_the_kick_frequency() {
    let base = hit(DrumRow::Kick, 1.0, 0.4);
    let mut kit = DrumKit::new(SR);
    kit.set_tune(DrumRow::Kick, 12.0);
    kit.trigger(DrumRow::Kick, 1.0);
    let up = render(&mut kit, frames(0.4));
    // 150-350 ms: the sweep has settled on the body frequency.
    let zb = zero_crossings(window(&base, 0.15, 0.35)) as f32;
    let zu = zero_crossings(window(&up, 0.15, 0.35)) as f32;
    let body_hz = zb / 2.0 / 0.2;
    assert!((40.0..=65.0).contains(&body_hz), "body {body_hz} Hz");
    let ratio = zu / zb;
    assert!((1.7..=2.3).contains(&ratio), "ratio {ratio}");
}

#[test]
fn tune_shifts_the_hat_brightness() {
    let down = {
        let mut kit = DrumKit::new(SR);
        kit.set_tune(DrumRow::OpenHat, -12.0);
        kit.trigger(DrumRow::OpenHat, 1.0);
        render(&mut kit, frames(0.2))
    };
    let up = {
        let mut kit = DrumKit::new(SR);
        kit.set_tune(DrumRow::OpenHat, 12.0);
        kit.trigger(DrumRow::OpenHat, 1.0);
        render(&mut kit, frames(0.2))
    };
    let zd = zero_crossings(window(&down, 0.01, 0.15));
    let zu = zero_crossings(window(&up, 0.01, 0.15));
    assert!(zd < zu, "down {zd} up {zu}");
}

#[test]
fn volume_minus_6_db_halves_the_peak() {
    for row in DrumRow::ALL {
        let reference = peak(&hit(row, 1.0, 0.5));
        let mut kit = DrumKit::new(SR);
        kit.set_volume(row, -6.0);
        settle(&mut kit);
        kit.trigger(row, 1.0);
        let out = render(&mut kit, frames(0.5));
        let ratio = peak(&out) / reference;
        assert!((0.47..=0.53).contains(&ratio), "{row:?} ratio {ratio}");
    }
}

#[test]
fn velocity_half_halves_the_peak() {
    for row in DrumRow::ALL {
        let reference = peak(&hit(row, 1.0, 0.5));
        let half = peak(&hit(row, 0.5, 0.5));
        let ratio = half / reference;
        assert!((0.49..=0.51).contains(&ratio), "{row:?} ratio {ratio}");
    }
}

#[test]
fn closed_hat_chokes_the_open_hat() {
    let choke_at = 0.1;
    let open_alone = hit(DrumRow::OpenHat, 1.0, 0.4);
    let mut kit = DrumKit::new(SR);
    kit.trigger(DrumRow::OpenHat, 1.0);
    let mut choked = render(&mut kit, frames(choke_at));
    kit.trigger(DrumRow::ClosedHat, 1.0);
    choked.extend(render(&mut kit, frames(0.4 - choke_at)));
    assert!(all_finite(&choked));
    // 50-150 ms after the choke the open hat alone would still ring; the choked kit only has
    // the dying closed hat left.
    let after = (choke_at + 0.05, choke_at + 0.15);
    let ringing = energy(window(&open_alone, after.0, after.1));
    let silenced = energy(window(&choked, after.0, after.1));
    assert!(
        silenced < ringing * 0.1,
        "ringing {ringing} choked {silenced}"
    );
    // The choke is a 5 ms linear fade, not a hard cut. The kit is linear and the noise is
    // deterministic, so the choked render must equal the fresh closed hat plus the open hat
    // multiplied by a ramp from 1 to 0 over 240 samples.
    let closed_alone = hit(DrumRow::ClosedHat, 1.0, 0.4 - choke_at);
    let fade_samples = 240.0;
    let start = frames(choke_at);
    let worst = choked[start..]
        .iter()
        .enumerate()
        .map(|(n, &x)| {
            let fade = (1.0 - n as f32 / fade_samples).max(0.0);
            let expected = closed_alone[n] + open_alone[start + n] * fade;
            (x - expected).abs()
        })
        .fold(0.0_f32, f32::max);
    assert!(
        worst < 1.0e-5,
        "choke deviates from a linear fade by {worst}"
    );
    // Half-way through the fade the open hat is still audibly present (not cut).
    let mid = start + 120;
    let open_mid = energy(&open_alone[mid..mid + 24]);
    let choked_mid = energy(&choked[mid..mid + 24]);
    let closed_mid = energy(&closed_alone[120..144]);
    assert!(
        choked_mid > closed_mid * 0.5 || open_mid < closed_mid,
        "open {open_mid} closed {closed_mid} choked {choked_mid}"
    );
}

#[test]
fn retrigger_while_sounding_does_not_click() {
    for row in DrumRow::ALL {
        let single = hit(row, 1.0, 0.3);
        let single_peak = peak(&single);
        let single_step = max_step(&single);
        let mut kit = DrumKit::new(SR);
        kit.trigger(row, 1.0);
        let mut out = render(&mut kit, frames(0.03));
        kit.trigger(row, 1.0);
        out.extend(render(&mut kit, frames(0.004)));
        kit.trigger(row, 1.0);
        out.extend(render(&mut kit, frames(0.0007)));
        kit.trigger(row, 1.0);
        out.extend(render(&mut kit, frames(0.3)));
        assert!(all_finite(&out), "{row:?}");
        let step = max_step(&out);
        // The kick is a smooth waveform: a restart must never jump by more than its peak. The
        // noise drums step by up to their peak every sample anyway; two voices overlapping in
        // the crossfade may at most double that.
        let bound = match row {
            DrumRow::Kick => single_peak,
            _ => single_peak.max(single_step * 2.0),
        };
        assert!(
            step <= bound,
            "{row:?} step {step} peak {single_peak} natural step {single_step}"
        );
        // During the 2 ms crossfade two full-level hits legitimately overlap (and a fresh
        // noise realisation may peak a little higher than the first); never much more.
        assert!(peak(&out) <= single_peak * 2.5, "{row:?} retrigger peak");
    }
}

#[test]
fn no_trigger_means_exact_silence() {
    let mut kit = DrumKit::new(SR);
    let out = render(&mut kit, frames(2.0));
    assert!(out.iter().all(|&x| x == 0.0));
    assert!(!kit.is_active());
}

#[test]
fn reset_silences_everything() {
    let mut kit = DrumKit::new(SR);
    for row in DrumRow::ALL {
        kit.trigger(row, 1.0);
    }
    let _ = render(&mut kit, frames(0.01));
    assert!(kit.is_active());
    kit.reset();
    assert!(!kit.is_active());
    let out = render(&mut kit, frames(0.1));
    assert!(out.iter().all(|&x| x == 0.0));
    // Still usable afterwards.
    kit.trigger(DrumRow::Kick, 1.0);
    assert!(peak(&render(&mut kit, frames(0.1))) > 0.1);
}

#[test]
fn all_five_at_once_stays_finite_and_bounded() {
    let mut kit = DrumKit::new(SR);
    for row in DrumRow::ALL {
        kit.trigger(row, 1.0);
    }
    let out = render(&mut kit, frames(1.0));
    assert!(all_finite(&out));
    let p = peak(&out);
    assert!(p > 0.3, "peak {p}");
    assert!(p <= 1.5, "peak {p}");
    // Even at +6 dB on every row nothing blows up (the engine limiter handles the level).
    let mut loud = DrumKit::new(SR);
    for row in DrumRow::ALL {
        loud.set_volume(row, 6.0);
    }
    settle(&mut loud);
    for row in DrumRow::ALL {
        loud.trigger(row, 1.0);
    }
    let out = render(&mut loud, frames(1.0));
    assert!(all_finite(&out));
    assert!(peak(&out) <= 3.0);
}

#[test]
fn works_at_other_sample_rates() {
    for sr in [22_050.0, 44_100.0, 96_000.0] {
        let mut kit = DrumKit::new(sr);
        assert_eq!(kit.sample_rate(), sr);
        for row in DrumRow::ALL {
            kit.trigger(row, 1.0);
        }
        let out = render(&mut kit, (sr * 2.0) as usize);
        assert!(all_finite(&out), "{sr}");
        assert!(peak(&out) > 0.3, "{sr}");
        assert!(!kit.is_active(), "{sr} still active after 2 s");
    }
    let bad = DrumKit::new(f32::NAN);
    assert_eq!(bad.sample_rate(), 48_000.0);
}
