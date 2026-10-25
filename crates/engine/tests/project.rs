//! M3 test gate: "Project JSON round-trips losslessly" and "Exported WAV has the expected
//! length and is non-silent" (docs/PLAN.md). Self-contained: helpers live here, not in
//! `tests/common`.

use std::collections::HashSet;
use std::path::PathBuf;

use engine::export::{self, TAIL_SECONDS, render_loop, render_loop_wav};
use engine::project::{FORMAT_VERSION, is_project_path};
use engine::{DrumRow, ExportError, ParamId, Pattern, Project, ProjectError, new_engine};

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
    p.pattern.clear_row(DrumRow::Kick);
    p.pattern.set_drum_step(DrumRow::Kick, 2, true, true);
    p.pattern.set_synth_step(0, Some(48), true);
    p.pattern.set_synth_step(1, None, false);
    p.pattern.set_drum_volume(DrumRow::Snare, -6.5);
    p.pattern.set_drum_tune(DrumRow::ClosedHat, 3.25);
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
        "\"version\": 2",
        "\"bpm\": 120.0",
        "\"swing\": 0.0",
        "\"pattern\"",
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
    assert_eq!(minimal.pattern, Pattern::demo());
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
    assert_eq!(engine.pattern(), &original.pattern);
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
        export::export_frames(&p, 1, SAMPLE_RATE),
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
