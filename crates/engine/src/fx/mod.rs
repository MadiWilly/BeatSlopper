//! Effects. The M2 chain is drive -> delay -> reverb -> master gain + limiter.
//!
//! M0 ships only the master [`Limiter`]; later milestones add `drive`, `delay` and `reverb`
//! as sibling modules. Every effect exposes a per-sample `process` that is allocation-free
//! and panic-free, and flushes denormals on its feedback paths (see [`crate::dsp`]).

pub mod limiter;

pub use limiter::Limiter;
