//! Project file (M3): `*.beatslopper.json` holding BPM, swing, pattern, synth/FX params.
//!
//! Serialization happens on the UI thread only; the audio thread receives the loaded state as
//! small `Copy` messages. M0 defines the envelope (version + master settings); M3 fills in the
//! pattern, synth and FX sections.

use serde::{Deserialize, Serialize};

use crate::transport::DEFAULT_BPM;

/// File extension for saved projects (`song.beatslopper.json`).
pub const FILE_EXTENSION: &str = "beatslopper.json";

/// Current on-disk format version. Bump when the layout changes incompatibly.
pub const FORMAT_VERSION: u32 = 1;

/// Everything needed to restore a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// On-disk format version ([`FORMAT_VERSION`] when written by this build).
    pub version: u32,
    /// Tempo in beats per minute.
    pub bpm: f32,
    /// Swing amount `0.0..=1.0`.
    pub swing: f32,
    /// Master output gain, linear.
    pub master_gain: f32,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            bpm: DEFAULT_BPM,
            swing: 0.0,
            master_gain: 1.0,
        }
    }
}
