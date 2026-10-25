//! Preset list used by the synth panel's preset selector.
//!
//! The canonical list lives in the engine (`engine::presets::ALL`, `engine::Preset`, see
//! docs/ARCHITECTURE.md "M2 additions / engine: synth"); this module only re-exports it so the
//! UI has one place to import from. Presets are plain `(ParamId, value)` data: the UI applies
//! them through `Message::SetParam` (see `AppState::apply_preset`), never by touching the
//! engine directly.

pub use engine::Preset;
pub use engine::presets::ALL;
