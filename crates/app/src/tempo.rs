//! Tempo workflow helpers (QOL pass): the BPM display format, tap tempo and the nudge step.
//!
//! The transport's BPM group shows a whole number while the tempo is integral and one
//! decimal otherwise ([`fmt_bpm`]: `"120"`, `"128.5"`), so a typed `128.5` or a tapped
//! tempo reads back exactly. [`TapTempo`] averages the intervals of the last [`TAP_WINDOW`]
//! taps (a pause longer than [`TAP_TIMEOUT`] starts a new series); the second tap already
//! yields a tempo. Times are seconds on the UI clock (`egui::InputState::time`), so tests
//! can drive them. Plain `+` / `-` (and Ctrl+Up / Ctrl+Down) nudge by [`NUDGE_BPM`].

use crate::app::{MAX_BPM, MIN_BPM};

/// Taps averaged for the tempo (the last four).
pub const TAP_WINDOW: usize = 4;
/// A gap longer than this (seconds) between taps starts a new series (slower than 30 BPM
/// anyway).
pub const TAP_TIMEOUT: f64 = 2.0;
/// How long (seconds) the TAP pill stays lit after a tap.
pub const TAP_FLASH: f64 = 0.25;
/// BPM moved by one nudge key press or wheel notch.
pub const NUDGE_BPM: f32 = 1.0;

/// The BPM display: a whole number when the tempo is (within 0.05), else one decimal.
#[must_use]
pub fn fmt_bpm(bpm: f32) -> String {
    let rounded = bpm.round();
    if (bpm - rounded).abs() < 0.05 {
        format!("{}", rounded as i32)
    } else {
        format!("{bpm:.1}")
    }
}

/// `bpm` moved by `steps` nudges and clamped to the transport's range; a non-integral tempo
/// first snaps to the nearest whole BPM in the nudge direction, so repeated nudges land on
/// whole numbers.
#[must_use]
pub fn nudge_bpm(bpm: f32, steps: i32) -> f32 {
    if steps == 0 {
        return bpm.clamp(MIN_BPM, MAX_BPM);
    }
    let base = if (bpm - bpm.round()).abs() < 0.05 {
        bpm.round()
    } else if steps > 0 {
        bpm.ceil() - 1.0
    } else {
        bpm.floor() + 1.0
    };
    (base + steps as f32 * NUDGE_BPM).clamp(MIN_BPM, MAX_BPM)
}

/// Tap-tempo state: the times (seconds on the UI clock) of the taps in the current series.
#[derive(Debug, Clone, Default)]
pub struct TapTempo {
    taps: Vec<f64>,
}

impl TapTempo {
    /// No taps yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a tap at `now`. Returns the tempo implied by the last [`TAP_WINDOW`] taps
    /// (the mean interval, clamped to the BPM range) once there are at least two taps in the
    /// series; `None` for the first tap of a series. A tap more than [`TAP_TIMEOUT`] after
    /// the previous one starts a new series.
    pub fn tap(&mut self, now: f64) -> Option<f32> {
        if let Some(last) = self.taps.last()
            && now - *last > TAP_TIMEOUT
        {
            self.taps.clear();
        }
        self.taps.push(now);
        if self.taps.len() > TAP_WINDOW {
            let excess = self.taps.len() - TAP_WINDOW;
            self.taps.drain(..excess);
        }
        self.bpm()
    }

    /// The tempo of the current series (`None` with fewer than two taps).
    #[must_use]
    pub fn bpm(&self) -> Option<f32> {
        if self.taps.len() < 2 {
            return None;
        }
        let first = self.taps[0];
        let last = self.taps[self.taps.len() - 1];
        let span = (last - first).max(0.0) as f32;
        let intervals = (self.taps.len() - 1) as f32;
        let mean = span / intervals;
        if mean <= 0.0 {
            return Some(MAX_BPM);
        }
        Some((60.0 / mean).clamp(MIN_BPM, MAX_BPM))
    }

    /// Number of taps in the current series.
    #[must_use]
    pub fn len(&self) -> usize {
        self.taps.len()
    }

    /// `true` before the first tap (or after a reset).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.taps.is_empty()
    }

    /// `true` for [`TAP_FLASH`] seconds after a tap (the pill lights up).
    #[must_use]
    pub fn is_flashing(&self, now: f64) -> bool {
        self.taps.last().is_some_and(|last| now - *last < TAP_FLASH)
    }

    /// Forgets the series.
    pub fn reset(&mut self) {
        self.taps.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bpm_shows_whole_numbers_and_one_decimal() {
        assert_eq!(fmt_bpm(120.0), "120");
        assert_eq!(fmt_bpm(120.02), "120");
        assert_eq!(fmt_bpm(128.5), "128.5");
        assert_eq!(fmt_bpm(99.97), "100");
        assert_eq!(fmt_bpm(133.333), "133.3");
    }

    #[test]
    fn nudges_step_whole_bpm_and_clamp() {
        assert_eq!(nudge_bpm(120.0, 1), 121.0);
        assert_eq!(nudge_bpm(120.0, -2), 118.0);
        assert_eq!(nudge_bpm(128.5, 1), 129.0);
        assert_eq!(nudge_bpm(128.5, -1), 128.0);
        assert_eq!(nudge_bpm(200.0, 1), 200.0);
        assert_eq!(nudge_bpm(60.0, -1), 60.0);
        assert_eq!(nudge_bpm(300.0, 0), 200.0);
    }

    #[test]
    fn four_even_taps_give_their_tempo() {
        let t0 = 100.0;
        let mut tap = TapTempo::new();
        assert_eq!(tap.tap(t0), None);
        let half = 0.5; // 120 BPM
        let bpm = tap.tap(t0 + half).unwrap();
        assert!((bpm - 120.0).abs() < 0.01, "{bpm}");
        tap.tap(t0 + 2.0 * half);
        let bpm = tap.tap(t0 + 3.0 * half).unwrap();
        assert!((bpm - 120.0).abs() < 0.01, "{bpm}");
        assert_eq!(tap.len(), 4);
        assert!(tap.is_flashing(t0 + 3.0 * half + 0.1));
        assert!(!tap.is_flashing(t0 + 3.0 * half + TAP_FLASH));
    }

    #[test]
    fn only_the_last_four_taps_count() {
        let t0 = 100.0;
        let mut tap = TapTempo::new();
        // Four slow taps (60 BPM), then four fast ones (150 BPM): the slow ones fall out.
        for i in 0..4 {
            tap.tap(t0 + f64::from(i));
        }
        assert!((tap.bpm().unwrap() - 60.0).abs() < 0.01);
        let mut t = t0 + 3.0;
        for _ in 0..4 {
            t += 0.4;
            tap.tap(t);
        }
        let bpm = tap.bpm().unwrap();
        assert!((bpm - 150.0).abs() < 0.01, "{bpm}");
        assert_eq!(tap.len(), TAP_WINDOW);
    }

    #[test]
    fn a_long_pause_starts_a_new_series_and_results_clamp() {
        let t0 = 100.0;
        let mut tap = TapTempo::new();
        tap.tap(t0);
        tap.tap(t0 + 0.5);
        assert_eq!(tap.tap(t0 + 10.0), None, "new series");
        assert_eq!(tap.len(), 1);
        // Very fast taps clamp to the maximum, very slow ones to the minimum.
        let bpm = tap.tap(t0 + 10.05);
        assert_eq!(bpm, Some(MAX_BPM));
        let mut slow = TapTempo::new();
        slow.tap(t0);
        assert_eq!(slow.tap(t0 + 1.9), Some(MIN_BPM));
        slow.reset();
        assert!(slow.is_empty());
    }
}
