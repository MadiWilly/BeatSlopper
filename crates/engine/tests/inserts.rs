//! M11 FL-style mixer: sources summed on shared inserts, route-to groups and the feedback
//! rule, the compressor and EQ3 slots, slot order, bit-transparent neutral inserts, the v7
//! migration (sample-identical render), insert solo, the messages and the release-mode perf
//! gate with compressors on eight inserts.

mod common;

use common::{
    SAMPLE_RATE, all_finite, is_silent, left, ms_to_frames, peak, render, render_in_blocks, right,
    rms, seq_engine,
};
use engine::inserts::{INSERT_COUNT, MASTER};
use engine::presets;
use engine::{
    Channel, ChannelParam, DrumRow, Engine, Eq3State, InsertParam, InsertState, MAX_TRACKS,
    Message, MixerState, Note, ParamId, Project, SlotKind, SlotState, Strip, new_engine,
};

/// One 16-step bar at 120 BPM / 48 kHz.
const BAR: usize = 96_000;
const STEP: usize = BAR / 16;

/// FNV-1a 64 over the `f32` bit patterns (little-endian bytes) of a buffer.
fn fnv1a(samples: &[f32]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for s in samples {
        for b in s.to_bits().to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

/// Reference render of `tests/fixtures/v7_demo.json` (the pre-M11 `Project::default()` as a
/// v7 file) with the pre-M11 engine: `play()`, 1 s at 48 kHz in blocks of 512. Captured
/// before the insert refactor; the render below must reproduce it exactly. (It equals the
/// v6 reference in `tests/tracks.rs`: the demo did not change between the formats.)
const V7_REFERENCE_HASH: u64 = 0xd016_a4bb_b4c2_4052;
/// RMS of each 100 ms window of the left channel of that render.
const V7_REFERENCE_RMS: [f32; 10] = [
    0.426_146_2,
    0.250_117_5,
    0.267_908_04,
    0.357_108_7,
    0.103_825_375,
    0.438_972_8,
    0.280_141_17,
    0.303_809_31,
    0.379_298_93,
    0.101_760_54,
];

const V7_DEMO_JSON: &str = include_str!("fixtures/v7_demo.json");

/// Asserts two renders are sample-identical, reporting the first difference instead of
/// dumping both buffers.
fn assert_same(a: &[f32], b: &[f32], what: &str) {
    assert_eq!(a.len(), b.len(), "{what}: lengths differ");
    if let Some(i) = (0..a.len()).find(|&i| a[i].to_bits() != b[i].to_bits() && a[i] != b[i]) {
        let worst = a
            .iter()
            .zip(b)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0_f32, f32::max);
        panic!(
            "{what}: first difference at sample {i}: {} vs {} (max deviation {worst})",
            a[i], b[i]
        );
    }
}

/// `true` when two renders differ anywhere by more than `eps`.
fn differs(a: &[f32], b: &[f32], eps: f32) -> bool {
    a.len() != b.len() || a.iter().zip(b).any(|(x, y)| (x - y).abs() > eps)
}

/// A dry engine (no effect returns, limiter off, master gain 1) with nothing programmed.
fn dry_engine() -> Engine {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    common::dry_fx(&mut engine);
    engine.set_param(ParamId::LimiterOn, 0.0);
    engine.set_param(ParamId::MasterGain, 1.0);
    engine
}

/// Settles every smoother while stopped (the last 100 ms must be silent: a previous bar's
/// tails may still be fading at the start), then plays one bar.
fn settle_and_play_bar(engine: &mut Engine) -> Vec<f32> {
    let warmup = render(engine, ms_to_frames(300.0));
    assert!(
        is_silent(&warmup[ms_to_frames(200.0) * 2..]),
        "stopped engine must be silent"
    );
    engine.play();
    let out = render(engine, BAR);
    engine.stop();
    out
}

/// Kick on step 0 and snare on step 4, both routed to `insert` (`0` = master).
fn drums_engine(kick: bool, snare: bool, insert: u8) -> Engine {
    let mut e = dry_engine();
    if kick {
        e.set_drum_step(DrumRow::Kick, 0, true, true);
    }
    if snare {
        e.set_drum_step(DrumRow::Snare, 4, true, true);
    }
    e.set_route(Channel::Kick, insert);
    e.set_route(Channel::Snare, insert);
    e
}

/// A dry sine-patch engine whose track 0 (insert 6) holds a sustained note of `pitch` at
/// `velocity` on step 0 (length 16), drum rows muted.
fn tone_engine(pitch: u8, velocity: f32) -> Engine {
    let (_h, mut e) = seq_engine();
    e.set_param(ParamId::LimiterOn, 0.0);
    e.set_param(ParamId::MasterGain, 1.0);
    for row in DrumRow::ALL {
        e.set_mixer(Channel::from_row(row), ChannelParam::Mute, 1.0);
    }
    e.set_note(0, 0, 0, Note::new(0, 16, pitch, velocity));
    e
}

/// Steady part of a one-bar tone render (the first 300 ms of attack and smoothing skipped).
fn steady(out: &[f32]) -> Vec<f32> {
    left(out)[ms_to_frames(300.0)..BAR - STEP].to_vec()
}

fn db(ratio: f32) -> f32 {
    20.0 * ratio.log10()
}

// (1) Two sources routed to one insert sum there; a muted insert silences both.

#[test]
fn two_sources_on_one_insert_sum_there_and_a_muted_insert_silences_both() {
    let kick = settle_and_play_bar(&mut drums_engine(true, false, 14));
    let snare = settle_and_play_bar(&mut drums_engine(false, true, 14));
    let mut e = drums_engine(true, true, 14);
    let both = settle_and_play_bar(&mut e);
    assert!(all_finite(&both));
    assert!(peak(&kick) > 0.1 && peak(&snare) > 0.1);
    assert!(peak(&left(&kick)[..4 * STEP - 100]) > 0.1);
    assert!(
        is_silent(&left(&kick)[6 * STEP..]),
        "the kick has decayed by step 6"
    );
    let sum: Vec<f32> = kick.iter().zip(&snare).map(|(a, b)| a + b).collect();
    assert_same(
        &both,
        &sum,
        "the insert bus is the plain sum of its sources",
    );
    assert_eq!(e.mixer().route(Channel::Kick), 14);
    assert_eq!(e.mixer().route(Channel::Snare), 14);

    // Mute the insert: both sources fall silent.
    assert_eq!(e.set_insert(14, InsertParam::Mute, 1.0), 1.0);
    assert!(e.mixer().insert(14).mute);
    let muted = settle_and_play_bar(&mut e);
    assert!(
        is_silent(&muted),
        "muted insert leaked, peak {}",
        peak(&muted)
    );
    // Unmuting brings both back (the kick's click is a fresh noise burst per hit, so a
    // re-render on the same engine matches the first one closely, not bit for bit).
    e.set_insert(14, InsertParam::Mute, 0.0);
    let back = settle_and_play_bar(&mut e);
    assert!((rms(&back) - rms(&sum)).abs() < rms(&sum) * 0.02, "unmuted");
    assert!(
        peak(&left(&back)[4 * STEP..5 * STEP]) > 0.1,
        "the snare is back"
    );
    // Level and pan on the insert act on the whole bus.
    e.set_insert(14, InsertParam::Level, -6.0);
    e.set_insert(14, InsertParam::Pan, -1.0);
    let quiet_left = settle_and_play_bar(&mut e);
    let expected = peak(&sum) * 10.0_f32.powf(-6.0 / 20.0) * core::f32::consts::SQRT_2;
    let got = peak(&left(&quiet_left));
    assert!(
        (got - expected).abs() < expected * 0.01,
        "{got} vs {expected}"
    );
    assert!(peak(&right(&quiet_left)) < got * 1e-4, "hard left");
    // Meters: the source meters still see a muted insert's sources (they are post-strip,
    // pre-insert), the insert meter and the master do not.
    e.set_insert(14, InsertParam::Level, 0.0);
    e.set_insert(14, InsertParam::Pan, 0.0);
    e.set_insert(14, InsertParam::Mute, 1.0);
    let _ = render(&mut e, ms_to_frames(200.0));
    e.play();
    let _ = render(&mut e, 512);
    let m = e.meters();
    assert!(m.of(Channel::Kick) > 0.1, "{m:?}");
    assert_eq!(m.of_insert(14), 0.0);
    assert_eq!(m.master, [0.0, 0.0]);
    e.stop();
}

// (2) Route-to groups: kick + snare -> insert 14 -> insert 16 -> master, and the feedback
//     rule.

#[test]
fn route_groups_chain_through_higher_inserts_and_lower_routes_are_refused() {
    let direct = settle_and_play_bar(&mut drums_engine(true, true, MASTER));
    assert!(peak(&direct) > 0.1);
    let mut e = drums_engine(true, true, 14);
    assert_eq!(e.set_insert(14, InsertParam::Route, 16.0), 16.0);
    assert_eq!(e.mixer().insert(14).route, 16);
    assert_eq!(e.mixer().insert(16).route, MASTER);
    let grouped = settle_and_play_bar(&mut e);
    assert_same(
        &grouped,
        &direct,
        "a chain of neutral inserts is bit-transparent",
    );
    // The group fader scales everything that flows through it.
    e.set_insert(16, InsertParam::Level, -6.0);
    let quiet = settle_and_play_bar(&mut e);
    let expected = peak(&direct) * 10.0_f32.powf(-6.0 / 20.0);
    assert!((peak(&quiet) - expected).abs() < expected * 0.01);
    // Muting the drums insert silences the group, muting the group silences the drums.
    e.set_insert(16, InsertParam::Level, 0.0);
    e.set_insert(14, InsertParam::Mute, 1.0);
    assert!(is_silent(&settle_and_play_bar(&mut e)));
    e.set_insert(14, InsertParam::Mute, 0.0);
    e.set_insert(16, InsertParam::Mute, 1.0);
    assert!(is_silent(&settle_and_play_bar(&mut e)));
    e.set_insert(16, InsertParam::Mute, 0.0);
    let back = settle_and_play_bar(&mut e);
    assert!(
        (rms(&back) - rms(&direct)).abs() < rms(&direct) * 0.02,
        "unmuted group"
    );

    // Feedback rule: a route to a lower-or-equal number (or past 16) is refused.
    assert_eq!(e.set_insert(16, InsertParam::Route, 14.0), 0.0);
    assert_eq!(e.mixer().insert(16).route, MASTER);
    assert_eq!(e.set_insert(14, InsertParam::Route, 14.0), 0.0);
    assert_eq!(e.set_insert(14, InsertParam::Route, 13.0), 0.0);
    assert_eq!(e.set_insert(14, InsertParam::Route, 17.0), 0.0);
    assert_eq!(e.set_insert(14, InsertParam::Route, 15.0), 15.0);
    assert_eq!(e.set_insert(15, InsertParam::Route, 16.0), 16.0);
    assert_eq!(e.set_insert(1, InsertParam::Route, 2.0), 2.0);
    assert_eq!(e.set_route(Channel::Kick, 17), 0);
    assert_eq!(e.set_route(Channel::Kick, 16), 16);
    assert_eq!(e.set_route(Channel::Kick, 0), 0);
    // A hand-edited state with a backwards route loads with that route refused.
    let mut state = *e.mixer();
    state.inserts[13].route = 2;
    state.inserts[15].route = 16;
    state.routes[0] = 99;
    e.set_mixer_state(&state);
    assert_eq!(e.mixer().insert(14).route, MASTER);
    assert_eq!(e.mixer().insert(16).route, MASTER);
    assert_eq!(e.mixer().route(Channel::Kick), MASTER);
    // Through the chain 14 -> 15 -> 16 -> master (on a fresh engine, so the kick's click
    // noise is the same) the result is still the plain sum, bit for bit.
    let mut f = drums_engine(true, true, 14);
    f.set_insert(14, InsertParam::Route, 15.0);
    f.set_insert(15, InsertParam::Route, 16.0);
    let chained = settle_and_play_bar(&mut f);
    assert_same(&chained, &direct, "14 -> 15 -> 16 -> master");
    assert!(all_finite(&chained));
}

// (3) The compressor slot reduces the peak of a loud burst, leaves a quiet signal untouched
//     and reports its gain reduction.

#[test]
fn compressor_slot_reduces_a_loud_tone_leaves_a_quiet_one_and_meters_gain_reduction() {
    let loud_dry = settle_and_play_bar(&mut tone_engine(45, 1.0));
    let in_peak = peak(&steady(&loud_dry));
    let in_db = db(in_peak);
    assert!(in_db > -15.0, "the dry tone must be loud: {in_db} dBFS");
    let threshold = in_db - 15.0;

    let mut e = tone_engine(45, 1.0);
    e.set_slot_kind(6, 0, SlotKind::Compressor);
    assert_eq!(e.mixer().insert(6).slot(0).kind, SlotKind::Compressor);
    assert_eq!(e.set_slot_param(6, 0, 0, threshold), threshold);
    assert_eq!(e.set_slot_param(6, 0, 1, 4.0), 4.0);
    assert_eq!(e.set_slot_param(6, 0, 2, 1.0), 1.0, "attack 1 ms");
    let loud_wet = settle_and_play_bar(&mut e);
    assert!(all_finite(&loud_wet));
    let out_db = db(peak(&steady(&loud_wet)));
    // 15 dB over the threshold at 4:1 leaves 3.75 dB over it.
    let expected = threshold + 15.0 / 4.0;
    assert!(
        (out_db - expected).abs() < 1.0,
        "compressed peak {out_db} dBFS, expected about {expected} (input {in_db})"
    );
    // The GR meter reports about 11.25 dB during the sustained part.
    let _ = render(&mut e, ms_to_frames(200.0));
    e.play();
    let _ = render(&mut e, ms_to_frames(400.0));
    let _ = render(&mut e, 512);
    let gr = e.meters().gr_of_insert(6);
    assert!((gr - 11.25).abs() < 1.0, "gain reduction meter {gr} dB");
    assert_eq!(e.meters().gr_of_insert(5), 0.0);
    assert!(e.meters().of_insert(6) > 0.0);
    e.stop();

    // A tone 30 dB under the loud one sits well below the threshold: untouched, bit for bit.
    let quiet_dry = settle_and_play_bar(&mut tone_engine(45, 0.03));
    let quiet_db = db(peak(&steady(&quiet_dry)));
    assert!(quiet_db < threshold - 3.0, "quiet tone at {quiet_db} dBFS");
    let mut q = tone_engine(45, 0.03);
    q.set_slot_kind(6, 0, SlotKind::Compressor);
    q.set_slot_param(6, 0, 0, threshold);
    let quiet_wet = settle_and_play_bar(&mut q);
    assert_same(
        &quiet_wet,
        &quiet_dry,
        "a quiet tone through the compressor",
    );
    let _ = render(&mut q, ms_to_frames(200.0));
    q.play();
    let _ = render(&mut q, ms_to_frames(400.0));
    assert_eq!(q.meters().gr_of_insert(6), 0.0);
}

// (4) EQ3: the low shelf cuts a 100 Hz tone and leaves 2 kHz alone; the high shelf cuts
//     8 kHz and leaves 500 Hz; the slot EQ3 works like the built-in one.

/// RMS gain in dB of the steady part of a tone at MIDI `pitch` through insert 6 with
/// `setup` applied, relative to the dry render.
fn tone_gain_db(pitch: u8, setup: impl Fn(&mut Engine)) -> f32 {
    let dry = settle_and_play_bar(&mut tone_engine(pitch, 0.5));
    let mut e = tone_engine(pitch, 0.5);
    setup(&mut e);
    let wet = settle_and_play_bar(&mut e);
    assert!(all_finite(&wet));
    db(rms(&steady(&wet)) / rms(&steady(&dry)))
}

#[test]
fn eq3_shelves_cut_lows_and_highs_on_the_insert() {
    let low_shelf = Eq3State::new(-15.0, 0.0, 1000.0, 0.0);
    let high_shelf = Eq3State::new(0.0, 0.0, 1000.0, -15.0);
    // MIDI 43 = 98 Hz, 95 = 1976 Hz, 119 = 7902 Hz, 71 = 523 Hz.
    let hz = |note: u8| 440.0 * 2.0_f32.powf((f32::from(note) - 69.0) / 12.0);
    let set_low = |e: &mut Engine| {
        assert_eq!(e.set_insert(6, InsertParam::Eq3Low, -15.0), -15.0);
    };
    let low = tone_gain_db(43, set_low);
    let expected = low_shelf.response_db(hz(43), SAMPLE_RATE);
    assert!(
        (low - expected).abs() < 0.5,
        "98 Hz: {low} dB, analytic {expected}"
    );
    assert!(low < -10.0, "98 Hz must drop by more than 10 dB: {low}");
    let mid = tone_gain_db(95, set_low);
    assert!(mid.abs() < 1.0, "2 kHz through the low shelf: {mid} dB");

    let set_high = |e: &mut Engine| {
        assert_eq!(e.set_insert(6, InsertParam::Eq3High, -15.0), -15.0);
    };
    let high = tone_gain_db(119, set_high);
    let expected = high_shelf.response_db(hz(119), SAMPLE_RATE);
    assert!(
        (high - expected).abs() < 0.5,
        "7.9 kHz: {high} dB, analytic {expected}"
    );
    assert!(high < -10.0, "7.9 kHz must drop by more than 10 dB: {high}");
    let low_through_high = tone_gain_db(71, set_high);
    assert!(
        low_through_high.abs() < 1.0,
        "523 Hz through the high shelf: {low_through_high} dB"
    );

    // The extra EQ3 slot does the same job, and the state reads back.
    let slot_low = tone_gain_db(43, |e| {
        e.set_slot_kind(6, 2, SlotKind::Eq3);
        assert_eq!(e.set_slot_param(6, 2, 0, -15.0), -15.0);
        assert_eq!(e.mixer().insert(6).slot(2).get(0), -15.0);
        assert_eq!(e.mixer().insert(6).slot(2).get(2), 1000.0);
    });
    assert!(
        (slot_low - low).abs() < 0.2,
        "slot {slot_low} vs built-in {low}"
    );
    // Flat EQ values are bit-transparent (the built-in one at the defaults).
    let dry = settle_and_play_bar(&mut tone_engine(60, 0.5));
    let mut e = tone_engine(60, 0.5);
    e.set_insert(6, InsertParam::Eq3MidFreq, 2500.0);
    assert_eq!(e.mixer().insert(6).eq3.mid_freq_hz, 2500.0);
    assert_same(&settle_and_play_bar(&mut e), &dry, "flat EQ3");
}

// (5) Slot order matters: drive before lo-fi differs from lo-fi before drive.

#[test]
fn slot_order_matters() {
    let dry = settle_and_play_bar(&mut tone_engine(45, 0.8));
    let mut a = tone_engine(45, 0.8);
    a.set_slot_kind(6, 0, SlotKind::Drive);
    a.set_slot_param(6, 0, 0, 1.0);
    a.set_slot_kind(6, 1, SlotKind::Lofi);
    a.set_slot_param(6, 1, 0, 5.0);
    let drive_then_lofi = settle_and_play_bar(&mut a);
    let mut b = tone_engine(45, 0.8);
    b.set_slot_kind(6, 0, SlotKind::Lofi);
    b.set_slot_param(6, 0, 0, 5.0);
    b.set_slot_kind(6, 1, SlotKind::Drive);
    b.set_slot_param(6, 1, 0, 1.0);
    let lofi_then_drive = settle_and_play_bar(&mut b);
    assert!(all_finite(&drive_then_lofi) && all_finite(&lofi_then_drive));
    assert!(rms(&steady(&drive_then_lofi)) > 0.01);
    assert!(rms(&steady(&lofi_then_drive)) > 0.01);
    assert!(
        differs(&drive_then_lofi, &dry, 1e-3),
        "the drive and lo-fi change the tone"
    );
    assert!(differs(&lofi_then_drive, &dry, 1e-3));
    assert!(
        differs(&steady(&drive_then_lofi), &steady(&lofi_then_drive), 1e-3),
        "slot order made no difference"
    );
    // Chorus slot: a mono tone becomes stereo (left and right differ) and keeps its level.
    let mut c = tone_engine(45, 0.8);
    c.set_slot_kind(6, 0, SlotKind::Chorus);
    let chorused = settle_and_play_bar(&mut c);
    assert!(all_finite(&chorused));
    assert!(differs(&left(&chorused), &right(&chorused), 1e-3));
    let ratio = rms(&steady(&chorused)) / rms(&steady(&dry));
    assert!((0.5..2.0).contains(&ratio), "chorus level ratio {ratio}");
    // Switching a slot back off restores the dry render exactly.
    c.set_slot_kind(6, 0, SlotKind::Off);
    assert_same(&settle_and_play_bar(&mut c), &dry, "slot switched off");
}

// (6) Neutral inserts are bit-transparent: every routing of the demo through neutral
//     inserts renders exactly like the straight-to-master mix.

#[test]
fn neutral_inserts_are_bit_transparent() {
    fn demo(routes: impl Fn(&mut Engine)) -> Vec<f32> {
        let (_h, mut e) = new_engine(SAMPLE_RATE);
        let mut project = Project::default();
        // Dry returns (the reverb state is authoritative for its mix), so the insert
        // sends, which the master route bypasses, do not matter.
        project.set_param(ParamId::DelayMix, 0.0);
        project.reverb.mix = 0.0;
        project.sync_reverb_params();
        project.apply_to(&mut e);
        routes(&mut e);
        let _settle = render(&mut e, ms_to_frames(200.0));
        e.play();
        render_in_blocks(&mut e, BAR, 512)
    }
    let direct = demo(|e| {
        for ch in Channel::ALL {
            e.set_route(ch, MASTER);
        }
    });
    assert!(peak(&direct) > 0.1);
    let defaults = demo(|_| {});
    assert_same(&defaults, &direct, "one neutral insert per source");
    let one_bus = demo(|e| {
        for ch in Channel::ALL {
            e.set_route(ch, 16);
        }
    });
    assert_same(&one_bus, &direct, "everything on insert 16");
    let groups = demo(|e| {
        for ch in Channel::ALL {
            e.set_route(ch, if ch.drum_row().is_some() { 14 } else { 15 });
        }
        e.set_insert(14, InsertParam::Route, 16.0);
        e.set_insert(15, InsertParam::Route, 16.0);
    });
    assert_same(&groups, &direct, "drums and tracks grouped onto 16");
    assert!(InsertState::NEUTRAL.is_neutral());
}

// (7) A v7 project migrates (strips onto inserts, sources routed one-to-one) and renders
//     sample-identical to its pre-M11 render.

#[test]
fn v7_project_migrates_and_renders_sample_identical() {
    let project = Project::from_json(V7_DEMO_JSON).expect("the v7 fixture loads");
    assert!(V7_DEMO_JSON.contains("\"version\": 7"));
    assert!(!V7_DEMO_JSON.contains("\"inserts\""));
    assert_eq!(project.version, 8);
    let m = &project.mixer;
    for (i, ch) in Channel::ALL.iter().enumerate() {
        assert_eq!(
            m.strip(*ch),
            &Strip::for_channel(*ch),
            "{ch:?} strip is neutral"
        );
        assert_eq!(m.route(*ch), i as u8 + 1, "{ch:?} routed one-to-one");
        let insert = m.insert(i as u8 + 1);
        assert!(insert.is_neutral(), "insert {}", i + 1);
        assert_eq!(insert.route, MASTER);
    }
    // The v7 defaults: tracks sent fully, snare / clap reverb 0.3, drums otherwise dry.
    assert_eq!(m.insert(6).delay_send, 1.0);
    assert_eq!(m.insert(6).reverb_send, 1.0);
    assert_eq!(m.insert(13).delay_send, 1.0);
    assert_eq!(m.insert(2).reverb_send, 0.3);
    assert_eq!(m.insert(3).reverb_send, 0.3);
    assert_eq!(m.insert(1).reverb_send, 0.0);
    assert_eq!(m.insert(14), &InsertState::default_for(14));
    // Re-saved it is a version-8 file with inserts and routes, and loads back equal.
    let json = project.to_json_pretty();
    assert!(json.starts_with("{\n  \"version\": 8,"));
    assert!(json.contains("\"inserts\": [") && json.contains("\"routes\": ["));
    assert_eq!(Project::from_json(&json).unwrap(), project);

    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    project.apply_to(&mut engine);
    engine.play();
    let out = render_in_blocks(&mut engine, SAMPLE_RATE as usize, 512);
    assert!(all_finite(&out));
    let l = left(&out);
    for (i, w) in common::per_window(&l, 4_800, <[f32]>::to_vec)
        .iter()
        .enumerate()
    {
        let got = rms(w);
        assert!(
            (got - V7_REFERENCE_RMS[i]).abs() < 1e-6,
            "window {i}: rms {got}, reference {}",
            V7_REFERENCE_RMS[i]
        );
    }
    assert_eq!(
        fnv1a(&out),
        V7_REFERENCE_HASH,
        "the v7 demo must render bit for bit as before M11"
    );
    // A migrated mix: a v7 file with the kick at -6 dB and the synth strip hard left
    // carries those onto inserts 1 and 6 (the strips stay neutral).
    let mut v7: serde_json::Value = serde_json::from_str(V7_DEMO_JSON).unwrap();
    v7["mixer"]["strips"][0]["level_db"] = serde_json::json!(-6.0);
    v7["mixer"]["strips"][5]["pan"] = serde_json::json!(-1.0);
    v7["mixer"]["strips"][5]["delay_send"] = serde_json::json!(0.4);
    let p = Project::from_json(&v7.to_string()).unwrap();
    assert_eq!(p.mixer.insert(1).level_db, -6.0);
    assert_eq!(p.mixer.insert(6).pan, -1.0);
    assert_eq!(p.mixer.insert(6).delay_send, 0.4);
    assert_eq!(p.mixer.strip(Channel::Kick).level_db, 0.0);
    assert_eq!(p.mixer.strip(Channel::Track(0)).pan, 0.0);
}

// (8) Solo on an insert silences the others; source solo and insert solo both apply.

#[test]
fn insert_solo_silences_the_other_inserts() {
    let kick_alone = settle_and_play_bar(&mut drums_engine(true, false, MASTER));
    let mut e = dry_engine();
    e.set_drum_step(DrumRow::Kick, 0, true, true);
    e.set_drum_step(DrumRow::Snare, 4, true, true);
    assert_eq!(e.mixer().route(Channel::Kick), 1);
    assert_eq!(e.mixer().route(Channel::Snare), 2);
    e.set_insert(2, InsertParam::Solo, 1.0);
    assert!(e.mixer().insert(2).solo);
    assert!(e.mixer().any_insert_solo());
    let out = left(&settle_and_play_bar(&mut e));
    assert!(
        is_silent(&out[..4 * STEP - 100]),
        "soloing the snare insert must silence the kick insert: {}",
        peak(&out[..4 * STEP - 100])
    );
    assert!(peak(&out[4 * STEP..8 * STEP]) > 0.1);
    // Soloing the kick insert as well lets both through.
    e.set_insert(1, InsertParam::Solo, 1.0);
    let out = left(&settle_and_play_bar(&mut e));
    assert!(peak(&out[..4 * STEP - 100]) > 0.1);
    assert!(peak(&out[4 * STEP..8 * STEP]) > 0.1);
    // Mute beats solo on the insert: only the kick's last decaying samples (below -80 dBFS)
    // are left where the snare was.
    e.set_insert(2, InsertParam::Mute, 1.0);
    let out = left(&settle_and_play_bar(&mut e));
    assert!(peak(&out[..4 * STEP - 100]) > 0.1);
    assert!(
        peak(&out[4 * STEP..5 * STEP]) < 1e-3,
        "muted soloed snare insert still sounds: {}",
        peak(&out[4 * STEP..5 * STEP])
    );
    assert!(peak(&kick_alone) > 0.1);
    e.set_insert(2, InsertParam::Mute, 0.0);
    // Both layers apply: a source solo on the kick plus the insert solos leaves only the kick.
    e.set_mixer(Channel::Kick, ChannelParam::Solo, 1.0);
    let out = left(&settle_and_play_bar(&mut e));
    assert!(peak(&out[..4 * STEP - 100]) > 0.1);
    assert!(
        peak(&out[4 * STEP..5 * STEP]) < 1e-3,
        "source solo on the kick must silence the snare: {}",
        peak(&out[4 * STEP..5 * STEP])
    );
    // Clearing every solo restores both.
    e.set_mixer(Channel::Kick, ChannelParam::Solo, 0.0);
    e.set_insert(1, InsertParam::Solo, 0.0);
    e.set_insert(2, InsertParam::Solo, 0.0);
    assert!(!e.mixer().any_insert_solo());
    let out = left(&settle_and_play_bar(&mut e));
    assert!(peak(&out[..4 * STEP - 100]) > 0.1);
    assert!(peak(&out[4 * STEP..8 * STEP]) > 0.1);
    // Solo toggles are smoothed: no step larger than the signal's own.
    let (_h, mut s) = seq_engine();
    s.note_on(45, 1.0);
    let _ = render(&mut s, ms_to_frames(100.0));
    let steady_tone = left(&render(&mut s, ms_to_frames(100.0)));
    let step = |buf: &[f32]| {
        buf.windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max)
    };
    s.set_insert(1, InsertParam::Solo, 1.0);
    let fade = left(&render(&mut s, ms_to_frames(150.0)));
    assert!(step(&fade) <= step(&steady_tone) * 2.0, "solo clicked");
    assert!(is_silent(&fade[ms_to_frames(100.0)..]));
}

// (9) Insert sends feed the returns post-fader; the messages apply and wrap.

#[test]
fn insert_sends_are_post_fader_and_messages_apply() {
    fn reverb_tail(setup: impl Fn(&mut Engine)) -> f32 {
        let (_h, mut e) = new_engine(SAMPLE_RATE);
        common::dry_fx(&mut e);
        e.set_param(ParamId::ReverbMix, 1.0);
        e.set_param(ParamId::ReverbSize, 0.8);
        e.set_drum_step(DrumRow::Kick, 0, true, true);
        setup(&mut e);
        let _ = settle_and_play_bar(&mut e);
        peak(&render(&mut e, ms_to_frames(2000.0)))
    }
    assert_eq!(
        reverb_tail(|_| {}),
        0.0,
        "insert 1 sends nothing by default"
    );
    let wet = reverb_tail(|e| {
        e.set_insert(1, InsertParam::ReverbSend, 1.0);
    });
    assert!(wet > 1e-4, "insert reverb send leaves a tail: {wet}");
    let muted = reverb_tail(|e| {
        e.set_insert(1, InsertParam::ReverbSend, 1.0);
        e.set_insert(1, InsertParam::Mute, 1.0);
    });
    assert_eq!(
        muted, 0.0,
        "sends are post-fader: a muted insert sends nothing"
    );
    let grouped = reverb_tail(|e| {
        e.set_insert(1, InsertParam::Route, 14.0);
        e.set_insert(14, InsertParam::ReverbSend, 1.0);
    });
    assert!(grouped > 1e-4, "a group insert's send works too: {grouped}");

    // Messages.
    let (mut handle, mut e) = new_engine(SAMPLE_RATE);
    assert!(handle.send(Message::SetInsert {
        insert: 14,
        param: InsertParam::Level,
        value: -3.0
    }));
    assert!(handle.send(Message::SetSlotKind {
        insert: 14,
        slot: 1,
        kind: SlotKind::Chorus
    }));
    assert!(handle.send(Message::SetSlotParam {
        insert: 14,
        slot: 1,
        index: 0,
        value: 2.0
    }));
    assert!(handle.send(Message::SetSlotParam {
        insert: 14,
        slot: 1,
        index: 9,
        value: 2.0
    }));
    assert!(handle.send(Message::SetRoute {
        channel: Channel::Clap,
        insert: 14
    }));
    assert!(handle.send(Message::SetInsert {
        insert: 14,
        param: InsertParam::Route,
        value: 16.0
    }));
    assert!(handle.send(Message::SetInsert {
        insert: 0,
        param: InsertParam::Pan,
        value: 0.5
    }));
    assert!(handle.send(Message::SetSlotKind {
        insert: 17,
        slot: 3,
        kind: SlotKind::Drive
    }));
    let _ = render(&mut e, 64);
    let m = e.mixer();
    assert_eq!(m.insert(14).level_db, -3.0);
    assert_eq!(m.insert(14).slot(1).kind, SlotKind::Chorus);
    assert_eq!(m.insert(14).slot(1).get(0), 2.0);
    assert_eq!(m.insert(14).slot(1).get(1), 0.3, "the chorus defaults");
    assert_eq!(m.route(Channel::Clap), 14);
    assert_eq!(m.insert(14).route, 16);
    assert_eq!(m.insert(16).pan, 0.5, "insert 0 wraps onto 16");
    assert_eq!(
        m.insert(1).slot(0).kind,
        SlotKind::Drive,
        "17 / slot 3 wrap"
    );
    assert_eq!(m.insert(1).slot(0), &SlotState::new(SlotKind::Drive));
    // The handle reads the insert meters the engine published.
    e.set_drum_step(DrumRow::Clap, 0, true, true);
    e.play();
    let _ = render(&mut e, 512);
    let meters = handle.meters();
    assert_eq!(meters, e.meters());
    assert!(meters.of_insert(14) > 0.0, "{meters:?}");
    assert!(meters.of_insert(16) > 0.0, "the group hears the clap");
    assert_eq!(meters.of_insert(3), 0.0, "the clap left its default insert");
    assert_eq!(meters.gr_of_insert(14), 0.0);
    // `set_mixer_state` carries slots into the DSP: a compressor loaded from a state meters
    // gain reduction straight away.
    let mut state = MixerState::default();
    state.set_slot_kind(3, 0, SlotKind::Compressor);
    state.set_slot_param(3, 0, 0, -60.0);
    state.set_slot_param(3, 0, 2, 0.1);
    state.set_route(Channel::Clap, 3);
    e.set_mixer_state(&state);
    e.play();
    let _ = render(&mut e, 512);
    assert!(
        e.meters().gr_of_insert(3) > 3.0,
        "{:?}",
        e.meters().insert_gr
    );
}

// (10) Perf gate: eight tracks through compressors on inserts 6..=13 (plus an EQ3 and a
//      chorus slot each) render well under real time. Release mode only
//      (`cargo test --release -p engine -- --ignored`).

#[test]
#[ignore]
fn eight_inserts_with_compressors_render_faster_than_real_time() {
    let (_handle, mut e) = new_engine(SAMPLE_RATE);
    for t in 0..MAX_TRACKS as u8 {
        e.set_track_active(t, true);
        presets::PAD.apply_to_track(&mut e, t);
        e.set_track_param(t, ParamId::Osc1Unison, 7.0);
        for i in 0..4u8 {
            e.set_note(0, t, i, Note::new(0, 16, 48 + 3 * i + t, 0.9));
        }
        let insert = 6 + t;
        e.set_slot_kind(insert, 0, SlotKind::Compressor);
        e.set_slot_param(insert, 0, 0, -30.0);
        e.set_slot_kind(insert, 1, SlotKind::Eq3);
        e.set_slot_param(insert, 1, 0, 3.0);
        e.set_slot_kind(insert, 2, SlotKind::Chorus);
        e.set_insert(insert, InsertParam::Eq3High, -2.0);
        e.set_insert(insert, InsertParam::Route, 16.0);
    }
    e.set_slot_kind(16, 0, SlotKind::Compressor);
    e.set_drum_step(DrumRow::Kick, 0, true, true);
    e.set_drum_step(DrumRow::Kick, 8, true, true);
    e.play();
    let frames = SAMPLE_RATE as usize;
    let mut out = vec![0.0_f32; frames * 2];
    let start = std::time::Instant::now();
    for chunk in out.chunks_mut(512 * 2) {
        let n = chunk.len() / 2;
        e.render(chunk, n);
    }
    let elapsed = start.elapsed();
    assert!(all_finite(&out));
    assert!(rms(&left(&out)) > 0.01, "the chords must sound");
    assert!(e.meters().gr_of_insert(16) >= 0.0);
    assert!(
        elapsed.as_secs_f32() < 0.5,
        "1 s of 8 tracks through compressed inserts took {elapsed:?} (must be under 0.5 s in release)"
    );
    assert_eq!(INSERT_COUNT, 16);
}
