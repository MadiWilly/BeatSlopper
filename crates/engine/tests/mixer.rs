//! M6 test gate: the mixer inside the engine, rendered offline at 48 kHz.
//!
//! Mute silences a channel, solo isolates it, the pan law is equal-power (-3 dB centre), a
//! level of -6 dB halves the peak, sends feed the delay/reverb returns per channel, the synth
//! default still runs through drive/delay/reverb, meters report block peaks, project v3 files
//! load with the default mixer and round-trip as v4, and toggles are click-free.

mod common;

use common::{SAMPLE_RATE, all_finite, is_silent, left, ms_to_frames, peak, render, right, rms};
use engine::export::render_loop;
use engine::{
    Channel, ChannelParam, DrumRow, Engine, EngineHandle, Message, Meters, MixerState, ParamId,
    Project, STEPS, new_engine,
};

/// One 16-step bar at 120 BPM / 48 kHz.
const BAR: usize = 96_000;
/// Hard-panned channels leak nothing measurable into the other side.
const LEAK: f32 = 1.0e-4;

/// A fresh engine with dry effects (return levels 0) and a one-bar edit pattern holding a
/// single step-0 hit on `row`. The smoothers settle during a silent, stopped warm-up.
fn kit_engine(row: DrumRow) -> (EngineHandle, Engine) {
    let (handle, mut engine) = new_engine(SAMPLE_RATE);
    common::dry_fx(&mut engine);
    engine.set_drum_step(row, 0, true, true);
    (handle, engine)
}

/// Settles every smoother while stopped (must stay silent), then plays one bar.
fn settle_and_play_bar(engine: &mut Engine) -> Vec<f32> {
    let warmup = render(engine, ms_to_frames(200.0));
    assert!(is_silent(&warmup), "stopped engine must be silent");
    engine.play();
    let out = render(engine, BAR);
    engine.stop();
    out
}

fn largest_step(buf: &[f32]) -> f32 {
    buf.windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0, f32::max)
}

#[test]
fn defaults_render_the_kick_unchanged_and_centred() {
    let (_h, mut engine) = kit_engine(DrumRow::Kick);
    assert_eq!(engine.mixer(), &MixerState::default());
    let out = settle_and_play_bar(&mut engine);
    assert!(all_finite(&out));
    let (l, r) = (left(&out), right(&out));
    assert!(peak(&l) > 0.1, "kick peak {}", peak(&l));
    assert_eq!(l, r, "centre pan is exactly equal on both sides");
    // Unity gain at the centre: the kick lands at the kit's own level (about -7 dBFS).
    let db = 20.0 * peak(&l).log10();
    assert!((-9.0..=-5.0).contains(&db), "kick at {db} dBFS");
}

#[test]
fn mute_silences_a_channel() {
    let (_h, mut engine) = kit_engine(DrumRow::Kick);
    engine.set_mixer(Channel::Kick, ChannelParam::Mute, 1.0);
    assert!(engine.mixer().strip(Channel::Kick).mute);
    let out = settle_and_play_bar(&mut engine);
    assert!(is_silent(&out), "muted kick leaked, peak {}", peak(&out));

    // Unmuting brings it back; muting another channel changes nothing.
    engine.set_mixer(Channel::Kick, ChannelParam::Mute, 0.0);
    engine.set_mixer(Channel::Snare, ChannelParam::Mute, 1.0);
    let out = settle_and_play_bar(&mut engine);
    assert!(peak(&out) > 0.1);
}

#[test]
fn solo_isolates_a_channel() {
    // Kick on step 0, snare on step 4 (one beat later).
    let (mut handle, mut engine) = kit_engine(DrumRow::Kick);
    engine.set_drum_step(DrumRow::Snare, 4, true, true);
    assert!(handle.send(Message::SetMixer {
        channel: Channel::Snare,
        param: ChannelParam::Solo,
        value: 1.0,
    }));
    let out = left(&settle_and_play_bar(&mut engine));
    assert!(engine.mixer().strip(Channel::Snare).solo);
    let step = BAR / STEPS;
    let kick_region = &out[..4 * step - 100];
    let snare_region = &out[4 * step..8 * step];
    assert!(
        is_silent(kick_region),
        "soloing the snare must silence the kick, peak {}",
        peak(kick_region)
    );
    assert!(
        peak(snare_region) > 0.1,
        "snare peak {}",
        peak(snare_region)
    );

    // Soloing the kick as well lets both through; mute still wins over solo.
    engine.set_mixer(Channel::Kick, ChannelParam::Solo, 1.0);
    let out = left(&settle_and_play_bar(&mut engine));
    assert!(peak(&out[..4 * step - 100]) > 0.1);
    engine.set_mixer(Channel::Snare, ChannelParam::Mute, 1.0);
    let out = left(&settle_and_play_bar(&mut engine));
    // Only the kick's last decaying samples (below -80 dBFS) are left where the snare was.
    assert!(
        peak(&out[4 * step..5 * step]) < 1e-3,
        "muted solo channel still sounds: {}",
        peak(&out[4 * step..5 * step])
    );
    assert!(peak(&out[..4 * step - 100]) > 0.1);
}

#[test]
fn pan_law_hard_left_and_minus_three_db_centre() {
    let (_h, mut engine) = kit_engine(DrumRow::Kick);
    let centre = settle_and_play_bar(&mut engine);
    engine.set_mixer(Channel::Kick, ChannelParam::Pan, -1.0);
    let hard = settle_and_play_bar(&mut engine);
    let (hl, hr) = (peak(&left(&hard)), peak(&right(&hard)));
    assert!(hl > 0.1, "left peak {hl}");
    assert!(hr < hl * LEAK, "hard left leaks right: {hr} vs {hl}");
    let (cl, cr) = (peak(&left(&centre)), peak(&right(&centre)));
    assert_eq!(cl, cr);
    let db = 20.0 * (cl / hl).log10();
    assert!(
        (db + 3.01).abs() < 0.05,
        "centre is {db} dB relative to hard pan"
    );
    // And the mirror image.
    engine.set_mixer(Channel::Kick, ChannelParam::Pan, 1.0);
    let hard_r = settle_and_play_bar(&mut engine);
    assert!(peak(&left(&hard_r)) < peak(&right(&hard_r)) * LEAK);
    // The kick's click is a fresh noise burst per hit, so the peaks match only closely.
    let hr = peak(&right(&hard_r));
    assert!(
        (hr - hl).abs() < hl * 0.02,
        "hard right {hr} vs hard left {hl}"
    );
}

#[test]
fn level_minus_six_db_halves_the_peak() {
    let (_h, mut engine) = kit_engine(DrumRow::Kick);
    let unity = peak(&settle_and_play_bar(&mut engine));
    engine.set_mixer(Channel::Kick, ChannelParam::Level, -6.0);
    let quiet = peak(&settle_and_play_bar(&mut engine));
    let expected = unity * 10.0_f32.powf(-6.0 / 20.0);
    assert!(
        (quiet - expected).abs() < expected * 0.01,
        "unity {unity}, -6 dB {quiet}, expected {expected}"
    );
    // The level is clamped to the documented range and read back.
    assert_eq!(
        engine.set_mixer(Channel::Kick, ChannelParam::Level, 40.0),
        6.0
    );
    assert_eq!(engine.mixer().get(Channel::Kick, ChannelParam::Level), 6.0);
    engine.set_mixer(Channel::Kick, ChannelParam::Level, -60.0);
    let floor = peak(&settle_and_play_bar(&mut engine));
    assert!(floor < unity * 0.0011, "-60 dB floor {floor}");
}

/// Renders one bar (2 s) of a kick on step 0 with the given reverb/delay send, then two more
/// seconds with the transport stopped; returns the peak of those two seconds. The kick itself
/// reaches its envelope floor (exact zero) about 0.6 s after the hit, so anything left in
/// the tail comes from the effect returns.
fn tail_after_kick(send: ChannelParam, amount: f32, mix: ParamId) -> f32 {
    let (_h, mut engine) = kit_engine(DrumRow::Kick);
    engine.set_param(mix, 1.0);
    engine.set_param(ParamId::DelayFeedback, 0.7);
    engine.set_param(ParamId::ReverbSize, 0.8);
    engine.set_mixer(Channel::Kick, send, amount);
    let bar = settle_and_play_bar(&mut engine);
    assert!(
        is_silent(&bar[bar.len() / 2..]) || amount > 0.0,
        "the dry kick must have decayed to zero by the end of the bar"
    );
    let tail = render(&mut engine, ms_to_frames(2000.0));
    assert!(all_finite(&tail));
    peak(&tail)
}

#[test]
fn reverb_send_controls_the_kick_tail() {
    let dry = tail_after_kick(ChannelParam::ReverbSend, 0.0, ParamId::ReverbMix);
    let wet = tail_after_kick(ChannelParam::ReverbSend, 1.0, ParamId::ReverbMix);
    assert_eq!(
        dry, 0.0,
        "send 0 must leave no reverb tail (the kick itself has decayed)"
    );
    assert!(wet > 1e-4, "send 1 must leave a reverb tail, got {wet}");
}

#[test]
fn delay_send_controls_the_kick_echoes() {
    let dry = tail_after_kick(ChannelParam::DelaySend, 0.0, ParamId::DelayMix);
    let wet = tail_after_kick(ChannelParam::DelaySend, 1.0, ParamId::DelayMix);
    assert_eq!(dry, 0.0, "send 0 must produce no echoes");
    assert!(
        wet > 1e-3,
        "send 1 with feedback must still echo, got {wet}"
    );

    // The echo of a hard-left kick stays hard left: sends follow the pan.
    let (_h, mut engine) = kit_engine(DrumRow::Kick);
    engine.set_param(ParamId::DelayMix, 1.0);
    engine.set_param(ParamId::DelayFeedback, 0.0);
    engine.set_param(ParamId::DelayTime, 0.0); // 1/16 = 6000 samples
    engine.set_mixer(Channel::Kick, ChannelParam::DelaySend, 1.0);
    engine.set_mixer(Channel::Kick, ChannelParam::Pan, -1.0);
    let out = settle_and_play_bar(&mut engine);
    let echo_l = peak(&left(&out)[6_000..12_000]);
    let echo_r = peak(&right(&out)[6_000..12_000]);
    assert!(echo_l > 0.05, "echo missing: {echo_l}");
    assert!(echo_r < echo_l * LEAK, "echo leaked right: {echo_r}");
}

#[test]
fn synth_default_still_runs_through_drive_delay_and_reverb() {
    // Default project: synth sends 1.0 to both effects, defaults DelayMix 0.25 / ReverbMix 0.2.
    let (_h, mut engine) = new_engine(SAMPLE_RATE);
    engine.set_param(ParamId::DelayFeedback, 0.0);
    engine.set_param(ParamId::DelayTime, 0.0); // 6000 samples at 120 BPM
    engine.set_param(ParamId::AmpAttack, 0.001);
    engine.set_param(ParamId::AmpRelease, 0.001);
    engine.set_param(ParamId::LfoDepth, 0.0);
    let _settle = render(&mut engine, ms_to_frames(100.0));
    engine.note_on(69, 1.0);
    let mut out = render(&mut engine, 960); // 20 ms note
    engine.note_off(69);
    out.extend(render(&mut engine, 48_000 - 960));
    let l = left(&out);
    assert!(all_finite(&l));
    let note = rms(&l[..960]);
    let echo = rms(&l[6_000..6_960]);
    let between = rms(&l[2_500..5_500]);
    assert!(note > 0.05, "note rms {note}");
    assert!(
        echo > 0.01,
        "delay echo missing from the synth channel: {echo}"
    );
    assert!(
        between > 1e-4,
        "reverb tail missing between note and echo: {between}"
    );
    assert!(between < echo, "echo must stand out above the reverb");
    assert!(peak(&l[30_000..]) > 0.0, "reverb keeps ringing");

    // Drive: a fully driven sine is squarer (lower crest factor) than a clean one.
    let crest = |amount: f32, mix: f32| {
        let (_h, mut e) = common::seq_engine();
        e.set_param(ParamId::DriveAmount, amount);
        e.set_param(ParamId::DriveMix, mix);
        let _settle = render(&mut e, ms_to_frames(50.0));
        e.note_on(45, 1.0);
        let _attack = render(&mut e, ms_to_frames(50.0));
        let s = left(&render(&mut e, ms_to_frames(500.0)));
        peak(&s) / rms(&s)
    };
    let clean = crest(0.0, 0.0);
    let hot = crest(1.0, 1.0);
    assert!(hot < clean * 0.85, "driven crest {hot} vs clean {clean}");

    // Pulling the synth's sends to zero removes the effects from it.
    let (_h, mut engine) = new_engine(SAMPLE_RATE);
    engine.set_param(ParamId::DelayFeedback, 0.0);
    engine.set_param(ParamId::DelayTime, 0.0);
    engine.set_param(ParamId::AmpAttack, 0.001);
    engine.set_param(ParamId::AmpRelease, 0.001);
    engine.set_mixer(Channel::Synth, ChannelParam::DelaySend, 0.0);
    engine.set_mixer(Channel::Synth, ChannelParam::ReverbSend, 0.0);
    let _settle = render(&mut engine, ms_to_frames(100.0));
    engine.note_on(69, 1.0);
    let mut out = render(&mut engine, 960);
    engine.note_off(69);
    out.extend(render(&mut engine, 24_000 - 960));
    let l = left(&out);
    assert!(rms(&l[..960]) > 0.05);
    assert!(
        is_silent(&l[2_000..]),
        "no sends, no tail: {}",
        peak(&l[2_000..])
    );
}

#[test]
fn meters_report_block_peaks_per_channel_and_master() {
    let (handle, mut engine) = kit_engine(DrumRow::Kick);
    assert_eq!(handle.meters(), Meters::SILENT);
    let silent = render(&mut engine, ms_to_frames(100.0));
    assert!(is_silent(&silent));
    assert_eq!(engine.meters(), Meters::SILENT);
    assert_eq!(handle.meters(), Meters::SILENT);

    engine.play();
    let out = render(&mut engine, 512);
    let m = engine.meters();
    assert_eq!(handle.meters(), m, "handle reads what the engine published");
    assert!(m.channel[Channel::Kick.index()] > 0.1, "{m:?}");
    for ch in Channel::ALL.iter().filter(|&&c| c != Channel::Kick) {
        assert_eq!(m.of(*ch), 0.0, "{ch:?} should be silent");
    }
    assert_eq!(m.master[0], peak(&left(&out)));
    assert_eq!(m.master[1], peak(&right(&out)));
    assert_eq!(m.master_peak(), peak(&out));
    // Centre pan at unity: the channel meter equals the master (limiter below threshold).
    assert!(
        (m.of(Channel::Kick) - m.master_peak()).abs() < 1e-3,
        "{m:?}"
    );

    // Meters are per block: a later, quieter block reports its own peak.
    let _mid = render(&mut engine, 24_000);
    let later = render(&mut engine, 512);
    let m2 = engine.meters();
    assert!(m2.of(Channel::Kick) < m.of(Channel::Kick));
    assert_eq!(m2.master[0], peak(&left(&later)));

    // A muted channel meters post-fader: silent (once the 10 ms glide has fully settled).
    engine.set_mixer(Channel::Kick, ChannelParam::Mute, 1.0);
    let _settle = render(&mut engine, ms_to_frames(150.0));
    engine.play();
    let _ = render(&mut engine, 512);
    assert_eq!(engine.meters().of(Channel::Kick), 0.0);
    assert_eq!(engine.meters().master, [0.0, 0.0]);
}

#[test]
fn project_v3_loads_default_mixer_and_round_trips_as_v4() {
    let mut v3 = serde_json::to_value(Project::default()).unwrap();
    v3["version"] = serde_json::json!(3);
    v3.as_object_mut().unwrap().remove("mixer");
    let text = v3.to_string();
    assert!(!text.contains("\"mixer\""));
    let p = Project::from_json(&text).expect("v3 loads");
    assert_eq!(p.version, 4);
    assert_eq!(p.mixer, MixerState::default());

    let mut edited = p.clone();
    edited.mixer.set(Channel::Clap, ChannelParam::Pan, 0.5);
    edited
        .mixer
        .set(Channel::Snare, ChannelParam::ReverbSend, 0.9);
    edited.mixer.set(Channel::OpenHat, ChannelParam::Mute, 1.0);
    let json = edited.to_json_pretty();
    assert!(json.starts_with("{\n  \"version\": 4,"));
    assert!(json.contains("\"mixer\""));
    let back = Project::from_json(&json).unwrap();
    assert_eq!(back, edited);
    assert_eq!(back.mixer.get(Channel::Clap, ChannelParam::Pan), 0.5);
    assert!(back.mixer.strip(Channel::OpenHat).mute);
    assert_eq!(back.to_json_pretty(), json);

    // The exporter honours the mix: a project with the kick hard left exports it hard left.
    let mut project = Project::default();
    project.bank.patterns[0].clear_all();
    project.bank.patterns[0].set_drum_step(DrumRow::Kick, 0, true, true);
    project.set_param(ParamId::DelayMix, 0.0);
    project.set_param(ParamId::ReverbMix, 0.0);
    project.mixer.set(Channel::Kick, ChannelParam::Pan, -1.0);
    let samples = render_loop(&project, 1, SAMPLE_RATE);
    let (l, r) = (peak(&left(&samples)), peak(&right(&samples)));
    assert!(l > 0.1, "exported kick missing: {l}");
    assert!(r < l * LEAK, "exported kick leaked right: {r}");
    project.mixer.set(Channel::Kick, ChannelParam::Mute, 1.0);
    let muted = render_loop(&project, 1, SAMPLE_RATE);
    assert!(
        is_silent(&muted),
        "exported muted kick leaked (no glide on load)"
    );
}

#[test]
fn mute_toggle_is_smoothed_and_click_free() {
    // A long, sustained synth tone; a mute lands mid-note.
    let (_h, mut engine) = common::seq_engine();
    engine.note_on(45, 1.0); // A2, 110 Hz
    let _attack = render(&mut engine, ms_to_frames(100.0));
    let steady = left(&render(&mut engine, ms_to_frames(100.0)));
    let signal_step = largest_step(&steady);
    assert!(signal_step > 0.0);

    // The 10 ms one-pole glide snaps to exactly zero after about 92 ms (ln(1e4) time
    // constants), so every window below is 150 ms and judged from 100 ms on.
    let window = ms_to_frames(150.0);
    let settled = ms_to_frames(100.0);

    engine.set_mixer(Channel::Synth, ChannelParam::Mute, 1.0);
    let fade = left(&render(&mut engine, window));
    assert!(all_finite(&fade));
    assert!(
        largest_step(&fade) <= signal_step * 2.0,
        "mute clicked: step {} vs signal step {}",
        largest_step(&fade),
        signal_step
    );
    // It really fades (not an instant cut) and ends exactly silent.
    assert!(
        peak(&fade[..ms_to_frames(2.0)]) > 0.0,
        "a 10 ms glide is still audible at 2 ms"
    );
    assert!(
        is_silent(&fade[settled..]),
        "still sounding after 100 ms: {}",
        peak(&fade[settled..])
    );

    // Unmute glides back without a step either.
    engine.set_mixer(Channel::Synth, ChannelParam::Mute, 0.0);
    let back = left(&render(&mut engine, window));
    assert!(largest_step(&back) <= signal_step * 2.0);
    assert!(peak(&back[settled..]) > 0.1);

    // Solo and pan changes are smoothed the same way.
    engine.set_mixer(Channel::Kick, ChannelParam::Solo, 1.0);
    let solo = left(&render(&mut engine, window));
    assert!(largest_step(&solo) <= signal_step * 2.0);
    assert!(is_silent(&solo[settled..]));
    engine.set_mixer(Channel::Kick, ChannelParam::Solo, 0.0);
    let _ = render(&mut engine, window);
    engine.set_mixer(Channel::Synth, ChannelParam::Pan, 1.0);
    let pan = left(&render(&mut engine, window));
    assert!(largest_step(&pan) <= signal_step * 2.0);
    assert!(
        is_silent(&pan[settled..]),
        "hard right leaves the left side silent"
    );
}

#[test]
fn set_mixer_state_snaps_and_everything_stays_finite() {
    let (_h, mut engine) = kit_engine(DrumRow::Kick);
    engine.set_drum_step(DrumRow::Snare, 2, true, false);
    engine.set_drum_step(DrumRow::Clap, 4, true, false);
    engine.set_drum_step(DrumRow::ClosedHat, 6, true, false);
    engine.set_drum_step(DrumRow::OpenHat, 8, true, false);
    engine.set_synth_step(10, Some(40), true);
    let mut state = MixerState::default();
    for ch in Channel::ALL {
        state.set(ch, ChannelParam::Level, 6.0);
        state.set(
            ch,
            ChannelParam::Pan,
            if ch.index() % 2 == 0 { -1.0 } else { 1.0 },
        );
        state.set(ch, ChannelParam::DelaySend, 1.0);
        state.set(ch, ChannelParam::ReverbSend, 1.0);
    }
    state.strips[Channel::Synth.index()].level_db = f32::NAN; // normalised on load
    // Let the FX return levels (set to 0 by `dry_fx`) finish their own 10 ms glide while
    // stopped; the mixer itself is then loaded with a snap and must need no settling.
    let warmup = render(&mut engine, ms_to_frames(200.0));
    assert!(is_silent(&warmup));
    engine.set_mixer_state(&state);
    assert_eq!(engine.mixer().get(Channel::Synth, ChannelParam::Level), 0.0);
    assert_eq!(engine.mixer().get(Channel::Kick, ChannelParam::Level), 6.0);
    // Snapped: the very first samples already have the new pan (kick hard left, effects
    // dry so nothing crosses over).
    engine.play();
    let out = render(&mut engine, BAR);
    assert!(all_finite(&out));
    assert!(peak(&out) <= 1.0, "limiter holds: {}", peak(&out));
    let step = BAR / STEPS;
    let (first_l, first_r) = (
        peak(&left(&out)[..2 * step - 100]),
        peak(&right(&out)[..2 * step - 100]),
    );
    assert!(first_l > 0.1, "kick missing: {first_l}");
    assert!(first_r < first_l * LEAK, "kick leaked right: {first_r}");
    // Every channel metered something.
    let m = engine.meters();
    assert!(m.channel.iter().all(|&v| v > 0.0 && v.is_finite()), "{m:?}");
    assert!(m.master.iter().all(|&v| v > 0.0 && v <= 1.0));

    // Everything hot into both effects at full return: still finite and limited.
    engine.set_param(ParamId::DelayMix, 1.0);
    engine.set_param(ParamId::ReverbMix, 1.0);
    engine.set_param(ParamId::DelayFeedback, 0.95);
    engine.set_param(ParamId::ReverbSize, 1.0);
    engine.play();
    let out = render(&mut engine, 2 * BAR);
    assert!(all_finite(&out));
    assert!(peak(&out) <= 1.0, "limiter holds: {}", peak(&out));
    assert!(peak(&left(&out)) > 0.1 && peak(&right(&out)) > 0.1);
    let m = engine.meters();
    assert!(m.master.iter().all(|&v| v.is_finite() && v <= 1.0));
}
