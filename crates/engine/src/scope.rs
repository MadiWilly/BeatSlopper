//! Master-output audio tap for the UI visualizers (lock-free, allocation-free).
//!
//! [`Engine::render`](crate::Engine::render) copies every rendered block (the final
//! interleaved stereo output, after the limiter) into a pre-allocated `rtrb` ring through a
//! [`ScopeTap`]. The UI side holds the matching [`ScopeReader`] (taken once from
//! [`EngineHandle::take_scope_reader`](crate::EngineHandle::take_scope_reader)) and drains it
//! every frame into a [`ScopeWindow`], a fixed-size window of the most recent
//! [`SCOPE_WINDOW_FRAMES`] frames that stays contiguous and in time order, so an oscilloscope,
//! a vectorscope or an FFT can read it as plain slices.
//!
//! The ring holds [`SCOPE_CAPACITY_FRAMES`] frames (about 170 ms at 48 kHz). The audio thread
//! never waits: when the ring is full (the UI stalled) the rest of the block is skipped and the
//! samples are counted in [`ScopeTap::skipped`]; the reader drains everything it finds on the
//! next frame, so the window catches up after one UI frame. Whole frames only: the ring never
//! ends up with the channels swapped.
//!
//! Drum hits travel separately as a bitmask (one bit per [`DrumRow`](crate::DrumRow) index)
//! through an `AtomicU32` in the engine's shared state: the engine ORs the rows that triggered
//! during a block, the UI takes and clears them with
//! [`EngineHandle::take_hits`](crate::EngineHandle::take_hits). Hits are latched until read,
//! so a UI frame slower than an audio block still sees every hit.

use rtrb::{Consumer, Producer, RingBuffer};

/// Frames the tap ring can hold before the audio thread starts skipping blocks.
pub const SCOPE_CAPACITY_FRAMES: usize = 8192;

/// Frames a [`ScopeWindow`] keeps (the latest audio readable by the UI).
pub const SCOPE_WINDOW_FRAMES: usize = 2048;

/// Creates the tap pair: the [`ScopeTap`] lives in the engine, the [`ScopeReader`] on the
/// UI side. Allocation happens here, once.
#[must_use]
pub fn channel() -> (ScopeTap, ScopeReader) {
    let (producer, consumer) = RingBuffer::new(SCOPE_CAPACITY_FRAMES * 2);
    (
        ScopeTap {
            producer,
            skipped: 0,
        },
        ScopeReader { consumer },
    )
}

/// The engine-side producer. Lives inside the `Engine`; `render` pushes each block.
pub struct ScopeTap {
    producer: Producer<f32>,
    /// Samples skipped so far because the ring was full (diagnostics).
    skipped: u64,
}

impl ScopeTap {
    /// Copies an interleaved stereo block into the ring. Only whole frames are written and
    /// only as many as fit; the rest is skipped (never blocks, never allocates, never panics).
    /// A slice with an odd length loses its last sample.
    pub fn push_block(&mut self, interleaved: &[f32]) {
        let even = interleaved.len() & !1;
        let fit = self.producer.slots() & !1;
        let n = even.min(fit);
        // `push_entire_slice` only fails when fewer than `n` slots are free, and `n` was
        // clamped to the free slots just now (we are the only producer).
        if n > 0 && self.producer.push_entire_slice(&interleaved[..n]).is_err() {
            self.skipped += n as u64;
            return;
        }
        self.skipped += (even - n) as u64;
    }

    /// Samples dropped so far because the UI did not drain the ring in time.
    #[must_use]
    pub fn skipped(&self) -> u64 {
        self.skipped
    }

    /// Frames that can still be pushed before the tap starts skipping.
    #[must_use]
    pub fn free_frames(&self) -> usize {
        self.producer.slots() / 2
    }
}

/// The UI-side consumer of the tap.
pub struct ScopeReader {
    consumer: Consumer<f32>,
}

impl ScopeReader {
    /// Frames waiting in the ring.
    #[must_use]
    pub fn available_frames(&self) -> usize {
        self.consumer.slots() / 2
    }

    /// Pops one `(left, right)` frame, or `None` when the ring is empty.
    pub fn pop_frame(&mut self) -> Option<(f32, f32)> {
        if self.consumer.slots() < 2 {
            return None;
        }
        let l = self.consumer.pop().ok()?;
        let r = self.consumer.pop().ok()?;
        Some((l, r))
    }

    /// Moves every waiting frame into `window`, oldest first. Returns the number of frames.
    pub fn drain_into(&mut self, window: &mut ScopeWindow) -> usize {
        let mut n = 0;
        while let Some((l, r)) = self.pop_frame() {
            window.push(l, r);
            n += 1;
        }
        n
    }

    /// `true` once the engine side dropped its [`ScopeTap`].
    #[must_use]
    pub fn is_disconnected(&self) -> bool {
        self.consumer.is_abandoned()
    }
}

/// A fixed window of the latest [`SCOPE_WINDOW_FRAMES`] frames, in time order.
///
/// Each channel is stored twice back to back (a mirrored ring), so [`left`](Self::left),
/// [`right`](Self::right) and [`mono`](Self::mono) are always contiguous slices of exactly
/// [`SCOPE_WINDOW_FRAMES`] samples, oldest first, without copying. Lives on the UI side (it is
/// allocated once in [`ScopeWindow::new`]).
#[derive(Debug, Clone)]
pub struct ScopeWindow {
    left: Vec<f32>,
    right: Vec<f32>,
    mono: Vec<f32>,
    /// Write position in `0..SCOPE_WINDOW_FRAMES` (the oldest sample's slot).
    pos: usize,
    /// Frames pushed so far.
    total: u64,
    /// Largest absolute sample pushed since [`take_peak`](Self::take_peak).
    peak: f32,
}

impl Default for ScopeWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl ScopeWindow {
    /// A silent window.
    #[must_use]
    pub fn new() -> Self {
        Self {
            left: vec![0.0; SCOPE_WINDOW_FRAMES * 2],
            right: vec![0.0; SCOPE_WINDOW_FRAMES * 2],
            mono: vec![0.0; SCOPE_WINDOW_FRAMES * 2],
            pos: 0,
            total: 0,
            peak: 0.0,
        }
    }

    /// Appends one frame (the oldest frame falls out). Non-finite samples read as silence.
    pub fn push(&mut self, l: f32, r: f32) {
        let l = if l.is_finite() { l } else { 0.0 };
        let r = if r.is_finite() { r } else { 0.0 };
        let m = 0.5 * (l + r);
        let i = self.pos;
        self.left[i] = l;
        self.left[i + SCOPE_WINDOW_FRAMES] = l;
        self.right[i] = r;
        self.right[i + SCOPE_WINDOW_FRAMES] = r;
        self.mono[i] = m;
        self.mono[i + SCOPE_WINDOW_FRAMES] = m;
        self.pos = (i + 1) % SCOPE_WINDOW_FRAMES;
        self.total += 1;
        self.peak = self.peak.max(l.abs()).max(r.abs());
    }

    /// Appends an interleaved stereo block.
    pub fn push_block(&mut self, interleaved: &[f32]) {
        let (frames, _) = interleaved.as_chunks::<2>();
        for [l, r] in frames {
            self.push(*l, *r);
        }
    }

    /// The left channel, oldest first, exactly [`SCOPE_WINDOW_FRAMES`] samples.
    #[must_use]
    pub fn left(&self) -> &[f32] {
        &self.left[self.pos..self.pos + SCOPE_WINDOW_FRAMES]
    }

    /// The right channel, oldest first.
    #[must_use]
    pub fn right(&self) -> &[f32] {
        &self.right[self.pos..self.pos + SCOPE_WINDOW_FRAMES]
    }

    /// The mono mix `(l + r) / 2`, oldest first.
    #[must_use]
    pub fn mono(&self) -> &[f32] {
        &self.mono[self.pos..self.pos + SCOPE_WINDOW_FRAMES]
    }

    /// Frames pushed since creation.
    #[must_use]
    pub fn total_frames(&self) -> u64 {
        self.total
    }

    /// Largest absolute sample pushed since the last call (then resets to 0).
    pub fn take_peak(&mut self) -> f32 {
        std::mem::replace(&mut self.peak, 0.0)
    }

    /// `true` when every sample of the window is below `threshold` in magnitude.
    #[must_use]
    pub fn is_silent(&self, threshold: f32) -> bool {
        self.mono().iter().all(|s| s.abs() < threshold)
    }

    /// Resets the window to silence.
    pub fn clear(&mut self) {
        self.left.fill(0.0);
        self.right.fill(0.0);
        self.mono.fill(0.0);
        self.pos = 0;
        self.peak = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_stays_contiguous_and_ordered_across_the_wrap() {
        let mut w = ScopeWindow::new();
        let n = SCOPE_WINDOW_FRAMES as u32;
        for i in 0..(n + 100) {
            w.push(i as f32, -(i as f32));
        }
        let left = w.left();
        assert_eq!(left.len(), SCOPE_WINDOW_FRAMES);
        assert_eq!(left[0], 100.0);
        assert_eq!(left[SCOPE_WINDOW_FRAMES - 1], (n + 99) as f32);
        for pair in left.windows(2) {
            assert_eq!(pair[1] - pair[0], 1.0);
        }
        assert_eq!(w.right()[0], -100.0);
        assert_eq!(w.mono()[0], 0.0);
        assert_eq!(w.total_frames(), u64::from(n + 100));
        assert!(w.take_peak() >= (n + 99) as f32);
        assert_eq!(w.take_peak(), 0.0);
    }

    #[test]
    fn tap_round_trips_whole_frames_and_skips_when_full() {
        let (mut tap, mut reader) = channel();
        let block: Vec<f32> = (0..64).map(|i| i as f32).collect();
        tap.push_block(&block);
        assert_eq!(reader.available_frames(), 32);
        assert_eq!(reader.pop_frame(), Some((0.0, 1.0)));
        let mut w = ScopeWindow::new();
        assert_eq!(reader.drain_into(&mut w), 31);
        assert_eq!(reader.pop_frame(), None);

        // Odd-length slices lose the dangling sample; the ring stays frame aligned.
        tap.push_block(&[1.0, 2.0, 3.0]);
        assert_eq!(reader.available_frames(), 1);
        assert_eq!(reader.pop_frame(), Some((1.0, 2.0)));

        // Overflow: push more than the capacity; nothing panics, the excess is counted.
        let big = vec![0.5_f32; SCOPE_CAPACITY_FRAMES * 2 + 1000];
        tap.push_block(&big);
        assert_eq!(reader.available_frames(), SCOPE_CAPACITY_FRAMES);
        assert_eq!(tap.skipped(), 1000);
        assert_eq!(tap.free_frames(), 0);
        tap.push_block(&[1.0, 1.0]);
        assert_eq!(tap.skipped(), 1002);
        reader.drain_into(&mut w);
        assert_eq!(tap.free_frames(), SCOPE_CAPACITY_FRAMES);
        assert!(!reader.is_disconnected());
        drop(tap);
        assert!(reader.is_disconnected());
    }

    #[test]
    fn window_filters_non_finite_and_reports_silence() {
        let mut w = ScopeWindow::new();
        w.push(f32::NAN, f32::INFINITY);
        assert!(w.is_silent(1e-6));
        w.push_block(&[0.2, -0.2, 0.0, 0.0]);
        // Silence is judged on the mono mix: an anti-phase frame mixes to nothing.
        assert!(w.is_silent(0.1));
        assert_eq!(w.take_peak(), 0.2);
        w.push(0.3, 0.3);
        assert!(!w.is_silent(0.1));
        assert_eq!(w.take_peak(), 0.3);
        w.clear();
        assert!(w.is_silent(1e-9));
    }
}
