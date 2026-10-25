//! M10 instrument tracks: the v6 regression (one track renders bit for bit like the pre-M10
//! synth), the v8 layered regression (two tracks with non-default insert settings pinned
//! bit for bit), track independence (voices and inserts), the edit track, lifecycle
//! messages, the v7 project format, the allocation-free render with eight tracks and the
//! release-mode perf gate.

mod common;

use common::{SAMPLE_RATE, all_finite, is_silent, left, render, render_in_blocks, rms, seq_engine};
use engine::presets;
use engine::{
    Channel, ChannelParam, DrumRow, Engine, MAX_TRACKS, MAX_VOICES, Message, Note, ParamId,
    Project, TrackState, new_engine,
};

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

/// Reference render of `tests/fixtures/v6_demo.json` (today's `Project::default()` as a v6
/// file) with the pre-M10 engine: `play()`, 1 s at 48 kHz in blocks of 512. Captured before
/// the track refactor; the render below must reproduce it exactly.
const V6_REFERENCE_HASH: u64 = 0xd016_a4bb_b4c2_4052;
/// RMS of each 100 ms window of the left channel of that render.
const V6_REFERENCE_RMS: [f32; 10] = [
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

const V6_DEMO_JSON: &str = include_str!("fixtures/v6_demo.json");

// (1) The v6 regression: a pre-M10 project migrates into track 0 and renders sample-identical.

#[test]
fn v6_project_renders_sample_identical_after_the_track_refactor() {
    let project = Project::from_json(V6_DEMO_JSON).expect("the v6 fixture loads");
    assert_eq!(project.tracks.len(), 1);
    assert_eq!(project.tracks[0].slot, 0);
    assert_eq!(project.tracks[0].name, "Synth");
    assert_eq!(project.tracks[0].preset, presets::ALL[0].name);
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    project.apply_to(&mut engine);
    assert!(engine.track_active(0));
    assert_eq!(engine.active_track_count(), 1);
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
            (got - V6_REFERENCE_RMS[i]).abs() < 1e-6,
            "window {i}: rms {got}, reference {}",
            V6_REFERENCE_RMS[i]
        );
    }
    assert_eq!(
        fnv1a(&out),
        V6_REFERENCE_HASH,
        "the v6 demo must render bit for bit as before M10"
    );
}

// (1b) The v8 layered regression: two tracks with non-default insert and synth settings
//      (Keys through lo-fi bits 6 / rate 3 / mix 0.8 and a chorus; Lead with drive 0.9 /
//      mix 0.7 and sidechain 0.7 pumped by a kick) render sample-identical, so a refactor of
//      the insert ordering (`drive -> lo-fi -> chorus -> x ducker`) or of the parameter
//      smoothing is caught.

/// Reference render of `tests/fixtures/v8_layered_demo.json` (built with the M11 engine from
/// `Project::from_engine`): `play()`, 1 s at 48 kHz in blocks of 512.
const V8_REFERENCE_HASH: u64 = 0xf7dc_466d_03a9_6ff6;
/// RMS of each 100 ms window of the left channel of that render.
const V8_REFERENCE_RMS: [f32; 10] = [
    0.459_317,
    0.548_292_4,
    0.594_854_2,
    0.569_408_95,
    0.551_517_07,
    0.322_251_47,
    0.159_717_02,
    0.434_829_47,
    0.614_625,
    0.614_486_6,
];

const V8_LAYERED_JSON: &str = include_str!("fixtures/v8_layered_demo.json");

#[test]
fn v8_layered_project_renders_sample_identical() {
    let project = Project::from_json(V8_LAYERED_JSON).expect("the v8 fixture loads");
    assert_eq!(project.tracks.len(), 2);
    let keys = project.track(0).unwrap();
    let lead = project.track(1).unwrap();
    assert_eq!((keys.name.as_str(), keys.preset.as_str()), ("Keys", "Keys"));
    assert_eq!((lead.name.as_str(), lead.preset.as_str()), ("Lead", "Lead"));
    // The fixture really carries the non-default insert settings it is meant to pin.
    assert_eq!(keys.param(ParamId::LofiBits), 6.0);
    assert_eq!(keys.param(ParamId::LofiRate), 3.0);
    assert_eq!(keys.param(ParamId::LofiMix), 0.8);
    assert_eq!(keys.param(ParamId::ChorusMix), 0.6);
    assert_eq!(lead.param(ParamId::DriveAmount), 0.9);
    assert_eq!(lead.param(ParamId::DriveMix), 0.7);
    assert_eq!(lead.param(ParamId::SidechainAmount), 0.7);
    assert!(project.bank.pattern(0).drums[DrumRow::Kick.index()][0].on);
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    project.apply_to(&mut engine);
    assert_eq!(engine.active_track_count(), 2);
    assert_eq!(engine.track(0).insert.param(ParamId::LofiBits), 6.0);
    assert_eq!(engine.track(1).insert.ducker().amount(), 0.7);
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
            (got - V8_REFERENCE_RMS[i]).abs() < 1e-6,
            "window {i}: rms {got}, reference {}",
            V8_REFERENCE_RMS[i]
        );
    }
    assert_eq!(
        fnv1a(&out),
        V8_REFERENCE_HASH,
        "the v8 layered demo must render bit for bit as when it was pinned"
    );
}

// --- helpers --------------------------------------------------------------------------------

/// A dry engine (no delay/reverb returns, limiter off, master gain 1) whose drum rows are
/// muted, so only the tracks are heard.
fn tracks_engine() -> Engine {
    let (_handle, mut engine) = seq_engine();
    engine.set_param(ParamId::LimiterOn, 0.0);
    engine.set_param(ParamId::MasterGain, 1.0);
    for row in DrumRow::ALL {
        engine.set_mixer(Channel::from_row(row), ChannelParam::Mute, 1.0);
    }
    engine
}

/// Applies `preset` to `track` and makes the track dry (no drive), so renders add linearly.
fn dry_preset(engine: &mut Engine, track: u8, preset: &presets::Preset) {
    engine.set_track_active(track, true);
    preset.apply_to_track(engine, track);
    engine.set_track_param(track, ParamId::DriveMix, 0.0);
    engine.set_track_param(track, ParamId::LofiMix, 0.0);
    engine.set_track_param(track, ParamId::ChorusMix, 0.0);
    engine.set_track_param(track, ParamId::SidechainAmount, 0.0);
    engine.set_track_param(track, ParamId::AmpRelease, 0.05);
}

/// Switches `track`'s lo-fi (bits 6, rate 3, mix 0.8) and chorus (mix 0.5) on, after
/// [`dry_preset`] made it dry: the insert now shapes the sound.
fn wet_insert(engine: &mut Engine, track: u8) {
    engine.set_track_param(track, ParamId::LofiBits, 6.0);
    engine.set_track_param(track, ParamId::LofiRate, 3.0);
    engine.set_track_param(track, ParamId::LofiMix, 0.8);
    engine.set_track_param(track, ParamId::ChorusMix, 0.5);
}

/// Puts a four-note chord of `pitch` on step 0 (length 8) of pattern A for `track`.
fn chord(engine: &mut Engine, track: u8, pitch: u8) {
    for (i, p) in [pitch, pitch + 4, pitch + 7, pitch + 11].iter().enumerate() {
        engine.set_note(0, track, i as u8, Note::new(0, 8, *p, 0.9));
    }
}

/// Renders one second from `play` after a 200 ms stopped settle.
fn settle_play_render(engine: &mut Engine) -> Vec<f32> {
    let _ = render(engine, 9_600);
    engine.play();
    render(engine, SAMPLE_RATE as usize)
}

// (2) Two tracks with different presets render as the sum of each alone.

#[test]
fn two_tracks_render_as_the_sum_of_each_alone() {
    let both = {
        let mut e = tracks_engine();
        dry_preset(&mut e, 0, &presets::PAD);
        dry_preset(&mut e, 1, &presets::KEYS);
        chord(&mut e, 0, 48);
        chord(&mut e, 1, 67);
        settle_play_render(&mut e)
    };
    let pad = {
        let mut e = tracks_engine();
        dry_preset(&mut e, 0, &presets::PAD);
        chord(&mut e, 0, 48);
        settle_play_render(&mut e)
    };
    // Keys alone on the same slot as in the combined render (slot 0 stays active and
    // silent: an idle track contributes exactly zero).
    let keys = {
        let mut e = tracks_engine();
        dry_preset(&mut e, 1, &presets::KEYS);
        chord(&mut e, 1, 67);
        settle_play_render(&mut e)
    };
    assert!(all_finite(&both));
    assert!(rms(&pad) > 0.01 && rms(&keys) > 0.01, "both parts sound");
    let mut worst = 0.0_f32;
    for i in 0..both.len() {
        worst = worst.max((both[i] - (pad[i] + keys[i])).abs());
    }
    assert!(worst < 1e-5, "max deviation from the sum: {worst}");
}

// (2b) Insert isolation: track 2 through a crushed lo-fi and a chorus while track 1 stays
//      dry. The sum of each alone still equals both together (one track's insert never sees
//      the other's signal), and muting the wet track leaves the dry one bit-identical.

#[test]
fn wet_insert_on_track_two_keeps_the_tracks_additive_and_isolated() {
    let both = {
        let mut e = tracks_engine();
        dry_preset(&mut e, 0, &presets::PAD);
        dry_preset(&mut e, 1, &presets::KEYS);
        wet_insert(&mut e, 1);
        chord(&mut e, 0, 48);
        chord(&mut e, 1, 67);
        settle_play_render(&mut e)
    };
    let pad = {
        let mut e = tracks_engine();
        dry_preset(&mut e, 0, &presets::PAD);
        chord(&mut e, 0, 48);
        settle_play_render(&mut e)
    };
    let keys_wet = {
        let mut e = tracks_engine();
        dry_preset(&mut e, 1, &presets::KEYS);
        wet_insert(&mut e, 1);
        chord(&mut e, 1, 67);
        settle_play_render(&mut e)
    };
    let keys_dry = {
        let mut e = tracks_engine();
        dry_preset(&mut e, 1, &presets::KEYS);
        chord(&mut e, 1, 67);
        settle_play_render(&mut e)
    };
    assert!(all_finite(&both));
    assert!(
        rms(&pad) > 0.01 && rms(&keys_wet) > 0.01,
        "both parts sound"
    );
    let insert_effect = keys_wet
        .iter()
        .zip(&keys_dry)
        .map(|(w, d)| (w - d).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        insert_effect > 1e-3,
        "the lo-fi / chorus insert must change track 2's sound: {insert_effect}"
    );
    let mut worst = 0.0_f32;
    for i in 0..both.len() {
        worst = worst.max((both[i] - (pad[i] + keys_wet[i])).abs());
    }
    assert!(worst < 1e-5, "max deviation from the sum: {worst}");

    // Muting the wet track's channel leaves track 1 bit-identical; its insert stays set.
    let mut e = tracks_engine();
    dry_preset(&mut e, 0, &presets::PAD);
    dry_preset(&mut e, 1, &presets::KEYS);
    wet_insert(&mut e, 1);
    chord(&mut e, 0, 48);
    chord(&mut e, 1, 67);
    e.set_mixer(Channel::Track(1), ChannelParam::Mute, 1.0);
    let muted = settle_play_render(&mut e);
    assert_eq!(
        muted, pad,
        "track 1 must not change when the wet track 2 is muted"
    );
    assert_eq!(e.track(1).insert.param(ParamId::LofiMix), 0.8);
    assert_eq!(e.track(1).insert.param(ParamId::ChorusMix), 0.5);
}

// (3) Muting track 2's channel leaves track 1 bit-identical; notes on track 2 never trigger
//     track 1's voices.

#[test]
fn muting_track_two_leaves_track_one_bit_identical() {
    let alone = {
        let mut e = tracks_engine();
        dry_preset(&mut e, 0, &presets::PAD);
        chord(&mut e, 0, 48);
        settle_play_render(&mut e)
    };
    let mut e = tracks_engine();
    dry_preset(&mut e, 0, &presets::PAD);
    dry_preset(&mut e, 1, &presets::LEAD);
    chord(&mut e, 0, 48);
    chord(&mut e, 1, 72);
    e.set_mixer(Channel::Track(1), ChannelParam::Mute, 1.0);
    let muted = settle_play_render(&mut e);
    assert_eq!(
        muted, alone,
        "track 1 must not change when track 2 is muted"
    );
    // The muted track still plays its notes (its channel is silent, its voices are not).
    e.play();
    let _ = render(&mut e, 4_800);
    assert_eq!(e.track_voices(0).held_voices(), 4);
    assert_eq!(e.track_voices(1).held_voices(), 4);
    assert_eq!(
        e.meters().of(Channel::Track(1)),
        0.0,
        "post-fader peak of a muted strip"
    );
    assert!(e.meters().of(Channel::Track(0)) > 0.0);
}

#[test]
fn notes_on_track_two_never_trigger_track_one_voices() {
    let mut e = tracks_engine();
    dry_preset(&mut e, 0, &presets::PAD);
    dry_preset(&mut e, 1, &presets::KEYS);
    chord(&mut e, 1, 60);
    e.play();
    let _ = render(&mut e, 4_800);
    assert_eq!(e.track_voices(0).active_voices(), 0);
    assert_eq!(e.track_voices(1).held_voices(), 4);
    // An inactive track's notes are never played either (there are none, by construction).
    assert!(e.bank().pattern(0).notes(2).is_empty());
    assert_eq!(e.track_voices(2).active_voices(), 0);
}

// (4) SetEditTrack routes NoteOn / NoteOff / SetParam and the lane messages.

#[test]
fn set_edit_track_routes_notes_and_params() {
    let (mut handle, mut e) = seq_engine();
    e.set_track_active(2, true);
    assert_eq!(e.edit_track(), 0);
    assert!(handle.send(Message::SetEditTrack(2)));
    assert!(handle.send(Message::NoteOn {
        note: 60,
        velocity: 1.0
    }));
    assert!(handle.send(Message::SetParam(ParamId::FilterCutoff, 777.0)));
    assert!(handle.send(Message::SetSynthStep {
        step: 3,
        note: Some(40),
        accent: false
    }));
    let _ = render(&mut e, 64);
    assert_eq!(e.edit_track(), 2);
    assert_eq!(e.track_voices(2).held_voices(), 1);
    assert_eq!(e.track_voices(0).active_voices(), 0);
    assert_eq!(e.track_param(2, ParamId::FilterCutoff), 777.0);
    assert_eq!(
        e.param(ParamId::FilterCutoff),
        777.0,
        "param() reads the edit track"
    );
    assert_eq!(
        e.track_param(0, ParamId::FilterCutoff),
        20_000.0,
        "track 0 keeps its own value"
    );
    assert_eq!(e.pattern().notes(2), &[Note::step(3, 40, false)]);
    assert!(e.pattern().notes(0).is_empty());
    assert_eq!(
        e.pattern().synth[3].note,
        Some(40),
        "the lane shows the edit track"
    );
    assert_eq!(e.pattern().lane_track, 2);
    // Switching back shows track 0's (empty) lane; the held key is released on track 2 even
    // though the edit track moved on.
    assert!(handle.send(Message::SetEditTrack(0)));
    assert!(handle.send(Message::NoteOff { note: 60 }));
    let _ = render(&mut e, 64);
    assert_eq!(e.pattern().synth[3].note, None);
    assert_eq!(e.track_voices(2).held_voices(), 0);
    assert!(e.track_voices(2).active_voices() <= 1, "releasing");
    // Explicit forms address any track regardless of the edit track.
    assert!(handle.send(Message::SetTrackParam {
        track: 2,
        id: ParamId::Osc1Mix,
        value: 0.25
    }));
    assert!(handle.send(Message::SetNote {
        pattern: 1,
        track: 2,
        index: 0,
        note: Note::new(0, 2, 50, 0.8)
    }));
    let _ = render(&mut e, 64);
    assert_eq!(e.track_param(2, ParamId::Osc1Mix), 0.25);
    assert_eq!(e.param(ParamId::Osc1Mix), 1.0, "edit track 0 unchanged");
    assert_eq!(e.bank().pattern(1).notes(2).len(), 1);
    assert!(handle.send(Message::ClearNotes {
        pattern: 1,
        track: 2
    }));
    let _ = render(&mut e, 64);
    assert!(e.bank().pattern(1).notes(2).is_empty());
    // A NoteOn on an inactive edit track is ignored.
    e.set_edit_track(5);
    e.note_on(60, 1.0);
    assert_eq!(e.track_voices(5).active_voices(), 0);
}

// (5) Lifecycle: SetTrackActive off clears notes in every pattern; CopyTrack copies
//     settings only; SwapTracks keeps notes with their track.

#[test]
fn set_track_active_off_clears_its_notes_in_every_pattern() {
    let (mut handle, mut e) = seq_engine();
    e.set_track_active(3, true);
    e.set_track_param(3, ParamId::FilterCutoff, 500.0);
    e.set_mixer(Channel::Track(3), ChannelParam::Level, -6.0);
    for p in 0..engine::PATTERN_COUNT as u8 {
        e.set_note(p, 3, 0, Note::new(0, 4, 60, 0.8));
        e.set_note(p, 0, 0, Note::new(0, 4, 36, 0.8));
    }
    e.set_edit_track(3);
    e.note_on(72, 1.0);
    let _ = render(&mut e, 64);
    assert_eq!(e.track_voices(3).held_voices(), 1);
    assert!(handle.send(Message::SetTrackActive {
        track: 3,
        on: false
    }));
    let _ = render(&mut e, 64);
    assert!(!e.track_active(3));
    assert_eq!(e.track_voices(3).held_voices(), 0, "voices released");
    for p in 0..engine::PATTERN_COUNT as u8 {
        assert!(e.bank().pattern(p).notes(3).is_empty(), "pattern {p}");
        assert_eq!(e.bank().pattern(p).notes(0).len(), 1, "track 0 untouched");
    }
    assert_eq!(
        e.track_param(3, ParamId::FilterCutoff),
        ParamId::FilterCutoff.default_value(),
        "params reset"
    );
    assert_eq!(
        e.mixer().get(Channel::Track(3), ChannelParam::Level),
        -6.0,
        "the strip is kept"
    );
    // The release tail still renders, then the track goes quiet and costs nothing.
    let tail = render(&mut e, 48_000);
    assert!(all_finite(&tail));
    assert_eq!(e.track_voices(3).active_voices(), 0);
    assert!(is_silent(&render(&mut e, 480)));
}

#[test]
fn copy_track_clones_settings_but_not_notes() {
    let (mut handle, mut e) = seq_engine();
    e.set_track_param(0, ParamId::FilterCutoff, 640.0);
    e.set_track_param(0, ParamId::LofiMix, 0.4);
    e.set_track_param(0, ParamId::SidechainAmount, 0.7);
    e.set_note(0, 0, 0, Note::new(0, 4, 36, 0.8));
    e.set_mixer(Channel::Track(0), ChannelParam::Pan, -0.5);
    assert!(!e.track_active(4));
    assert!(handle.send(Message::CopyTrack { from: 0, to: 4 }));
    let _ = render(&mut e, 1);
    assert!(e.track_active(4));
    assert_eq!(e.track_params(4), e.track_params(0));
    assert_eq!(e.track(4).synth_params().cutoff, 640.0);
    assert_eq!(e.track(4).insert.param(ParamId::LofiMix), 0.4);
    assert_eq!(e.track(4).insert.ducker().amount(), 0.7);
    assert!(
        e.bank().pattern(0).notes(4).is_empty(),
        "notes are not cloned"
    );
    assert_eq!(
        e.mixer().get(Channel::Track(4), ChannelParam::Pan),
        0.0,
        "strip untouched"
    );
}

#[test]
fn swap_tracks_keeps_notes_with_their_track() {
    let (mut handle, mut e) = seq_engine();
    e.set_track_active(1, true);
    e.set_track_param(0, ParamId::FilterCutoff, 300.0);
    e.set_track_param(1, ParamId::FilterCutoff, 3_000.0);
    e.set_mixer(Channel::Track(0), ChannelParam::Level, -3.0);
    e.set_mixer(Channel::Track(1), ChannelParam::Level, -9.0);
    let a = Note::new(0, 4, 36, 0.8);
    let b = Note::new(2, 1, 72, 0.6);
    for p in 0..engine::PATTERN_COUNT as u8 {
        e.set_note(p, 0, 0, a);
        e.set_note(p, 1, 0, b);
    }
    e.set_edit_track(1);
    assert!(handle.send(Message::SwapTracks(0, 1)));
    let _ = render(&mut e, 1);
    for p in 0..engine::PATTERN_COUNT as u8 {
        assert_eq!(e.bank().pattern(p).notes(0), &[b], "pattern {p}");
        assert_eq!(e.bank().pattern(p).notes(1), &[a], "pattern {p}");
    }
    assert_eq!(e.track_param(0, ParamId::FilterCutoff), 3_000.0);
    assert_eq!(e.track_param(1, ParamId::FilterCutoff), 300.0);
    assert_eq!(e.mixer().get(Channel::Track(0), ChannelParam::Level), -9.0);
    assert_eq!(e.mixer().get(Channel::Track(1), ChannelParam::Level), -3.0);
    assert_eq!(e.edit_track(), 0, "the edit track follows its content");
    assert_eq!(
        e.pattern().synth[2].note,
        Some(72),
        "lane shows the moved track"
    );
    // Swapping with an inactive slot moves the activity flag too.
    e.swap_tracks(1, 6);
    assert!(!e.track_active(1) && e.track_active(6));
    assert_eq!(e.bank().pattern(0).notes(6), &[a]);
    assert!(e.bank().pattern(0).notes(1).is_empty());
    // The sequencer plays the swapped notes on the new slots.
    e.play();
    let _ = render(&mut e, 64);
    assert_eq!(e.track_voices(6).held_voices(), 1);
    assert_eq!(e.track_voices(0).held_voices(), 0, "b starts on step 2");
}

// (6) The v7 project round-trips eight tracks, and apply_to honours every slot.

#[test]
fn eight_track_project_round_trips_through_json() {
    let (_handle, mut e) = new_engine(SAMPLE_RATE);
    for t in 0..MAX_TRACKS as u8 {
        e.set_track_active(t, true);
        presets::ALL[usize::from(t) % presets::ALL.len()].apply_to_track(&mut e, t);
        e.set_track_param(t, ParamId::FilterCutoff, 400.0 + 100.0 * f32::from(t));
        e.set_note(0, t, 0, Note::new(t * 2, 2, 40 + t, 0.7));
        e.set_note(1, t, 0, Note::new(1, 1, 60 + t, 0.5));
        e.set_mixer(Channel::Track(t), ChannelParam::Level, -f32::from(t));
    }
    e.set_edit_track(5);
    let mut project = Project::from_engine(&e, 3, 0.6);
    assert_eq!(project.tracks.len(), MAX_TRACKS);
    for t in 0..MAX_TRACKS as u8 {
        let state = project.track_mut(t).unwrap();
        state.name = format!("Layer {t}");
        state.colour = t % 8;
        state.preset = presets::ALL[usize::from(t) % presets::ALL.len()]
            .name
            .to_owned();
    }
    let json = project.to_json_pretty();
    assert!(json.contains("\"version\": 8"));
    let back = Project::from_json(&json).unwrap();
    assert_eq!(back, project);
    assert_eq!(back.to_json_pretty(), json, "stable output");
    for t in 0..MAX_TRACKS as u8 {
        let s = back.track(t).unwrap();
        assert_eq!(s.name, format!("Layer {t}"));
        assert_eq!(s.param(ParamId::FilterCutoff), 400.0 + 100.0 * f32::from(t));
        assert_eq!(back.bank.pattern(0).notes(t).len(), 1);
        assert_eq!(back.bank.pattern(1).notes(t)[0].pitch, 60 + t);
        assert_eq!(
            back.mixer.get(Channel::Track(t), ChannelParam::Level),
            -f32::from(t)
        );
    }
    // Per-track ids never leak into the global map, and only global ids are there.
    assert!(
        back.params
            .keys()
            .all(|k| ParamId::from_key(k).is_some_and(ParamId::is_global))
    );
    assert_eq!(
        back.params.len(),
        ParamId::ALL.iter().filter(|id| id.is_global()).count()
    );

    // apply_to onto a fresh engine reproduces every track (and from_engine agrees).
    let (_h2, mut f) = new_engine(SAMPLE_RATE);
    back.apply_to(&mut f);
    assert_eq!(f.active_track_count(), MAX_TRACKS);
    for t in 0..MAX_TRACKS as u8 {
        assert_eq!(f.track_params(t), e.track_params(t), "track {t}");
        assert_eq!(f.bank().pattern(0).notes(t), e.bank().pattern(0).notes(t));
    }
    let again = Project::from_engine(&f, 3, 0.6);
    for t in 0..MAX_TRACKS as u8 {
        assert_eq!(
            again.track(t).unwrap().params,
            back.track(t).unwrap().params
        );
    }
    // A project with fewer tracks switches the other slots off (dropping notes the file holds
    // on them: the demo bassline sits on slot 0 here) and moves the edit track.
    let mut small = Project {
        tracks: vec![TrackState::new(2)],
        ..Project::default()
    };
    small.bank.patterns[0]
        .add_note(2, Note::new(0, 1, 50, 0.8))
        .unwrap();
    assert_eq!(small.bank.pattern(0).notes(0).len(), 9);
    small.apply_to(&mut f);
    assert_eq!(f.active_track_count(), 1);
    assert!(f.track_active(2));
    assert_eq!(f.edit_track(), 2);
    assert_eq!(f.bank().pattern(0).notes(2).len(), 1);
    for t in [0u8, 1, 3, 4, 5, 6, 7] {
        assert!(!f.track_active(t));
        assert!(f.bank().pattern(0).notes(t).is_empty(), "track {t}");
    }
}

// (7) The render stays allocation-free with eight active tracks, and the perf gate.

#[test]
fn render_with_eight_tracks_stays_finite_under_every_track_message() {
    // The allocator-based proof for the engine render path is
    // `eight_tracks_render_is_allocation_free` in tests/engine_alloc.rs (a global allocator
    // is per test binary). Here: a long render with every track busy, the lifecycle messages
    // in flight and the voices stealing stays finite and bounded.
    let (mut handle, mut e) = new_engine(SAMPLE_RATE);
    for t in 0..MAX_TRACKS as u8 {
        e.set_track_active(t, true);
        e.set_track_param(t, ParamId::Osc1Unison, 7.0);
        for i in 0..4u8 {
            e.set_note(0, t, i, Note::new(0, 16, 48 + 3 * i + t, 0.9));
        }
    }
    e.play();
    for block in 0..100 {
        if block % 10 == 0 {
            // Every send is asserted: a dropped message would make the block below pass
            // without the lifecycle message it is meant to survive.
            assert!(
                handle.send(Message::SwapTracks(block as u8 % 8, (block as u8 + 1) % 8)),
                "queue full"
            );
            assert!(
                handle.send(Message::CopyTrack {
                    from: 0,
                    to: block as u8 % 8,
                }),
                "queue full"
            );
            assert!(
                handle.send(Message::SetTrackActive {
                    track: 7,
                    on: block % 20 == 0,
                }),
                "queue full"
            );
        }
        let out = render(&mut e, 512);
        assert!(all_finite(&out), "block {block}");
    }
    assert!(e.total_active_voices() <= MAX_TRACKS * MAX_VOICES);
}

/// Eight active tracks, four-note chords, unison 7: one second of audio must render in well
/// under real time. Release mode only (`cargo test --release -p engine -- --ignored`).
#[test]
#[ignore]
fn eight_tracks_render_faster_than_real_time() {
    let (_handle, mut e) = new_engine(SAMPLE_RATE);
    for t in 0..MAX_TRACKS as u8 {
        e.set_track_active(t, true);
        presets::PAD.apply_to_track(&mut e, t);
        e.set_track_param(t, ParamId::Osc1Unison, 7.0);
        e.set_track_param(t, ParamId::LofiMix, 0.5);
        e.set_track_param(t, ParamId::ChorusMix, 0.5);
        e.set_track_param(t, ParamId::SidechainAmount, 0.5);
        for i in 0..4u8 {
            e.set_note(0, t, i, Note::new(0, 16, 48 + 3 * i + t, 0.9));
        }
    }
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
    assert_eq!(e.total_active_voices(), MAX_TRACKS * 4);
    assert!(
        elapsed.as_secs_f32() < 0.5,
        "1 s of 8-track audio took {elapsed:?} (must be under 0.5 s in release)"
    );
}
