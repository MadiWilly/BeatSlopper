//! Mixer (M6, 13 channels since M10): channel strips for the five drum rows plus the eight
//! instrument tracks, with level, pan, mute, solo and post-fader sends into the shared delay
//! and reverb.
//!
//! This module is plain data plus arithmetic: [`MixerState`] is what the project file stores
//! and what the UI mirrors, [`Channel`] / [`ChannelParam`] address one value for the
//! `Message::SetMixer` message, and the two helpers [`MixerState::effective_gain`] and
//! [`pan_gains`] define the gain law. The smoothing and the actual summing live in the engine's
//! render loop (`crate::engine`), which reads this state once per change and never allocates.
//!
//! # Gain law
//!
//! - **Level** is in dB (`-60..=6`, `0` = unity). A channel that is muted, or not soloed while
//!   any other channel is soloed, has an effective gain of exactly `0.0`.
//! - **Pan** uses the equal-power (sine/cosine) law normalised to unity at the centre:
//!   `pan_gains(0.0) == (1.0, 1.0)`, `pan_gains(-1.0) == (sqrt 2, 0.0)`,
//!   `pan_gains(1.0) == (0.0, sqrt 2)`. The centre therefore sits 3 dB below a hard pan
//!   while `l^2 + r^2` stays constant, and a channel at the default (centre) renders exactly as
//!   the pre-mixer engine did, with no level change.
//! - **Sends** (`0..=1`) are post-fader and follow the channel's pan into a stereo delay bus
//!   and a stereo reverb bus; `ParamId::DelayMix` / `ParamId::ReverbMix` are the return
//!   levels of those buses.

use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::drums::DrumRow;
use crate::dsp::db_to_gain;
use crate::track::{MAX_TRACKS, TrackId, track_index};

/// Lowest channel level, dB (treated as silence by the UI's fader bottom).
pub const MIN_LEVEL_DB: f32 = -60.0;
/// Highest channel level, dB.
pub const MAX_LEVEL_DB: f32 = 6.0;
/// Default reverb send of the snare and clap channels.
pub const DRUM_REVERB_SEND: f32 = 0.3;

/// A mixer channel: the five drum rows in sequencer order, then the eight instrument tracks
/// (M10; before that a single `Synth` channel, which is now [`Channel::Track`]`(0)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// Kick drum row.
    Kick,
    /// Snare row.
    Snare,
    /// Clap row.
    Clap,
    /// Closed hat row.
    ClosedHat,
    /// Open hat row.
    OpenHat,
    /// Instrument track `0..MAX_TRACKS` (its voices through its insert, stereo).
    Track(u8),
}

/// Strip labels of the track channels.
const TRACK_LABELS: [&str; MAX_TRACKS] = ["T1", "T2", "T3", "T4", "T5", "T6", "T7", "T8"];
/// Stable keys of the track channels.
const TRACK_KEYS: [&str; MAX_TRACKS] = [
    "track_1", "track_2", "track_3", "track_4", "track_5", "track_6", "track_7", "track_8",
];

impl Channel {
    /// Number of channels: the drum rows plus one per track slot.
    pub const COUNT: usize = DrumRow::COUNT + MAX_TRACKS;

    /// All channels in strip order.
    pub const ALL: [Channel; Self::COUNT] = {
        let mut all = [Channel::Kick; Self::COUNT];
        all[1] = Channel::Snare;
        all[2] = Channel::Clap;
        all[3] = Channel::ClosedHat;
        all[4] = Channel::OpenHat;
        let mut t = 0;
        while t < MAX_TRACKS {
            all[DrumRow::COUNT + t] = Channel::Track(t as u8);
            t += 1;
        }
        all
    };

    /// The pre-M10 synth channel, now track 0's channel (kept so older call sites read on).
    #[allow(non_upper_case_globals)]
    pub const Synth: Channel = Channel::Track(0);

    /// Index of this channel in [`Channel::ALL`]: drum rows share their [`DrumRow::index`],
    /// track `t` is `DrumRow::COUNT + t` (track indices wrap into `0..MAX_TRACKS`).
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Channel::Kick => 0,
            Channel::Snare => 1,
            Channel::Clap => 2,
            Channel::ClosedHat => 3,
            Channel::OpenHat => 4,
            Channel::Track(t) => DrumRow::COUNT + track_index(t),
        }
    }

    /// Channel at `index` (wraps into `0..COUNT`).
    #[must_use]
    pub const fn from_index(index: usize) -> Channel {
        Self::ALL[index % Self::COUNT]
    }

    /// The channel of instrument track `track` (wraps into `0..MAX_TRACKS`).
    #[must_use]
    pub const fn from_track(track: TrackId) -> Channel {
        Channel::Track(track_index(track) as u8)
    }

    /// The track behind this channel, or `None` for a drum row.
    #[must_use]
    pub const fn track(self) -> Option<TrackId> {
        match self {
            Channel::Track(t) => Some(track_index(t) as u8),
            _ => None,
        }
    }

    /// The channel of a drum row.
    #[must_use]
    pub const fn from_row(row: DrumRow) -> Channel {
        match row {
            DrumRow::Kick => Channel::Kick,
            DrumRow::Snare => Channel::Snare,
            DrumRow::Clap => Channel::Clap,
            DrumRow::ClosedHat => Channel::ClosedHat,
            DrumRow::OpenHat => Channel::OpenHat,
        }
    }

    /// The drum row behind this channel, or `None` for a track.
    #[must_use]
    pub const fn drum_row(self) -> Option<DrumRow> {
        match self {
            Channel::Kick => Some(DrumRow::Kick),
            Channel::Snare => Some(DrumRow::Snare),
            Channel::Clap => Some(DrumRow::Clap),
            Channel::ClosedHat => Some(DrumRow::ClosedHat),
            Channel::OpenHat => Some(DrumRow::OpenHat),
            Channel::Track(_) => None,
        }
    }

    /// Short strip label (`"T1"`..`"T8"` for the tracks; the UI substitutes the track name).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Channel::Kick => "Kick",
            Channel::Snare => "Snare",
            Channel::Clap => "Clap",
            Channel::ClosedHat => "CH",
            Channel::OpenHat => "OH",
            Channel::Track(t) => TRACK_LABELS[track_index(t)],
        }
    }

    /// Stable snake_case name (for keys and tests): `"track_1"`..`"track_8"` for the tracks.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Channel::Kick => "kick",
            Channel::Snare => "snare",
            Channel::Clap => "clap",
            Channel::ClosedHat => "closed_hat",
            Channel::OpenHat => "open_hat",
            Channel::Track(t) => TRACK_KEYS[track_index(t)],
        }
    }
}

/// One adjustable value of a channel strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelParam {
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
}

impl ChannelParam {
    /// All strip parameters in display order.
    pub const ALL: [ChannelParam; 6] = [
        ChannelParam::Level,
        ChannelParam::Pan,
        ChannelParam::Mute,
        ChannelParam::Solo,
        ChannelParam::DelaySend,
        ChannelParam::ReverbSend,
    ];

    /// Valid range of the parameter.
    #[must_use]
    pub const fn range(self) -> RangeInclusive<f32> {
        match self {
            ChannelParam::Level => MIN_LEVEL_DB..=MAX_LEVEL_DB,
            ChannelParam::Pan => -1.0..=1.0,
            ChannelParam::Mute | ChannelParam::Solo => 0.0..=1.0,
            ChannelParam::DelaySend | ChannelParam::ReverbSend => 0.0..=1.0,
        }
    }

    /// Default value for `channel`: every track sends fully into both effects, the snare and
    /// clap send [`DRUM_REVERB_SEND`] into the reverb, everything else is `0`.
    #[must_use]
    pub const fn default_value(self, channel: Channel) -> f32 {
        match (self, channel) {
            (ChannelParam::DelaySend, Channel::Track(_)) => 1.0,
            (ChannelParam::ReverbSend, Channel::Track(_)) => 1.0,
            (ChannelParam::ReverbSend, Channel::Snare | Channel::Clap) => DRUM_REVERB_SEND,
            _ => 0.0,
        }
    }

    /// `true` for the on/off parameters (mute, solo).
    #[must_use]
    pub const fn is_toggle(self) -> bool {
        matches!(self, ChannelParam::Mute | ChannelParam::Solo)
    }

    /// Stable snake_case name.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            ChannelParam::Level => "level_db",
            ChannelParam::Pan => "pan",
            ChannelParam::Mute => "mute",
            ChannelParam::Solo => "solo",
            ChannelParam::DelaySend => "delay_send",
            ChannelParam::ReverbSend => "reverb_send",
        }
    }

    /// Short UI label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            ChannelParam::Level => "Level",
            ChannelParam::Pan => "Pan",
            ChannelParam::Mute => "M",
            ChannelParam::Solo => "S",
            ChannelParam::DelaySend => "Delay",
            ChannelParam::ReverbSend => "Reverb",
        }
    }

    /// Clamps `value` into [`range`](Self::range); toggles snap to `0.0` / `1.0`. Non-finite
    /// values become the channel's default.
    #[must_use]
    pub fn clamp(self, channel: Channel, value: f32) -> f32 {
        if !value.is_finite() {
            return self.default_value(channel);
        }
        if self.is_toggle() {
            return if value >= 0.5 { 1.0 } else { 0.0 };
        }
        let r = self.range();
        value.clamp(*r.start(), *r.end())
    }
}

/// One channel strip's settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Strip {
    /// Fader level in dB, `-60..=6`.
    pub level_db: f32,
    /// Pan, `-1..=1`.
    pub pan: f32,
    /// Muted.
    pub mute: bool,
    /// Soloed.
    pub solo: bool,
    /// Post-fader delay send, `0..=1`.
    pub delay_send: f32,
    /// Post-fader reverb send, `0..=1`.
    pub reverb_send: f32,
}

impl Default for Strip {
    /// Unity, centre, no mute/solo, no sends (the defaults of a drum channel without reverb).
    fn default() -> Self {
        Self::for_channel(Channel::Kick)
    }
}

impl Strip {
    /// The default strip of `channel` (see [`ChannelParam::default_value`]).
    #[must_use]
    pub fn for_channel(channel: Channel) -> Self {
        Self {
            level_db: ChannelParam::Level.default_value(channel),
            pan: ChannelParam::Pan.default_value(channel),
            mute: ChannelParam::Mute.default_value(channel) >= 0.5,
            solo: ChannelParam::Solo.default_value(channel) >= 0.5,
            delay_send: ChannelParam::DelaySend.default_value(channel),
            reverb_send: ChannelParam::ReverbSend.default_value(channel),
        }
    }

    /// Reads one parameter (toggles as `0.0` / `1.0`).
    #[must_use]
    pub fn get(&self, param: ChannelParam) -> f32 {
        match param {
            ChannelParam::Level => self.level_db,
            ChannelParam::Pan => self.pan,
            ChannelParam::Mute => f32::from(u8::from(self.mute)),
            ChannelParam::Solo => f32::from(u8::from(self.solo)),
            ChannelParam::DelaySend => self.delay_send,
            ChannelParam::ReverbSend => self.reverb_send,
        }
    }

    /// Writes one parameter; `value` must already be clamped (see [`ChannelParam::clamp`]).
    fn put(&mut self, param: ChannelParam, value: f32) {
        match param {
            ChannelParam::Level => self.level_db = value,
            ChannelParam::Pan => self.pan = value,
            ChannelParam::Mute => self.mute = value >= 0.5,
            ChannelParam::Solo => self.solo = value >= 0.5,
            ChannelParam::DelaySend => self.delay_send = value,
            ChannelParam::ReverbSend => self.reverb_send = value,
        }
    }

    /// Clamps every field into range (non-finite values become the channel's defaults).
    pub fn normalise(&mut self, channel: Channel) {
        for param in ChannelParam::ALL {
            let v = param.clamp(channel, self.get(param));
            self.put(param, v);
        }
    }

    /// Linear fader gain (`db_to_gain(level_db)`), ignoring mute and solo.
    #[must_use]
    pub fn level_gain(&self) -> f32 {
        db_to_gain(self.level_db)
    }
}

/// The whole mixer: one [`Strip`] per [`Channel`], in [`Channel::ALL`] order.
///
/// Serialises as `{ "strips": [...] }`. On load a shorter array is padded with the missing
/// channels' default strips, so a pre-M10 file (six strips: five drums plus the synth) loads
/// with its synth strip on [`Channel::Track`]`(0)` and defaults on tracks 1..8; more than
/// [`Channel::COUNT`] strips is an error.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MixerState {
    /// Strips indexed by [`Channel::index`].
    pub strips: [Strip; Channel::COUNT],
}

impl Default for MixerState {
    /// Every channel at unity and centre, every track sent fully to delay and reverb, snare
    /// and clap sent [`DRUM_REVERB_SEND`] to the reverb, nothing muted or soloed.
    fn default() -> Self {
        Self {
            strips: Channel::ALL.map(Strip::for_channel),
        }
    }
}

/// On-disk shape of [`MixerState`]: any number of strips up to [`Channel::COUNT`].
#[derive(Deserialize)]
#[serde(default)]
struct MixerRepr {
    strips: Vec<Strip>,
}

impl Default for MixerRepr {
    fn default() -> Self {
        Self {
            strips: MixerState::default().strips.to_vec(),
        }
    }
}

impl<'de> Deserialize<'de> for MixerState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        let repr = MixerRepr::deserialize(deserializer)?;
        if repr.strips.len() > Channel::COUNT {
            return Err(D::Error::custom(format!(
                "mixer has {} strips, at most {} are allowed",
                repr.strips.len(),
                Channel::COUNT
            )));
        }
        let mut state = MixerState::default();
        for (slot, strip) in state.strips.iter_mut().zip(repr.strips) {
            *slot = strip;
        }
        Ok(state)
    }
}

impl MixerState {
    /// Swaps the strips of two channels (M10 `SwapTracks`).
    pub fn swap(&mut self, a: Channel, b: Channel) {
        self.strips.swap(a.index(), b.index());
    }

    /// The strip of `channel`.
    #[must_use]
    pub fn strip(&self, channel: Channel) -> &Strip {
        &self.strips[channel.index()]
    }

    /// The strip of `channel`, mutably. Call [`normalise`](Self::normalise) afterwards if the
    /// values may be out of range.
    pub fn strip_mut(&mut self, channel: Channel) -> &mut Strip {
        &mut self.strips[channel.index()]
    }

    /// Reads one value.
    #[must_use]
    pub fn get(&self, channel: Channel, param: ChannelParam) -> f32 {
        self.strip(channel).get(param)
    }

    /// Sets one value, clamped with [`ChannelParam::clamp`] (toggles snap to 0/1, non-finite
    /// values reset to the default). Returns the stored value.
    pub fn set(&mut self, channel: Channel, param: ChannelParam, value: f32) -> f32 {
        let v = param.clamp(channel, value);
        self.strip_mut(channel).put(param, v);
        v
    }

    /// Clamps every strip into range.
    pub fn normalise(&mut self) {
        for channel in Channel::ALL {
            self.strips[channel.index()].normalise(channel);
        }
    }

    /// `true` when at least one channel is soloed.
    #[must_use]
    pub fn any_solo(&self) -> bool {
        self.strips.iter().any(|s| s.solo)
    }

    /// Mute/solo logic alone: a channel sounds unless it is muted, or another channel is
    /// soloed and this one is not.
    #[must_use]
    pub fn is_audible(&self, channel: Channel) -> bool {
        let strip = self.strip(channel);
        !strip.mute && (strip.solo || !self.any_solo())
    }

    /// The channel's linear gain target: its fader gain when [`is_audible`](Self::is_audible),
    /// else exactly `0.0`.
    #[must_use]
    pub fn effective_gain(&self, channel: Channel) -> f32 {
        if self.is_audible(channel) {
            self.strip(channel).level_gain()
        } else {
            0.0
        }
    }

    /// Left/right gains of `channel`'s pan position (see [`pan_gains`]).
    #[must_use]
    pub fn pan_gains(&self, channel: Channel) -> (f32, f32) {
        pan_gains(self.strip(channel).pan)
    }
}

/// Equal-power pan law normalised to unity at the centre.
///
/// `pan` is clamped to `-1..=1` (non-finite = centre). With `theta = (pan + 1) * pi / 4`:
/// `l = sqrt(2) * cos(theta)`, `r = sqrt(2) * sin(theta)`, so `l^2 + r^2 == 2` everywhere,
/// the centre is exactly `(1.0, 1.0)` and a hard pan is `(sqrt 2, 0.0)` / `(0.0, sqrt 2)`:
/// the centre is 3 dB below the sides, the classic -3 dB law.
#[inline]
#[must_use]
pub fn pan_gains(pan: f32) -> (f32, f32) {
    let pan = if pan.is_finite() {
        pan.clamp(-1.0, 1.0)
    } else {
        0.0
    };
    if pan == 0.0 {
        return (1.0, 1.0);
    }
    if pan <= -1.0 {
        return (core::f32::consts::SQRT_2, 0.0);
    }
    if pan >= 1.0 {
        return (0.0, core::f32::consts::SQRT_2);
    }
    let theta = (pan + 1.0) * core::f32::consts::FRAC_PI_4;
    let (s, c) = theta.sin_cos();
    (c * core::f32::consts::SQRT_2, s * core::f32::consts::SQRT_2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_indices_and_rows_line_up() {
        for (i, ch) in Channel::ALL.iter().enumerate() {
            assert_eq!(ch.index(), i);
            assert_eq!(Channel::from_index(i), *ch);
            if let Some(row) = ch.drum_row() {
                assert_eq!(row.index(), i);
                assert_eq!(Channel::from_row(row), *ch);
                assert_eq!(ch.track(), None);
            } else {
                let t = ch.track().unwrap();
                assert_eq!(usize::from(t), i - DrumRow::COUNT);
                assert_eq!(Channel::from_track(t), *ch);
            }
        }
        assert_eq!(Channel::from_index(14), Channel::Snare);
        assert_eq!(Channel::COUNT, DrumRow::COUNT + MAX_TRACKS);
        assert_eq!(Channel::COUNT, 13);
        assert_eq!(Channel::Synth, Channel::Track(0));
        assert_eq!(
            Channel::Track(9).index(),
            Channel::Track(1).index(),
            "wraps"
        );
        assert_eq!(Channel::from_track(8), Channel::Track(0));
        assert_eq!(Channel::Track(0).label(), "T1");
        assert_eq!(Channel::Track(7).label(), "T8");
        assert_eq!(Channel::Track(2).key(), "track_3");
        let keys: std::collections::HashSet<&str> = Channel::ALL.iter().map(|c| c.key()).collect();
        assert_eq!(keys.len(), Channel::COUNT);
    }

    #[test]
    fn defaults_match_the_spec() {
        let m = MixerState::default();
        for ch in Channel::ALL {
            let s = m.strip(ch);
            assert_eq!(s.level_db, 0.0);
            assert_eq!(s.pan, 0.0);
            assert!(!s.mute && !s.solo);
            let (d, r) = match ch {
                Channel::Track(_) => (1.0, 1.0),
                Channel::Snare | Channel::Clap => (0.0, DRUM_REVERB_SEND),
                _ => (0.0, 0.0),
            };
            assert_eq!(s.delay_send, d, "{ch:?}");
            assert_eq!(s.reverb_send, r, "{ch:?}");
            assert_eq!(m.effective_gain(ch), 1.0);
            assert_eq!(m.pan_gains(ch), (1.0, 1.0));
            for p in ChannelParam::ALL {
                assert_eq!(m.get(ch, p), p.default_value(ch), "{ch:?} {p:?}");
            }
        }
        assert_eq!(Strip::default(), Strip::for_channel(Channel::Kick));
    }

    #[test]
    fn set_clamps_and_snaps_toggles() {
        let mut m = MixerState::default();
        assert_eq!(
            m.set(Channel::Kick, ChannelParam::Level, 100.0),
            MAX_LEVEL_DB
        );
        assert_eq!(
            m.set(Channel::Kick, ChannelParam::Level, -100.0),
            MIN_LEVEL_DB
        );
        assert_eq!(m.set(Channel::Kick, ChannelParam::Pan, -7.0), -1.0);
        assert_eq!(m.set(Channel::Kick, ChannelParam::DelaySend, 2.0), 1.0);
        assert_eq!(m.set(Channel::Kick, ChannelParam::Mute, 0.7), 1.0);
        assert!(m.strip(Channel::Kick).mute);
        assert_eq!(m.set(Channel::Kick, ChannelParam::Mute, 0.2), 0.0);
        assert!(!m.strip(Channel::Kick).mute);
        assert_eq!(
            m.set(Channel::Synth, ChannelParam::ReverbSend, f32::NAN),
            1.0
        );
        assert_eq!(
            m.set(Channel::Kick, ChannelParam::ReverbSend, f32::NAN),
            0.0
        );
        for ch in Channel::ALL {
            for p in ChannelParam::ALL {
                let r = p.range();
                assert!(r.contains(&p.default_value(ch)), "{ch:?} {p:?}");
                assert_eq!(p.clamp(ch, *r.end() + 1.0), *r.end());
                assert_eq!(p.clamp(ch, *r.start() - 1.0), *r.start());
            }
        }
        let mut s = Strip {
            level_db: f32::INFINITY,
            pan: 3.0,
            mute: false,
            solo: true,
            delay_send: -1.0,
            reverb_send: f32::NAN,
        };
        s.normalise(Channel::Snare);
        assert_eq!(s.level_db, 0.0);
        assert_eq!(s.pan, 1.0);
        assert!(s.solo);
        assert_eq!(s.delay_send, 0.0);
        assert_eq!(s.reverb_send, DRUM_REVERB_SEND);
    }

    #[test]
    fn mute_and_solo_logic() {
        let mut m = MixerState::default();
        m.set(Channel::Kick, ChannelParam::Level, -6.0);
        let g = m.effective_gain(Channel::Kick);
        assert!((g - db_to_gain(-6.0)).abs() < 1e-6);
        m.set(Channel::Kick, ChannelParam::Mute, 1.0);
        assert_eq!(m.effective_gain(Channel::Kick), 0.0);
        assert!(!m.is_audible(Channel::Kick));
        assert_eq!(m.effective_gain(Channel::Snare), 1.0);
        m.set(Channel::Kick, ChannelParam::Mute, 0.0);

        // Solo the snare: everything else goes quiet, the snare keeps its level.
        m.set(Channel::Snare, ChannelParam::Solo, 1.0);
        assert!(m.any_solo());
        for ch in Channel::ALL {
            let expected = if ch == Channel::Snare { 1.0 } else { 0.0 };
            assert_eq!(m.effective_gain(ch), expected, "{ch:?}");
        }
        // A second solo adds that channel.
        m.set(Channel::Synth, ChannelParam::Solo, 1.0);
        assert_eq!(m.effective_gain(Channel::Synth), 1.0);
        assert_eq!(m.effective_gain(Channel::Kick), 0.0);
        // Mute beats solo.
        m.set(Channel::Snare, ChannelParam::Mute, 1.0);
        assert_eq!(m.effective_gain(Channel::Snare), 0.0);
        // Clearing the solos restores the rest.
        m.set(Channel::Snare, ChannelParam::Solo, 0.0);
        m.set(Channel::Synth, ChannelParam::Solo, 0.0);
        assert!(!m.any_solo());
        assert!((m.effective_gain(Channel::Kick) - g).abs() < 1e-6);
    }

    #[test]
    fn pan_law_is_equal_power_with_unity_centre() {
        assert_eq!(pan_gains(0.0), (1.0, 1.0));
        assert_eq!(pan_gains(-1.0), (core::f32::consts::SQRT_2, 0.0));
        assert_eq!(pan_gains(1.0), (0.0, core::f32::consts::SQRT_2));
        assert_eq!(pan_gains(-5.0), pan_gains(-1.0));
        assert_eq!(pan_gains(f32::NAN), (1.0, 1.0));
        let mut prev_l = f32::INFINITY;
        for i in 0..=100 {
            let pan = -1.0 + i as f32 * 0.02;
            let (l, r) = pan_gains(pan);
            assert!((l * l + r * r - 2.0).abs() < 1e-5, "pan {pan}: {l} {r}");
            assert!(l <= prev_l + 1e-6, "left gain must fall as pan goes right");
            assert!(l >= 0.0 && r >= 0.0);
            prev_l = l;
        }
        // Centre is -3 dB relative to a hard pan.
        let (hard, _) = pan_gains(-1.0);
        let (centre, _) = pan_gains(0.0);
        let db = 20.0 * (centre / hard).log10();
        assert!((db + 3.01).abs() < 0.02, "{db} dB");
    }

    #[test]
    fn serde_round_trip_and_missing_fields_default() {
        let mut m = MixerState::default();
        m.set(Channel::Clap, ChannelParam::Pan, 0.4);
        m.set(Channel::OpenHat, ChannelParam::Mute, 1.0);
        let json = serde_json::to_string(&m).unwrap();
        let back: MixerState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
        let partial: MixerState = serde_json::from_str("{}").unwrap();
        assert_eq!(partial, MixerState::default());
        // A pre-M10 file has six strips: the synth's becomes track 0's, the rest default.
        let six: Vec<Strip> = Channel::ALL[..6]
            .iter()
            .map(|&c| Strip::for_channel(c))
            .collect();
        let mut v = serde_json::json!({ "strips": six });
        v["strips"][5]["pan"] = serde_json::json!(0.25);
        let m: MixerState = serde_json::from_value(v).unwrap();
        assert_eq!(m.strip(Channel::Track(0)).pan, 0.25);
        assert_eq!(
            *m.strip(Channel::Track(1)),
            Strip::for_channel(Channel::Track(1))
        );
        let many: Vec<Strip> = vec![Strip::default(); Channel::COUNT + 1];
        assert!(
            serde_json::from_value::<MixerState>(serde_json::json!({ "strips": many })).is_err()
        );
        let mut m = MixerState::default();
        m.set(Channel::Track(0), ChannelParam::Level, -6.0);
        m.swap(Channel::Track(0), Channel::Track(3));
        assert_eq!(m.get(Channel::Track(3), ChannelParam::Level), -6.0);
        assert_eq!(m.get(Channel::Track(0), ChannelParam::Level), 0.0);
        let strip: Strip = serde_json::from_str(r#"{ "level_db": -3.0 }"#).unwrap();
        assert_eq!(strip.level_db, -3.0);
        assert_eq!(strip.pan, 0.0);
        assert_eq!(
            serde_json::to_string(&Channel::ClosedHat).unwrap(),
            "\"closed_hat\""
        );
        assert_eq!(
            serde_json::to_string(&ChannelParam::ReverbSend).unwrap(),
            "\"reverb_send\""
        );
    }
}
