//! Project file (M3): `*.beatslopper.json` holding BPM, swing, pattern, synth/FX params,
//! preset and keyboard settings.
//!
//! Serialization happens on the UI thread only; the audio thread receives the loaded state
//! through [`Project::apply_to`] (before the stream starts) or as small `Copy` messages.
//!
//! Layout ([`FORMAT_VERSION`] 2), human-readable and stable:
//!
//! ```json
//! {
//!   "version": 2,
//!   "bpm": 120.0,
//!   "swing": 0.0,
//!   "pattern": { "drums": [...], "synth": [...], "drum_volume": [...], "drum_tune": [...] },
//!   "params": { "amp_attack": 0.01, "osc1_wave": 1.0, "...": 0.0 },
//!   "preset_index": 0,
//!   "octave": 4,
//!   "velocity": 0.8
//! }
//! ```
//!
//! Parameters are keyed by [`ParamId::key`] so renaming a UI label never breaks a file.
//! On load, unknown parameter keys are ignored and missing ones take
//! [`ParamId::default_value`]; missing top-level fields take their [`Project::default`] value,
//! which is how version 1 files (just `version`, `bpm`, `swing`, `master_gain`) still open.
//! Files written by a *newer* format version are refused ([`ProjectError::Version`]).

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::engine::Engine;
use crate::params::ParamId;
use crate::pattern::Pattern;
use crate::transport::DEFAULT_BPM;

/// File extension for saved projects (`song.beatslopper.json`).
pub const FILE_EXTENSION: &str = "beatslopper.json";

/// Current on-disk format version. Bump when the layout changes incompatibly.
///
/// - 1: envelope only (`version`, `bpm`, `swing`, `master_gain`).
/// - 2: adds `pattern`, `params` (keyed by [`ParamId::key`]), `preset_index`, `octave`,
///   `velocity`; `master_gain` moved into `params`.
pub const FORMAT_VERSION: u32 = 2;

/// Default keyboard octave of a fresh project.
pub const DEFAULT_OCTAVE: i32 = 4;
/// Default keyboard velocity of a fresh project.
pub const DEFAULT_VELOCITY: f32 = 0.8;

/// Everything needed to restore a session.
///
/// `params` holds one entry per [`ParamId`] (see [`Project::param`] for the default-filling
/// accessor). A `BTreeMap` keeps the JSON sorted, so saving the same project twice yields the
/// same text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    /// On-disk format version ([`FORMAT_VERSION`] when written by this build).
    pub version: u32,
    /// Tempo in beats per minute.
    pub bpm: f32,
    /// Swing amount `0.0..=1.0`.
    pub swing: f32,
    /// The 16-step drum and synth pattern.
    pub pattern: Pattern,
    /// Synth, FX and master parameters keyed by [`ParamId::key`].
    pub params: BTreeMap<String, f32>,
    /// Index of the selected built-in preset.
    pub preset_index: usize,
    /// Keyboard octave (the `Z`/`X` shift).
    pub octave: i32,
    /// Keyboard velocity `0.0..=1.0`.
    pub velocity: f32,
}

impl Default for Project {
    /// The demo beat at 120 BPM with every parameter at its default, preset 0, octave 4,
    /// velocity 0.8.
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            bpm: DEFAULT_BPM,
            swing: 0.0,
            pattern: Pattern::demo(),
            params: default_params(),
            preset_index: 0,
            octave: DEFAULT_OCTAVE,
            velocity: DEFAULT_VELOCITY,
        }
    }
}

/// Every parameter at its default value, keyed by [`ParamId::key`].
fn default_params() -> BTreeMap<String, f32> {
    ParamId::ALL
        .iter()
        .map(|id| (id.key().to_owned(), id.default_value()))
        .collect()
}

impl Project {
    /// Snapshot of the engine's tempo, swing, pattern and parameters plus the UI-side preset
    /// and keyboard settings.
    #[must_use]
    pub fn from_engine(engine: &Engine, preset_index: usize, octave: i32, velocity: f32) -> Self {
        let params = ParamId::ALL
            .iter()
            .map(|&id| (id.key().to_owned(), engine.param(id)))
            .collect();
        Self {
            version: FORMAT_VERSION,
            bpm: engine.bpm(),
            swing: engine.transport().swing(),
            pattern: engine.pattern().clone(),
            params,
            preset_index,
            octave,
            velocity,
        }
    }

    /// Pushes tempo, swing, every known parameter and the pattern into `engine`.
    ///
    /// Unknown parameter keys are ignored; parameters missing from the file are set to their
    /// defaults, so a loaded project never inherits state from the previous one. Call before
    /// the audio stream starts (or accept a one-off pattern copy on the audio thread).
    pub fn apply_to(&self, engine: &mut Engine) {
        engine.set_bpm(self.bpm);
        engine.set_swing(self.swing);
        for &id in ParamId::ALL {
            engine.set_param(id, self.param(id));
        }
        engine.set_pattern(&self.pattern);
    }

    /// Value of parameter `id`, or its default when the project does not list it.
    #[must_use]
    pub fn param(&self, id: ParamId) -> f32 {
        self.params
            .get(id.key())
            .copied()
            .unwrap_or_else(|| id.default_value())
    }

    /// Sets parameter `id`.
    pub fn set_param(&mut self, id: ParamId, value: f32) {
        self.params.insert(id.key().to_owned(), value);
    }

    /// Pretty-printed JSON (two-space indent, trailing newline).
    ///
    /// Serializing a `Project` cannot fail (no maps with non-string keys, no custom
    /// serializers), so this is infallible.
    #[must_use]
    pub fn to_json_pretty(&self) -> String {
        let mut text = serde_json::to_string_pretty(self)
            .expect("Project serialization is infallible: plain structs, string-keyed map");
        text.push('\n');
        text
    }

    /// Parses a project. Older format versions are accepted (missing fields default); a newer
    /// version than [`FORMAT_VERSION`] is an error.
    pub fn from_json(text: &str) -> Result<Project, ProjectError> {
        let value: serde_json::Value = serde_json::from_str(text)?;
        let version = value
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .map_or(FORMAT_VERSION, |v| u32::try_from(v).unwrap_or(u32::MAX));
        if version > FORMAT_VERSION {
            return Err(ProjectError::Version {
                found: version,
                supported: FORMAT_VERSION,
            });
        }
        // Version 1 kept the master gain as a top-level field.
        let legacy_master_gain = (version < 2)
            .then(|| value.get("master_gain").and_then(serde_json::Value::as_f64))
            .flatten();

        let mut project: Project = serde_json::from_value(value)?;
        project.version = FORMAT_VERSION;
        project.normalize_params();
        if let Some(gain) = legacy_master_gain {
            project.set_param(ParamId::MasterGain, gain as f32);
        }
        Ok(project)
    }

    /// Drops unknown parameter keys and fills missing ones with their defaults, so `params`
    /// holds exactly one entry per [`ParamId`].
    fn normalize_params(&mut self) {
        self.params
            .retain(|key, _| ParamId::from_key(key).is_some());
        for &id in ParamId::ALL {
            self.params
                .entry(id.key().to_owned())
                .or_insert_with(|| id.default_value());
        }
    }

    /// Writes the project as pretty JSON to `path` (overwriting).
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ProjectError> {
        std::fs::write(path, self.to_json_pretty())?;
        Ok(())
    }

    /// Reads and parses a project file.
    pub fn load(path: impl AsRef<Path>) -> Result<Project, ProjectError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_json(&text)
    }
}

/// `true` when `path` ends in `.beatslopper.json` (case-insensitive).
#[must_use]
pub fn is_project_path(path: impl AsRef<Path>) -> bool {
    path.as_ref()
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            lower.len() > FILE_EXTENSION.len() + 1
                && lower.ends_with(FILE_EXTENSION)
                && lower.as_bytes()[lower.len() - FILE_EXTENSION.len() - 1] == b'.'
        })
}

/// Why a project could not be saved or loaded.
#[derive(Debug)]
pub enum ProjectError {
    /// Reading or writing the file failed.
    Io(std::io::Error),
    /// The text is not valid project JSON.
    Json(serde_json::Error),
    /// The file was written by a newer BeatSlopper.
    Version {
        /// Version found in the file.
        found: u32,
        /// Newest version this build reads ([`FORMAT_VERSION`]).
        supported: u32,
    },
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProjectError::Io(e) => write!(f, "project file I/O error: {e}"),
            ProjectError::Json(e) => write!(f, "invalid project JSON: {e}"),
            ProjectError::Version { found, supported } => write!(
                f,
                "project format version {found} is newer than the supported version {supported}"
            ),
        }
    }
}

impl std::error::Error for ProjectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProjectError::Io(e) => Some(e),
            ProjectError::Json(e) => Some(e),
            ProjectError::Version { .. } => None,
        }
    }
}

impl From<std::io::Error> for ProjectError {
    fn from(e: std::io::Error) -> Self {
        ProjectError::Io(e)
    }
}

impl From<serde_json::Error> for ProjectError {
    fn from(e: serde_json::Error) -> Self {
        ProjectError::Json(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_project_lists_every_param_once() {
        let p = Project::default();
        assert_eq!(p.version, FORMAT_VERSION);
        assert_eq!(p.params.len(), ParamId::COUNT);
        for &id in ParamId::ALL {
            assert_eq!(p.param(id), id.default_value(), "{id:?}");
        }
        assert_eq!(p.pattern, Pattern::demo());
    }

    #[test]
    fn json_is_pretty_and_sorted() {
        let text = Project::default().to_json_pretty();
        assert!(text.starts_with("{\n  \"version\": 2,"));
        assert!(text.ends_with("}\n"));
        let amp = text.find("\"amp_attack\"").unwrap();
        let osc = text.find("\"osc1_wave\"").unwrap();
        assert!(amp < osc, "params must be alphabetically sorted");
    }

    #[test]
    fn version_1_envelope_loads_with_defaults() {
        let v1 = r#"{ "version": 1, "bpm": 100.0, "swing": 0.25, "master_gain": 0.5 }"#;
        let p = Project::from_json(v1).unwrap();
        assert_eq!(p.version, FORMAT_VERSION);
        assert_eq!(p.bpm, 100.0);
        assert_eq!(p.swing, 0.25);
        assert_eq!(p.param(ParamId::MasterGain), 0.5);
        assert_eq!(p.pattern, Pattern::demo());
        assert_eq!(p.octave, DEFAULT_OCTAVE);
    }

    #[test]
    fn newer_version_is_refused_and_garbage_is_a_json_error() {
        let err = Project::from_json(r#"{ "version": 3 }"#).unwrap_err();
        assert!(matches!(
            err,
            ProjectError::Version {
                found: 3,
                supported: FORMAT_VERSION
            }
        ));
        assert!(err.to_string().contains("newer"));
        let err = Project::from_json("not json").unwrap_err();
        assert!(matches!(err, ProjectError::Json(_)));
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn project_paths() {
        assert!(is_project_path("song.beatslopper.json"));
        assert!(is_project_path("C:/beats/My Song.BeatSlopper.JSON"));
        assert!(!is_project_path("song.json"));
        assert!(!is_project_path(".beatslopper.json"));
        assert!(!is_project_path("beatslopper.json"));
        assert!(!is_project_path("song.wav"));
    }

    #[test]
    fn load_missing_file_is_an_io_error() {
        let err = Project::load("Z:/definitely/not/here.beatslopper.json").unwrap_err();
        assert!(matches!(err, ProjectError::Io(_)));
    }
}
