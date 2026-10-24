//! Computer-keyboard piano input (M1).
//!
//! M1 maps `A W S E D F T G Y H U J K O L P ;` to notes over about 1.5 octaves, with `Z`/`X`
//! shifting the octave. The OS repeats key-down events while a key is held, so the mapping
//! must be driven by a set of *held* keys: a key only triggers a note the first time it goes
//! down, and releases it when it goes up. [`HeldKeys`] is the home for that tracking.
//!
//! M0 ships only the empty type so the module layout is fixed; no behaviour yet.

/// Keys currently held down on the computer keyboard (M1 fills this in).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HeldKeys {}

impl HeldKeys {
    /// Creates an empty set.
    pub fn new() -> Self {
        Self::default()
    }
}
