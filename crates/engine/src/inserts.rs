//! FL-style mixer inserts (M11): the state of the sixteen insert tracks, their effect slots
//! and the source -> insert routing.
//!
//! The model: every source ([`Channel`]: five drum rows, eight instrument tracks) carries an
//! insert number in `MixerState::routes` (`1..=16`; [`MASTER`] = `0` sends it straight to
//! the master sum). Several sources may share an insert (a drum bus). Each [`InsertState`]
//! has a fader (`level_db`), pan, mute, solo, post-fader delay / reverb sends, a built-in
//! three-band EQ ([`Eq3State`]), [`SLOT_COUNT`] effect slots ([`SlotState`]: a
//! [`SlotKind`] plus a fixed array of [`SLOT_PARAMS`] values described by the kind's
//! [`SlotParam`] table) and a `route` destination: `0` = master or a **higher** insert
//! number, so groups and sub-groups work and feedback is impossible by construction
//! ([`InsertState::valid_route`]). The engine processes inserts in ascending number order
//! (`crate::engine`), so an insert routed to a higher one is summed into a bus that has not
//! been processed yet.
//!
//! Insert numbers are 1-based in every API (`insert: u8`); [`insert_index`] maps a number to
//! the `0..INSERT_COUNT` array index and wraps (`0` addresses insert 16: the master has no
//! insert state), so an unchecked message index never panics.
//!
//! Everything here is plain `Copy` data plus clamping; the DSP lives in `crate::engine`
//! (slot instances) and `crate::fx` (`Compressor`, `Eq3`, `Drive`, `Lofi`, `Chorus`).

use core::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::dsp::db_to_gain;
use crate::fx::eq3::{self, Eq3State};
use crate::mixer::{Channel, DRUM_REVERB_SEND, MAX_LEVEL_DB, MIN_LEVEL_DB};

/// Number of insert tracks (numbered `1..=INSERT_COUNT`).
pub const INSERT_COUNT: usize = 16;
/// Effect slots per insert.
pub const SLOT_COUNT: usize = 3;
/// Fixed size of a slot's parameter array (the largest kind uses all six).
pub const SLOT_PARAMS: usize = 6;
/// The route value that means "straight to the master sum".
pub const MASTER: u8 = 0;
/// Highest insert number.
pub const MAX_INSERT: u8 = INSERT_COUNT as u8;

/// Default source routes: drum rows on inserts 1..=5, tracks on 6..=13.
pub const DEFAULT_ROUTES: [u8; Channel::COUNT] = {
    let mut routes = [0_u8; Channel::COUNT];
    let mut i = 0;
    while i < Channel::COUNT {
        routes[i] = (i + 1) as u8;
        i += 1;
    }
    routes
};

/// Array index of insert number `insert` (wraps: `(insert + 15) % 16`, so `0` and `16` both
/// address the last insert and `17` the first).
#[inline]
#[must_use]
pub const fn insert_index(insert: u8) -> usize {
    (insert as usize + INSERT_COUNT - 1) % INSERT_COUNT
}

/// Insert number of array index `index` (wraps into `1..=16`).
#[inline]
#[must_use]
pub const fn insert_number(index: usize) -> u8 {
    (index % INSERT_COUNT) as u8 + 1
}

/// Array index of slot `slot` (wraps into `0..SLOT_COUNT`).
#[inline]
#[must_use]
pub const fn slot_index(slot: u8) -> usize {
    slot as usize % SLOT_COUNT
}

/// Describes one parameter of a [`SlotKind`], so the UI can draw a knob per entry without
/// knowing the effect.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotParam {
    /// Stable snake_case name.
    pub key: &'static str,
    /// Short knob label.
    pub label: &'static str,
    /// Unit suffix for the readout (`"dB"`, `"ms"`, `"Hz"`, `":1"`, `""`).
    pub unit: &'static str,
    /// Lowest value.
    pub min: f32,
    /// Highest value.
    pub max: f32,
    /// Value a fresh slot of the kind starts with.
    pub default: f32,
}

impl SlotParam {
    const fn new(
        key: &'static str,
        label: &'static str,
        unit: &'static str,
        min: f32,
        max: f32,
        default: f32,
    ) -> Self {
        Self {
            key,
            label,
            unit,
            min,
            max,
            default,
        }
    }

    /// Valid range.
    #[must_use]
    pub const fn range(&self) -> RangeInclusive<f32> {
        self.min..=self.max
    }

    /// Clamps `value` into range; non-finite values become the default.
    #[must_use]
    pub fn clamp(&self, value: f32) -> f32 {
        if value.is_finite() {
            value.clamp(self.min, self.max)
        } else {
            self.default
        }
    }
}

/// Compressor slot: threshold, ratio, attack, release, make-up, mix.
pub const COMPRESSOR_PARAMS: [SlotParam; 6] = [
    SlotParam::new("threshold", "Thresh", "dB", -60.0, 0.0, -18.0),
    SlotParam::new("ratio", "Ratio", ":1", 1.0, 20.0, 4.0),
    SlotParam::new("attack", "Attack", "ms", 0.1, 100.0, 10.0),
    SlotParam::new("release", "Release", "ms", 10.0, 1000.0, 100.0),
    SlotParam::new("makeup", "Makeup", "dB", 0.0, 24.0, 0.0),
    SlotParam::new("mix", "Mix", "", 0.0, 1.0, 1.0),
];

/// Drive slot: amount, mix (the `tanh` waveshaper of [`crate::fx::Drive`]).
pub const DRIVE_PARAMS: [SlotParam; 2] = [
    SlotParam::new("amount", "Drive", "", 0.0, 1.0, 0.35),
    SlotParam::new("mix", "Mix", "", 0.0, 1.0, 1.0),
];

/// Lo-fi slot: bits, rate (hold length), tone (high cut), mix ([`crate::fx::Lofi`] with the
/// wobble and vinyl noise at zero).
pub const LOFI_PARAMS: [SlotParam; 4] = [
    SlotParam::new("bits", "Bits", "", 4.0, 16.0, 8.0),
    SlotParam::new("rate", "Rate", "x", 1.0, 16.0, 2.0),
    SlotParam::new("tone", "Tone", "Hz", 1000.0, 20_000.0, 10_000.0),
    SlotParam::new("mix", "Mix", "", 0.0, 1.0, 1.0),
];

/// Chorus slot: rate, depth, mix ([`crate::fx::Chorus`]).
pub const CHORUS_PARAMS: [SlotParam; 3] = [
    SlotParam::new("rate", "Rate", "Hz", 0.1, 5.0, 0.8),
    SlotParam::new("depth", "Depth", "", 0.0, 1.0, 0.3),
    SlotParam::new("mix", "Mix", "", 0.0, 1.0, 1.0),
];

/// Extra EQ3 slot: low, mid, mid frequency, high ([`crate::fx::Eq3`]).
pub const EQ3_PARAMS: [SlotParam; 4] = [
    SlotParam::new("low", "Low", "dB", eq3::MIN_GAIN_DB, eq3::MAX_GAIN_DB, 0.0),
    SlotParam::new("mid", "Mid", "dB", eq3::MIN_GAIN_DB, eq3::MAX_GAIN_DB, 0.0),
    SlotParam::new(
        "mid_freq",
        "Mid Hz",
        "Hz",
        eq3::MIN_MID_HZ,
        eq3::MAX_MID_HZ,
        eq3::DEFAULT_MID_HZ,
    ),
    SlotParam::new(
        "high",
        "High",
        "dB",
        eq3::MIN_GAIN_DB,
        eq3::MAX_GAIN_DB,
        0.0,
    ),
];

/// What an effect slot holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotKind {
    /// Nothing: the slot passes its input through untouched.
    #[default]
    Off,
    /// Feed-forward peak compressor ([`COMPRESSOR_PARAMS`]).
    Compressor,
    /// `tanh` drive ([`DRIVE_PARAMS`]).
    Drive,
    /// Bit crusher / rate reducer / high cut ([`LOFI_PARAMS`]).
    Lofi,
    /// Stereo chorus ([`CHORUS_PARAMS`]).
    Chorus,
    /// An extra three-band EQ ([`EQ3_PARAMS`]).
    Eq3,
}

impl SlotKind {
    /// Every kind, in the order of the slot picker.
    pub const ALL: [SlotKind; 6] = [
        SlotKind::Off,
        SlotKind::Compressor,
        SlotKind::Drive,
        SlotKind::Lofi,
        SlotKind::Chorus,
        SlotKind::Eq3,
    ];

    /// Index in [`SlotKind::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Kind at `index` (clamped to the last).
    #[must_use]
    pub const fn from_index(index: usize) -> SlotKind {
        if index >= Self::ALL.len() {
            Self::ALL[Self::ALL.len() - 1]
        } else {
            Self::ALL[index]
        }
    }

    /// Kind for an `f32` value (rounded, clamped; non-finite = `Off`).
    #[must_use]
    pub fn from_value(value: f32) -> SlotKind {
        if value.is_finite() {
            Self::from_index(value.round().max(0.0) as usize)
        } else {
            SlotKind::Off
        }
    }

    /// Short label for the slot pill.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            SlotKind::Off => "Off",
            SlotKind::Compressor => "Comp",
            SlotKind::Drive => "Drive",
            SlotKind::Lofi => "Lo-fi",
            SlotKind::Chorus => "Chorus",
            SlotKind::Eq3 => "EQ3",
        }
    }

    /// Stable snake_case name.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            SlotKind::Off => "off",
            SlotKind::Compressor => "compressor",
            SlotKind::Drive => "drive",
            SlotKind::Lofi => "lofi",
            SlotKind::Chorus => "chorus",
            SlotKind::Eq3 => "eq3",
        }
    }

    /// The kind's parameter table, in `params` index order (empty for `Off`).
    #[must_use]
    pub const fn params(self) -> &'static [SlotParam] {
        match self {
            SlotKind::Off => &[],
            SlotKind::Compressor => &COMPRESSOR_PARAMS,
            SlotKind::Drive => &DRIVE_PARAMS,
            SlotKind::Lofi => &LOFI_PARAMS,
            SlotKind::Chorus => &CHORUS_PARAMS,
            SlotKind::Eq3 => &EQ3_PARAMS,
        }
    }

    /// Number of parameters the kind uses (`0..=SLOT_PARAMS`).
    #[must_use]
    pub const fn param_count(self) -> usize {
        self.params().len()
    }

    /// The parameter array a fresh slot of this kind starts with (unused entries `0.0`).
    #[must_use]
    pub const fn default_params(self) -> [f32; SLOT_PARAMS] {
        let table = self.params();
        let mut out = [0.0; SLOT_PARAMS];
        let mut i = 0;
        while i < table.len() {
            out[i] = table[i].default;
            i += 1;
        }
        out
    }
}

/// One effect slot: its kind and the kind's parameter values (`params[i]` is described by
/// `kind.params()[i]`; entries past the kind's count are unused and read `0.0`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SlotState {
    /// The effect in the slot.
    pub kind: SlotKind,
    /// Its parameter values.
    pub params: [f32; SLOT_PARAMS],
}

impl Default for SlotState {
    /// [`SlotState::OFF`].
    fn default() -> Self {
        Self::OFF
    }
}

impl SlotState {
    /// An empty slot.
    pub const OFF: SlotState = SlotState {
        kind: SlotKind::Off,
        params: [0.0; SLOT_PARAMS],
    };

    /// A slot of `kind` at the kind's default parameters.
    #[must_use]
    pub const fn new(kind: SlotKind) -> Self {
        Self {
            kind,
            params: kind.default_params(),
        }
    }

    /// Replaces the kind and resets the parameters to the new kind's defaults (choosing an
    /// effect always starts it fresh, like loading a plugin).
    pub fn set_kind(&mut self, kind: SlotKind) {
        *self = Self::new(kind);
    }

    /// The descriptor of parameter `index`, if the kind uses that index.
    #[must_use]
    pub fn param(&self, index: usize) -> Option<&'static SlotParam> {
        self.kind.params().get(index)
    }

    /// Value of parameter `index` (`0.0` for an unused index).
    #[must_use]
    pub fn get(&self, index: usize) -> f32 {
        if index < self.kind.param_count() {
            self.params[index]
        } else {
            0.0
        }
    }

    /// Sets parameter `index` (clamped with its [`SlotParam::clamp`]). Returns the stored
    /// value; an unused index changes nothing and returns `0.0`.
    pub fn set(&mut self, index: usize, value: f32) -> f32 {
        match self.param(index) {
            Some(p) => {
                let v = p.clamp(value);
                self.params[index] = v;
                v
            }
            None => 0.0,
        }
    }

    /// Clamps every used parameter into range and zeroes the unused ones.
    pub fn normalise(&mut self) {
        let table = self.kind.params();
        for (i, v) in self.params.iter_mut().enumerate() {
            *v = match table.get(i) {
                Some(p) => p.clamp(*v),
                None => 0.0,
            };
        }
    }

    /// `true` for an empty slot.
    #[must_use]
    pub const fn is_off(&self) -> bool {
        matches!(self.kind, SlotKind::Off)
    }
}

/// One adjustable value of an insert, for `Message::SetInsert`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsertParam {
    /// Fader level in dB, `-60..=6`, default `0`.
    Level,
    /// Pan, `-1` (left) ..= `1` (right), default `0` (centre).
    Pan,
    /// Mute, `0` or `1` (values `>= 0.5` are on).
    Mute,
    /// Solo, `0` or `1` (values `>= 0.5` are on).
    Solo,
    /// Post-fader send into the delay bus, `0..=1`.
    DelaySend,
    /// Post-fader send into the reverb bus, `0..=1`.
    ReverbSend,
    /// Built-in EQ low shelf gain, dB (`-15..=15`).
    Eq3Low,
    /// Built-in EQ mid peak gain, dB (`-15..=15`).
    Eq3Mid,
    /// Built-in EQ mid frequency, Hz (`100..=10000`).
    Eq3MidFreq,
    /// Built-in EQ high shelf gain, dB (`-15..=15`).
    Eq3High,
    /// Destination: `0` = master, else a higher insert number (`insert + 1 ..= 16`);
    /// anything else is refused and stored as `0`.
    Route,
}

impl InsertParam {
    /// Every parameter in display order.
    pub const ALL: [InsertParam; 11] = [
        InsertParam::Level,
        InsertParam::Pan,
        InsertParam::Mute,
        InsertParam::Solo,
        InsertParam::DelaySend,
        InsertParam::ReverbSend,
        InsertParam::Eq3Low,
        InsertParam::Eq3Mid,
        InsertParam::Eq3MidFreq,
        InsertParam::Eq3High,
        InsertParam::Route,
    ];

    /// Valid range (for `Route`: `0..=16`; the per-insert rule is applied on top).
    #[must_use]
    pub const fn range(self) -> RangeInclusive<f32> {
        match self {
            InsertParam::Level => MIN_LEVEL_DB..=MAX_LEVEL_DB,
            InsertParam::Pan => -1.0..=1.0,
            InsertParam::Mute | InsertParam::Solo => 0.0..=1.0,
            InsertParam::DelaySend | InsertParam::ReverbSend => 0.0..=1.0,
            InsertParam::Eq3Low | InsertParam::Eq3Mid | InsertParam::Eq3High => {
                eq3::MIN_GAIN_DB..=eq3::MAX_GAIN_DB
            }
            InsertParam::Eq3MidFreq => eq3::MIN_MID_HZ..=eq3::MAX_MID_HZ,
            InsertParam::Route => 0.0..=(INSERT_COUNT as f32),
        }
    }

    /// Default value of the parameter on insert number `insert` (see
    /// [`default_sends`] for the sends; everything else is independent of the number).
    #[must_use]
    pub fn default_value(self, insert: u8) -> f32 {
        match self {
            InsertParam::DelaySend => default_sends(insert).0,
            InsertParam::ReverbSend => default_sends(insert).1,
            InsertParam::Eq3MidFreq => eq3::DEFAULT_MID_HZ,
            _ => 0.0,
        }
    }

    /// `true` for the on/off parameters (mute, solo).
    #[must_use]
    pub const fn is_toggle(self) -> bool {
        matches!(self, InsertParam::Mute | InsertParam::Solo)
    }

    /// Stable snake_case name.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            InsertParam::Level => "level_db",
            InsertParam::Pan => "pan",
            InsertParam::Mute => "mute",
            InsertParam::Solo => "solo",
            InsertParam::DelaySend => "delay_send",
            InsertParam::ReverbSend => "reverb_send",
            InsertParam::Eq3Low => "eq3_low",
            InsertParam::Eq3Mid => "eq3_mid",
            InsertParam::Eq3MidFreq => "eq3_mid_freq",
            InsertParam::Eq3High => "eq3_high",
            InsertParam::Route => "route",
        }
    }

    /// Short UI label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            InsertParam::Level => "Level",
            InsertParam::Pan => "Pan",
            InsertParam::Mute => "M",
            InsertParam::Solo => "S",
            InsertParam::DelaySend => "Delay",
            InsertParam::ReverbSend => "Reverb",
            InsertParam::Eq3Low => "Low",
            InsertParam::Eq3Mid => "Mid",
            InsertParam::Eq3MidFreq => "Mid Hz",
            InsertParam::Eq3High => "High",
            InsertParam::Route => "To",
        }
    }

    /// Clamps `value` for insert number `insert`: toggles snap to `0.0` / `1.0`, `Route`
    /// rounds and applies [`InsertState::valid_route`], non-finite values become the
    /// default.
    #[must_use]
    pub fn clamp(self, insert: u8, value: f32) -> f32 {
        if !value.is_finite() {
            return self.default_value(insert);
        }
        if self.is_toggle() {
            return if value >= 0.5 { 1.0 } else { 0.0 };
        }
        if self == InsertParam::Route {
            let r = value.round().clamp(0.0, 255.0) as u8;
            return f32::from(InsertState::valid_route(insert, r));
        }
        let r = self.range();
        value.clamp(*r.start(), *r.end())
    }
}

/// Default `(delay, reverb)` sends of insert number `insert`: inserts 1..=13 are the default
/// homes of [`Channel::ALL`] in order, so the snare and clap inserts (2, 3) send
/// [`DRUM_REVERB_SEND`] into the reverb and the track inserts (6..=13) send fully into both
/// effects (what the pre-M11 strips did); 14..=16 send nothing.
#[must_use]
pub fn default_sends(insert: u8) -> (f32, f32) {
    let number = usize::from(insert);
    if number == 0 || number > Channel::COUNT {
        return (0.0, 0.0);
    }
    match Channel::ALL[number - 1] {
        Channel::Track(_) => (1.0, 1.0),
        Channel::Snare | Channel::Clap => (0.0, DRUM_REVERB_SEND),
        _ => (0.0, 0.0),
    }
}

/// One insert track's settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InsertState {
    /// Fader level in dB, `-60..=6`.
    pub level_db: f32,
    /// Pan, `-1..=1`.
    pub pan: f32,
    /// Muted.
    pub mute: bool,
    /// Soloed (when any insert is soloed, the non-soloed ones are silent).
    pub solo: bool,
    /// Post-fader delay send, `0..=1`.
    pub delay_send: f32,
    /// Post-fader reverb send, `0..=1`.
    pub reverb_send: f32,
    /// The built-in three-band EQ (after the slots, before the fader).
    pub eq3: Eq3State,
    /// The effect slots, processed in order.
    pub slots: [SlotState; SLOT_COUNT],
    /// Destination: [`MASTER`] or a higher insert number.
    pub route: u8,
}

impl Default for InsertState {
    /// Unity, centre, no mute/solo, no sends, flat EQ, empty slots, routed to the master.
    fn default() -> Self {
        Self::NEUTRAL
    }
}

impl InsertState {
    /// The neutral insert: bit-transparent, no sends, to the master.
    pub const NEUTRAL: InsertState = InsertState {
        level_db: 0.0,
        pan: 0.0,
        mute: false,
        solo: false,
        delay_send: 0.0,
        reverb_send: 0.0,
        eq3: Eq3State::FLAT,
        slots: [SlotState::OFF; SLOT_COUNT],
        route: MASTER,
    };

    /// The default state of insert number `insert`: neutral except for the sends of
    /// [`default_sends`].
    #[must_use]
    pub fn default_for(insert: u8) -> Self {
        let (delay_send, reverb_send) = default_sends(insert);
        Self {
            delay_send,
            reverb_send,
            ..Self::NEUTRAL
        }
    }

    /// The route insert number `insert` may store: `route` itself when it is [`MASTER`] or a
    /// higher insert number within `1..=16`, otherwise [`MASTER`].
    #[must_use]
    pub const fn valid_route(insert: u8, route: u8) -> u8 {
        if route > insert && route <= MAX_INSERT {
            route
        } else {
            MASTER
        }
    }

    /// Reads one parameter (toggles as `0.0` / `1.0`, the route as a whole number).
    #[must_use]
    pub fn get(&self, param: InsertParam) -> f32 {
        match param {
            InsertParam::Level => self.level_db,
            InsertParam::Pan => self.pan,
            InsertParam::Mute => f32::from(u8::from(self.mute)),
            InsertParam::Solo => f32::from(u8::from(self.solo)),
            InsertParam::DelaySend => self.delay_send,
            InsertParam::ReverbSend => self.reverb_send,
            InsertParam::Eq3Low => self.eq3.low_gain_db,
            InsertParam::Eq3Mid => self.eq3.mid_gain_db,
            InsertParam::Eq3MidFreq => self.eq3.mid_freq_hz,
            InsertParam::Eq3High => self.eq3.high_gain_db,
            InsertParam::Route => f32::from(self.route),
        }
    }

    /// Writes one parameter; `value` must already be clamped (see [`InsertParam::clamp`]).
    fn put(&mut self, param: InsertParam, value: f32) {
        match param {
            InsertParam::Level => self.level_db = value,
            InsertParam::Pan => self.pan = value,
            InsertParam::Mute => self.mute = value >= 0.5,
            InsertParam::Solo => self.solo = value >= 0.5,
            InsertParam::DelaySend => self.delay_send = value,
            InsertParam::ReverbSend => self.reverb_send = value,
            InsertParam::Eq3Low => self.eq3.low_gain_db = value,
            InsertParam::Eq3Mid => self.eq3.mid_gain_db = value,
            InsertParam::Eq3MidFreq => self.eq3.mid_freq_hz = value,
            InsertParam::Eq3High => self.eq3.high_gain_db = value,
            InsertParam::Route => self.route = value.round().clamp(0.0, 255.0) as u8,
        }
    }

    /// Sets one parameter of insert number `insert` (clamped with [`InsertParam::clamp`]).
    /// Returns the stored value (for `Route`, `0.0` when the destination was refused).
    pub fn set(&mut self, insert: u8, param: InsertParam, value: f32) -> f32 {
        let v = param.clamp(insert, value);
        self.put(param, v);
        v
    }

    /// Sets the route of insert number `insert` (see [`valid_route`](Self::valid_route)).
    /// Returns the stored value.
    pub fn set_route(&mut self, insert: u8, route: u8) -> u8 {
        self.route = Self::valid_route(insert, route);
        self.route
    }

    /// Clamps every field into range for insert number `insert` (non-finite values become
    /// the defaults, an invalid route becomes [`MASTER`]).
    pub fn normalise(&mut self, insert: u8) {
        for param in InsertParam::ALL {
            let v = param.clamp(insert, self.get(param));
            self.put(param, v);
        }
        self.eq3.normalise();
        for slot in &mut self.slots {
            slot.normalise();
        }
    }

    /// Linear fader gain (`db_to_gain(level_db)`), ignoring mute and solo.
    #[must_use]
    pub fn level_gain(&self) -> f32 {
        db_to_gain(self.level_db)
    }

    /// Slot `slot` (wraps into `0..SLOT_COUNT`).
    #[must_use]
    pub fn slot(&self, slot: u8) -> &SlotState {
        &self.slots[slot_index(slot)]
    }

    /// Slot `slot`, mutably (wraps).
    pub fn slot_mut(&mut self, slot: u8) -> &mut SlotState {
        &mut self.slots[slot_index(slot)]
    }

    /// `true` when the insert is bit-transparent apart from its sends and route: every slot
    /// off, flat EQ, 0 dB, centre, neither muted nor soloed.
    #[must_use]
    pub fn is_neutral(&self) -> bool {
        self.level_db == 0.0
            && self.pan == 0.0
            && !self.mute
            && !self.solo
            && self.eq3.is_flat()
            && self.slots.iter().all(SlotState::is_off)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indices_wrap_and_routes_default_one_to_one() {
        assert_eq!(insert_index(1), 0);
        assert_eq!(insert_index(16), 15);
        assert_eq!(insert_index(0), 15);
        assert_eq!(insert_index(17), 0);
        assert_eq!(insert_number(0), 1);
        assert_eq!(insert_number(15), 16);
        assert_eq!(insert_number(16), 1);
        assert_eq!(slot_index(3), 0);
        assert_eq!(slot_index(5), 2);
        assert_eq!(DEFAULT_ROUTES[0], 1);
        assert_eq!(DEFAULT_ROUTES[4], 5);
        assert_eq!(DEFAULT_ROUTES[Channel::Track(0).index()], 6);
        assert_eq!(DEFAULT_ROUTES[Channel::Track(7).index()], 13);
    }

    #[test]
    fn slot_kinds_tables_and_defaults() {
        for (i, k) in SlotKind::ALL.iter().enumerate() {
            assert_eq!(k.index(), i);
            assert_eq!(SlotKind::from_index(i), *k);
            assert_eq!(SlotKind::from_value(i as f32 + 0.3), *k);
            assert!(k.param_count() <= SLOT_PARAMS);
            for p in k.params() {
                assert!(p.range().contains(&p.default), "{k:?} {}", p.key);
                assert!(p.min < p.max);
            }
            let keys: std::collections::HashSet<&str> = k.params().iter().map(|p| p.key).collect();
            assert_eq!(keys.len(), k.param_count(), "{k:?} keys are unique");
            assert_eq!(
                serde_json::to_string(k).unwrap(),
                format!("\"{}\"", k.key())
            );
        }
        assert_eq!(SlotKind::from_index(99), SlotKind::Eq3);
        assert_eq!(SlotKind::from_value(f32::NAN), SlotKind::Off);
        assert_eq!(SlotKind::from_value(-3.0), SlotKind::Off);
        assert_eq!(SlotKind::Compressor.param_count(), 6);
        assert_eq!(SlotKind::Drive.param_count(), 2);
        assert_eq!(SlotKind::Lofi.param_count(), 4);
        assert_eq!(SlotKind::Chorus.param_count(), 3);
        assert_eq!(SlotKind::Eq3.param_count(), 4);
        assert_eq!(SlotKind::Off.param_count(), 0);
        let s = SlotState::new(SlotKind::Compressor);
        assert_eq!(s.get(0), -18.0);
        assert_eq!(s.get(1), 4.0);
        assert_eq!(s.get(5), 1.0);
        let mut s = SlotState::new(SlotKind::Drive);
        assert_eq!(s.params, [0.35, 1.0, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(s.set(0, 7.0), 1.0);
        assert_eq!(s.set(1, f32::NAN), 1.0);
        assert_eq!(s.set(4, 0.5), 0.0, "unused index is refused");
        assert_eq!(s.get(4), 0.0);
        s.params[3] = 9.0;
        s.params[0] = -2.0;
        s.normalise();
        assert_eq!(s.params, [0.0, 1.0, 0.0, 0.0, 0.0, 0.0]);
        s.set_kind(SlotKind::Chorus);
        assert_eq!(s, SlotState::new(SlotKind::Chorus));
        assert!(SlotState::default().is_off());
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<SlotState>(&json).unwrap(), s);
        let partial: SlotState = serde_json::from_str(r#"{ "kind": "lofi" }"#).unwrap();
        assert_eq!(partial.kind, SlotKind::Lofi);
        assert_eq!(partial.params, [0.0; SLOT_PARAMS]);
    }

    #[test]
    fn insert_defaults_clamping_and_routes() {
        assert_eq!(default_sends(0), (0.0, 0.0));
        assert_eq!(default_sends(1), (0.0, 0.0));
        assert_eq!(default_sends(2), (0.0, DRUM_REVERB_SEND));
        assert_eq!(default_sends(3), (0.0, DRUM_REVERB_SEND));
        assert_eq!(default_sends(6), (1.0, 1.0));
        assert_eq!(default_sends(13), (1.0, 1.0));
        assert_eq!(default_sends(14), (0.0, 0.0));
        assert_eq!(default_sends(200), (0.0, 0.0));
        let d = InsertState::default_for(7);
        assert_eq!(d.delay_send, 1.0);
        assert!(d.is_neutral());
        assert_eq!(InsertState::default(), InsertState::NEUTRAL);
        for p in InsertParam::ALL {
            for n in 0..=17u8 {
                assert!(p.range().contains(&p.default_value(n)), "{p:?} {n}");
            }
            if p != InsertParam::Route {
                assert_eq!(p.clamp(3, *p.range().end() + 1.0), *p.range().end());
            }
            let json = serde_json::to_string(&p).unwrap();
            assert_eq!(serde_json::from_str::<InsertParam>(&json).unwrap(), p);
        }
        assert_eq!(
            serde_json::to_string(&InsertParam::Eq3MidFreq).unwrap(),
            "\"eq3_mid_freq\""
        );
        let mut s = InsertState::default_for(3);
        assert_eq!(s.set(3, InsertParam::Level, 40.0), MAX_LEVEL_DB);
        assert_eq!(s.set(3, InsertParam::Pan, -3.0), -1.0);
        assert_eq!(s.set(3, InsertParam::Mute, 0.6), 1.0);
        assert!(s.mute);
        assert_eq!(s.set(3, InsertParam::Eq3Low, -30.0), eq3::MIN_GAIN_DB);
        assert_eq!(
            s.set(3, InsertParam::Eq3MidFreq, f32::NAN),
            eq3::DEFAULT_MID_HZ
        );
        assert_eq!(
            s.set(3, InsertParam::ReverbSend, f32::NAN),
            DRUM_REVERB_SEND
        );
        assert_eq!(s.get(InsertParam::Eq3Low), -15.0);
        // Routes: only higher numbers up to 16 are accepted; the rest become the master.
        assert_eq!(s.set(3, InsertParam::Route, 4.0), 4.0);
        assert_eq!(s.route, 4);
        assert_eq!(s.set(3, InsertParam::Route, 3.0), 0.0, "self is refused");
        assert_eq!(s.set(3, InsertParam::Route, 2.0), 0.0, "lower is refused");
        assert_eq!(s.set_route(3, 16), 16);
        assert_eq!(s.set_route(3, 17), 0);
        assert_eq!(s.set_route(16, 16), 0);
        assert_eq!(InsertState::valid_route(15, 16), 16);
        assert_eq!(InsertState::valid_route(1, 0), 0);
        s.route = 2;
        s.level_db = f32::INFINITY;
        s.slots[1] = SlotState {
            kind: SlotKind::Compressor,
            params: [-100.0, 50.0, 0.0, 0.0, 0.0, 2.0],
        };
        s.normalise(3);
        assert_eq!(s.route, 0);
        assert_eq!(s.level_db, 0.0);
        assert_eq!(s.slots[1].params, [-60.0, 20.0, 0.1, 10.0, 0.0, 1.0]);
        assert!(!s.is_neutral());
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<InsertState>(&json).unwrap(), s);
        let partial: InsertState = serde_json::from_str(r#"{ "route": 5 }"#).unwrap();
        assert_eq!(partial.route, 5);
        assert_eq!(partial.eq3, Eq3State::FLAT);
    }
}
