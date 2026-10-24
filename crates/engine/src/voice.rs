//! Polyphonic voice allocation (M1): a fixed pool of voices with oldest-note stealing.
//!
//! Voices are pre-allocated in `new_engine`; the pool never grows on the audio thread.
//! M0 only fixes the pool size and the voice life cycle.

/// Maximum number of simultaneously sounding synth voices.
pub const MAX_VOICES: usize = 16;

/// Life cycle of one voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VoiceState {
    /// Free; can be allocated without stealing.
    #[default]
    Idle,
    /// Key held; envelope in attack/decay/sustain.
    Held,
    /// Key released; envelope in release. Stolen before held voices.
    Releasing,
}
