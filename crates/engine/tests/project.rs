//! M3 test gate: "Project JSON round-trips losslessly" and "Exported WAV has the expected
//! length and is non-silent" (docs/PLAN.md). Self-contained: helpers live here, not in
//! `tests/common`.

use std::collections::HashSet;
use std::path::PathBuf;

use engine::export::{
    self, TAIL_SECONDS, render_loop, render_loop_wav, render_song, render_song_wav,
};
use engine::project::{FORMAT_VERSION, is_project_path};
use engine::{
    Bank, ChainEntry, DrumRow, ExportError, ParamId, Pattern, PatternLength, PlayMode, Project,
    ProjectError, new_engine,
};

const SAMPLE_RATE: f32 = 48_000.0;
/// One 16-step bar at 120 BPM / 48 kHz: 16 * 6000 frames.
const BAR_FRAMES_120: usize = 96_000;

fn rms(buf: &[f32]) -> f32 {
    if buf.is_empty() {
        return 0.0;
    }
    let sum: f64 = buf.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    (sum / buf.len() as f64).sqrt() as f32
}

fn peak(buf: &[f32]) -> f32 {
    buf.iter()
        .map(|&s| if s.is_nan() { f32::INFINITY } else { s.abs() })
        .fold(0.0, f32::max)
}

/// A unique path in the OS temp dir (tests run in parallel, so no fixed names).
fn temp_path(name: &str, ext: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!(
        "beatslopper-test-{name}-{}-{nanos}.{ext}",
        std::process::id()
    ))
}

/// A project that differs from the default in every section, so round-trip tests cannot pass
/// by accident.
fn custom_project() -> Project {
    let mut p = Project {
        bpm: 133.0,
        swing: 0.3,
        preset_index: 5,
        octave: 2,
        velocity: 0.55,
        ..Project::default()
    };
    p.bank.patterns[0].clear_row(DrumRow::Kick);
    p.bank.patterns[0].set_drum_step(DrumRow::Kick, 2, true, true);
    p.bank.patterns[0].set_synth_step(0, Some(48), true);
    p.bank.patterns[0].set_synth_step(1, None, false);
    p.bank.patterns[0].set_drum_volume(DrumRow::Snare, -6.5);
    p.bank.patterns[0].set_drum_tune(DrumRow::ClosedHat, 3.25);
    // M5: a two-bar pattern B with a hit in its second bar, a four-bar C, a chain and song mode.
    p.bank.patterns[1] = Pattern::demo_variation();
    p.bank.set_length(1, PatternLength::Bar2);
    p.bank.patterns[1].set_drum_step(DrumRow::Clap, 30, true, false);
    p.bank.set_length(2, PatternLength::Bar4);
    p.bank.patterns[2].set_synth_step(63, Some(72), true);
    p.bank.truncate(0);
    p.bank.push(ChainEntry::new(0, 2));
    p.bank.push(ChainEntry::new(1, 1));
    p.bank.push(ChainEntry::new(2, 3));
    p.bank.mode = PlayMode::Song;
    p.bank.current = 1;
    p.set_param(ParamId::FilterCutoff, 812.5);
    p.set_param(ParamId::Osc1Wave, 2.0);
    p.set_param(ParamId::DelayFeedback, 0.6);
    p.set_param(ParamId::MasterGain, 0.75);
    p.set_param(ParamId::LimiterOn, 0.0);
    p
}

#[test]
fn default_project_round_trips_through_json() {
    let original = Project::default();
    let json = original.to_json_pretty();
    let back = Project::from_json(&json).expect("default project JSON parses");
    assert_eq!(back, original);
    // And again, so re-saving a loaded file is byte-identical.
    assert_eq!(back.to_json_pretty(), json);
}

#[test]
fn custom_project_round_trips_through_json() {
    let original = custom_project();
    let back = Project::from_json(&original.to_json_pretty()).unwrap();
    assert_eq!(back, original);
}

#[test]
fn json_layout_is_as_documented() {
    let json = Project::default().to_json_pretty();
    for field in [
        "\"version\": 3",
        "\"bpm\": 120.0",
        "\"swing\": 0.0",
        "\"bank\"",
        "\"patterns\"",
        "\"chain\"",
        "\"mode\": \"pattern\"",
        "\"current\": 0",
        "\"length\": 16",
        "\"params\"",
        "\"preset_index\": 0",
        "\"octave\": 4",
        "\"velocity\": 0.8",
        "\"osc1_wave\": 1.0",
        "\"filter_cutoff\": 2400.0",
        "\"master_gain\": 1.0",
        "\"limiter_on\": 1.0",
    ] {
        assert!(json.contains(field), "missing {field} in:\n{json}");
    }
}

#[test]
fn param_keys_are_unique_snake_case_and_reversible() {
    let mut seen = HashSet::new();
    for &id in ParamId::ALL {
        let key = id.key();
        assert!(!key.is_empty());
        assert!(
            key.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
            "{key} is not snake_case"
        );
        assert!(seen.insert(key), "duplicate key {key}");
        assert_eq!(ParamId::from_key(key), Some(id));
    }
    assert_eq!(seen.len(), ParamId::COUNT);
    assert_eq!(ParamId::from_key("Osc1Wave"), None);
    assert_eq!(ParamId::from_key("nope"), None);
    assert_eq!(ParamId::from_key(""), None);
    assert_eq!(ParamId::Osc1Wave.key(), "osc1_wave");
    assert_eq!(ParamId::FilterCutoff.key(), "filter_cutoff");
    assert_eq!(ParamId::DelayTime.key(), "delay_time");
    assert_eq!(ParamId::MasterGain.key(), "master_gain");
    assert_eq!(ParamId::LimiterOn.key(), "limiter_on");
}

#[test]
fn unknown_param_keys_are_ignored_and_missing_ones_default() {
    let mut json = serde_json::to_value(Project::default()).unwrap();
    let params = json["params"].as_object_mut().unwrap();
    params.remove("filter_cutoff");
    params.insert("flux_capacitor".into(), serde_json::json!(1.21));
    params.insert("osc1_mix".into(), serde_json::json!(0.33));
    let text = serde_json::to_string(&json).unwrap();

    let p = Project::from_json(&text).expect("unknown and missing params must not fail");
    assert_eq!(
        p.param(ParamId::FilterCutoff),
        ParamId::FilterCutoff.default_value()
    );
    assert_eq!(p.param(ParamId::Osc1Mix), 0.33);
    assert!(!p.params.contains_key("flux_capacitor"));
    assert_eq!(p.params.len(), ParamId::COUNT);

    // Missing top-level fields default as well.
    let minimal = Project::from_json(r#"{ "version": 2, "bpm": 90.0 }"#).unwrap();
    assert_eq!(minimal.bpm, 90.0);
    assert_eq!(minimal.swing, 0.0);
    assert_eq!(minimal.bank, Bank::default());
    assert_eq!(minimal.params, Project::default().params);
    assert_eq!(minimal.octave, 4);
    assert_eq!(minimal.velocity, 0.8);
}

#[test]
fn newer_format_version_fails_to_load() {
    let mut json = serde_json::to_value(Project::default()).unwrap();
    json["version"] = serde_json::json!(99);
    let err = Project::from_json(&json.to_string()).unwrap_err();
    match err {
        ProjectError::Version { found, supported } => {
            assert_eq!(found, 99);
            assert_eq!(supported, FORMAT_VERSION);
        }
        other => panic!("expected a version error, got {other}"),
    }
    assert!(matches!(
        Project::from_json("{ this is not json }"),
        Err(ProjectError::Json(_))
    ));
}

#[test]
fn apply_to_then_from_engine_reproduces_the_project() {
    let original = custom_project();
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    original.apply_to(&mut engine);

    assert_eq!(engine.bpm(), 133.0);
    assert_eq!(engine.transport().swing(), 0.3);
    assert_eq!(engine.bank(), &original.bank);
    assert_eq!(
        engine.pattern(),
        &original.bank.patterns[1],
        "edit = current"
    );
    assert_eq!(engine.param(ParamId::FilterCutoff), 812.5);
    assert!(!engine.limiter_on());

    let back = Project::from_engine(
        &engine,
        original.preset_index,
        original.octave,
        original.velocity,
    );
    assert_eq!(back, original);
}

#[test]
fn apply_to_overwrites_previous_state() {
    let (_handle, mut engine) = new_engine(SAMPLE_RATE);
    custom_project().apply_to(&mut engine);
    // A file that lists no params at all must still reset every knob to its default.
    let bare = Project::from_json(r#"{ "version": 2 }"#).unwrap();
    bare.apply_to(&mut engine);
    for &id in ParamId::ALL {
        assert_eq!(engine.param(id), id.default_value(), "{id:?}");
    }
    assert_eq!(engine.bpm(), 120.0);
    assert_eq!(engine.pattern(), &Pattern::demo());
}

#[test]
fn save_and_load_through_a_temp_file() {
    let path = temp_path("roundtrip", "beatslopper.json");
    assert!(is_project_path(&path));
    let original = custom_project();
    original.save(&path).expect("save");
    let loaded = Project::load(&path).expect("load");
    std::fs::remove_file(&path).ok();
    assert_eq!(loaded, original);
}

#[test]
fn load_errors_are_typed() {
    let missing = temp_path("missing", "beatslopper.json");
    assert!(matches!(Project::load(&missing), Err(ProjectError::Io(_))));
    let garbage = temp_path("garbage", "beatslopper.json");
    std::fs::write(&garbage, "{{{").unwrap();
    let err = Project::load(&garbage);
    std::fs::remove_file(&garbage).ok();
    assert!(matches!(err, Err(ProjectError::Json(_))));
    assert!(!is_project_path("loop.wav"));
    assert!(!is_project_path("notes.json"));
}

// --- export -----------------------------------------------------------------------------

#[test]
fn export_length_formula() {
    assert_eq!(export::bar_frames(120.0, SAMPLE_RATE), BAR_FRAMES_120);
    assert_eq!(
        export::tail_frames(SAMPLE_RATE),
        (TAIL_SECONDS * SAMPLE_RATE) as usize
    );
    let p = Project::default();
    assert_eq!(
        export::export_frames(&p, 1, false, SAMPLE_RATE),
        BAR_FRAMES_120 + 2 * 48_000
    );
}

#[test]
fn render_loop_has_exact_length_and_is_bounded() {
    let project = Project::default();
    let samples = render_loop(&project, 1, SAMPLE_RATE);
    let expected_frames = BAR_FRAMES_120 + (TAIL_SECONDS * SAMPLE_RATE) as usize;
    assert_eq!(
        samples.len(),
        expected_frames * 2,
        "interleaved stereo length"
    );
    assert!(samples.iter().all(|s| s.is_finite()), "non-finite sample");
    assert!(peak(&samples) <= 1.0, "peak {}", peak(&samples));
}

#[test]
fn render_loop_repeats_two_doubles_the_loop() {
    let project = Project::default();
    let one = render_loop(&project, 1, SAMPLE_RATE).len();
    let two = render_loop(&project, 2, SAMPLE_RATE).len();
    let tail = (TAIL_SECONDS * SAMPLE_RATE) as usize * 2;
    assert_eq!(one - tail, BAR_FRAMES_120 * 2);
    assert_eq!(two - tail, 2 * (one - tail));
    // Out-of-range repeats are clamped by the in-memory renderer, never a panic.
    assert_eq!(render_loop(&project, 0, SAMPLE_RATE).len(), one);
}

#[test]
fn render_loop_wav_writes_a_readable_file() {
    let path = temp_path("export", "wav");
    let project = Project::default();
    let stats = render_loop_wav(&project, 1, SAMPLE_RATE, &path).expect("export");
    let expected_frames = BAR_FRAMES_120 + (TAIL_SECONDS * SAMPLE_RATE) as usize;
    assert_eq!(stats.frames, expected_frames);
    assert!((stats.duration_seconds - 4.0).abs() < 1e-4);
    assert!(stats.peak.is_finite() && stats.peak <= 1.0);

    let mut reader = hound::WavReader::open(&path).expect("hound opens the file");
    let spec = reader.spec();
    assert_eq!(spec.channels, 2);
    assert_eq!(spec.sample_rate, 48_000);
    assert_eq!(spec.bits_per_sample, 16);
    assert_eq!(spec.sample_format, hound::SampleFormat::Int);
    assert_eq!(reader.len() as usize, expected_frames * 2);
    let decoded: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
    std::fs::remove_file(&path).ok();
    assert_eq!(decoded.len(), expected_frames * 2);
    let in_memory = render_loop(&project, 1, SAMPLE_RATE);
    let wav_peak = decoded
        .iter()
        .map(|s| i32::from(*s).abs())
        .max()
        .unwrap_or(0);
    let expected_peak = (peak(&in_memory).min(1.0) * f32::from(i16::MAX)).round() as i32;
    assert!(
        (wav_peak - expected_peak).abs() <= 1,
        "wav peak {wav_peak} vs rendered {expected_peak}"
    );
}

#[test]
fn render_loop_wav_rejects_bad_repeats() {
    let path = temp_path("never-written", "wav");
    for bad in [0, 9, 100] {
        match render_loop_wav(&Project::default(), bad, SAMPLE_RATE, &path) {
            Err(ExportError::InvalidRepeats(n)) => assert_eq!(n, bad),
            other => panic!("repeats {bad}: expected InvalidRepeats, got {other:?}"),
        }
    }
    assert!(
        !path.exists(),
        "validation must happen before the file is created"
    );
    for ok in [1, 2, 4, 8] {
        assert!(export::validate(ok, SAMPLE_RATE).is_ok());
    }
}

/// The M3 gate: the exported loop must actually contain the beat. This depends on the
/// sequencer scheduler and the drum kit landing in `engine.rs` / `drums/`.
#[test]
fn render_loop_of_demo_pattern_is_not_silent() {
    let samples = render_loop(&Project::default(), 1, SAMPLE_RATE);
    let level = rms(&samples);
    assert!(
        level > 0.01,
        "exported loop is silent or nearly so: rms {level}"
    );
}

// --- M5: version 3, migration, song export ----------------------------------------------

/// A literal version-2 file, as the M3/M4 builds wrote it (single `pattern`, 16-entry rows).
fn v2_json() -> String {
    let empty_drum = r#"{ "on": false, "accent": false }"#;
    let kick_row: Vec<&str> = (0..16)
        .map(|i| {
            if i == 5 {
                r#"{ "on": true, "accent": true }"#
            } else {
                empty_drum
            }
        })
        .collect();
    let empty_row = vec![empty_drum; 16];
    let synth: Vec<&str> = (0..16)
        .map(|i| {
            if i == 7 {
                r#"{ "note": 43, "accent": false }"#
            } else {
                r#"{ "note": null, "accent": false }"#
            }
        })
        .collect();
    format!(
        r#"{{
  "version": 2,
  "bpm": 101.0,
  "swing": 0.1,
  "pattern": {{
    "drums": [[{kick}], [{row}], [{row}], [{row}], [{row}]],
    "synth": [{synth}],
    "drum_volume": [-1.0, 0.0, 0.0, 0.0, -3.0],
    "drum_tune": [0.0, 2.0, 0.0, 0.0, 0.0]
  }},
  "params": {{ "osc1_wave": 3.0 }},
  "preset_index": 2,
  "octave": 3,
  "velocity": 0.6
}}
"#,
        kick = kick_row.join(", "),
        row = empty_row.join(", "),
        synth = synth.join(", ")
    )
}

#[test]
fn version_2_file_migrates_to_pattern_a_and_round_trips_as_v3() {
    let p = Project::from_json(&v2_json()).expect("v2 file loads");
    assert_eq!(p.version, 3);
    assert_eq!(FORMAT_VERSION, 3);
    assert_eq!(p.bpm, 101.0);
    assert_eq!(p.swing, 0.1);
    assert_eq!(p.preset_index, 2);
    assert_eq!(p.octave, 3);
    assert_eq!(p.velocity, 0.6);
    assert_eq!(p.param(ParamId::Osc1Wave), 3.0);

    let a = &p.bank.patterns[0];
    assert_eq!(a.len(), 16, "16-entry rows load as a one-bar pattern");
    assert!(a.drums[DrumRow::Kick.index()][5].on);
    assert!(a.drums[DrumRow::Kick.index()][5].accent);
    assert_eq!(
        a.drums.iter().flatten().filter(|s| s.on).count(),
        1,
        "only the one kick"
    );
    assert_eq!(a.synth[7].note, Some(43));
    assert_eq!(a.synth.iter().filter(|s| s.note.is_some()).count(), 1);
    assert!(a.synth[16..].iter().all(|s| s.note.is_none()), "padding");
    assert_eq!(a.drum_volume[DrumRow::OpenHat.index()], -3.0);
    assert_eq!(a.drum_tune[DrumRow::Snare.index()], 2.0);
    assert!(p.bank.patterns[1..].iter().all(Pattern::is_empty));
    assert_eq!(p.bank.chain_entries(), &[ChainEntry::new(0, 1)]);
    assert_eq!(p.bank.mode, PlayMode::Pattern);
    assert_eq!(p.bank.current, 0);
    assert_eq!(p.pattern(), a);

    // Re-saving writes version 3 with a bank and no legacy field; that file round-trips.
    let v3 = p.to_json_pretty();
    assert!(v3.starts_with("{\n  \"version\": 3,"));
    assert!(v3.contains("\"bank\""));
    assert!(!v3.contains("\n  \"pattern\""));
    let back = Project::from_json(&v3).unwrap();
    assert_eq!(back, p);
    assert_eq!(back.to_json_pretty(), v3);
}

#[test]
fn version_3_file_round_trips_losslessly() {
    let original = custom_project();
    let json = original.to_json_pretty();
    let back = Project::from_json(&json).unwrap();
    assert_eq!(back, original);
    assert_eq!(back.to_json_pretty(), json, "re-saving is byte-identical");
    assert_eq!(back.bank.patterns[1].len(), 32);
    assert!(back.bank.patterns[1].drums[DrumRow::Clap.index()][30].on);
    assert_eq!(back.bank.patterns[2].synth[63].note, Some(72));
    assert_eq!(back.bank.mode, PlayMode::Song);
    assert_eq!(back.bank.current, 1);
    assert_eq!(back.bank.chain_entries().len(), 3);

    // Through a file as well.
    let path = temp_path("v3", "beatslopper.json");
    original.save(&path).unwrap();
    let loaded = Project::load(&path);
    std::fs::remove_file(&path).ok();
    assert_eq!(loaded.unwrap(), original);
}

#[test]
fn row_lengths_follow_the_pattern_length_in_json() {
    let mut p = Project::default();
    p.bank.set_length(0, PatternLength::Bar4);
    p.bank.set_length(1, PatternLength::Bar2);
    let value: serde_json::Value = serde_json::from_str(&p.to_json_pretty()).unwrap();
    let patterns = value["bank"]["patterns"].as_array().unwrap();
    assert_eq!(patterns.len(), 8);
    assert_eq!(patterns[0]["length"], 64);
    for row in patterns[0]["drums"].as_array().unwrap() {
        assert_eq!(
            row.as_array().unwrap().len(),
            64,
            "a 64-step row has 64 entries"
        );
    }
    assert_eq!(patterns[0]["synth"].as_array().unwrap().len(), 64);
    assert_eq!(patterns[1]["length"], 32);
    assert_eq!(patterns[1]["drums"][0].as_array().unwrap().len(), 32);
    assert_eq!(patterns[2]["length"], 16);
    assert_eq!(patterns[2]["synth"].as_array().unwrap().len(), 16);
    assert_eq!(value["bank"]["chain"].as_array().unwrap().len(), 1);
    assert_eq!(value["bank"]["chain"][0]["pattern"], 0);
    assert_eq!(value["bank"]["chain"][0]["repeats"], 1);
    assert_eq!(value["bank"]["mode"], "pattern");

    // Rows longer than 64 entries are rejected.
    let mut bad = value.clone();
    let row = bad["bank"]["patterns"][0]["drums"][0]
        .as_array_mut()
        .unwrap();
    row.push(serde_json::json!({ "on": true, "accent": false }));
    assert!(matches!(
        Project::from_json(&bad.to_string()),
        Err(ProjectError::Json(_))
    ));
}

/// Default tempo: 16 steps x 6000 frames per bar. Chain A(2) B(1) A(1) with B two bars long.
fn song_project() -> Project {
    let mut p = Project::default();
    p.bank.patterns[1] = Pattern::demo_variation();
    p.bank.set_length(1, PatternLength::Bar2);
    p.bank.truncate(0);
    p.bank.push(ChainEntry::new(0, 2)); // 2 bars
    p.bank.push(ChainEntry::new(1, 1)); // 2 bars
    p.bank.push(ChainEntry::new(0, 1)); // 1 bar
    p.bank.mode = PlayMode::Song;
    p
}

#[test]
fn render_song_length_matches_the_chain_and_is_not_silent() {
    let project = song_project();
    assert_eq!(project.bank.song_bars(), 5);
    assert_eq!(project.bank.song_steps(), 80);
    let tail = (TAIL_SECONDS * SAMPLE_RATE) as usize;
    let expected_frames = 80 * 6000 + tail;
    assert_eq!(
        export::export_frames(&project, 1, true, SAMPLE_RATE),
        expected_frames
    );
    let samples = render_song(&project, SAMPLE_RATE);
    assert_eq!(samples.len(), expected_frames * 2);
    assert!(samples.iter().all(|s| s.is_finite()));
    assert!(peak(&samples) <= 1.0);
    assert!(
        rms(&samples) > 0.01,
        "song is silent: rms {}",
        rms(&samples)
    );
    // Every bar of the song carries sound (the fifth bar is pattern A again).
    for bar in 0..5 {
        let start = bar * BAR_FRAMES_120 * 2;
        let level = rms(&samples[start..start + BAR_FRAMES_120 * 2]);
        assert!(level > 0.005, "bar {bar} is silent: rms {level}");
    }

    // The song export ignores the mode stored in the project and renders the chain.
    let mut pattern_mode = project.clone();
    pattern_mode.bank.mode = PlayMode::Pattern;
    assert_eq!(render_song(&pattern_mode, SAMPLE_RATE).len(), samples.len());

    // An empty chain renders the current pattern once.
    let mut empty_chain = project.clone();
    empty_chain.bank.truncate(0);
    empty_chain.bank.current = 1;
    assert_eq!(
        render_song(&empty_chain, SAMPLE_RATE).len(),
        (2 * BAR_FRAMES_120 + tail) * 2
    );
}

#[test]
fn render_loop_of_a_32_step_pattern_with_two_repeats_is_four_bars() {
    let mut project = Project::default();
    project.bank.set_length(0, PatternLength::Bar2);
    let tail = (TAIL_SECONDS * SAMPLE_RATE) as usize;
    assert_eq!(
        export::export_frames(&project, 2, false, SAMPLE_RATE),
        4 * BAR_FRAMES_120 + tail
    );
    let samples = render_loop(&project, 2, SAMPLE_RATE);
    assert_eq!(samples.len(), (4 * BAR_FRAMES_120 + tail) * 2);
    assert!(rms(&samples) > 0.01);
    // Pattern mode is forced even when the project is in song mode: the loop export renders
    // the current pattern, not the chain.
    let mut song = song_project();
    song.bank.current = 1;
    assert_eq!(
        render_loop(&song, 1, SAMPLE_RATE).len(),
        (2 * BAR_FRAMES_120 + tail) * 2
    );
}

#[test]
fn render_song_wav_writes_the_whole_chain() {
    let path = temp_path("song", "wav");
    let project = song_project();
    let stats = render_song_wav(&project, SAMPLE_RATE, &path).expect("song export");
    let expected_frames = 5 * BAR_FRAMES_120 + (TAIL_SECONDS * SAMPLE_RATE) as usize;
    assert_eq!(stats.frames, expected_frames);
    assert!((stats.duration_seconds - 12.0).abs() < 1e-4);
    assert!(stats.peak > 0.0 && stats.peak <= 1.0);
    let reader = hound::WavReader::open(&path).expect("hound opens the file");
    assert_eq!(reader.spec().channels, 2);
    assert_eq!(reader.len() as usize, expected_frames * 2);
    drop(reader);
    std::fs::remove_file(&path).ok();

    assert!(matches!(
        render_song_wav(&project, f32::NAN, temp_path("never", "wav")),
        Err(ExportError::InvalidSampleRate(_))
    ));
}
