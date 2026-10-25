//! cpal output stream that owns the `engine::Engine`.
//!
//! API: [`AudioOutput::start`] takes a closure `FnOnce(sample_rate: f32) -> (EngineHandle,
//! Engine)` (normally `engine::new_engine`). The device is opened first so the engine is
//! created with the device's *actual* sample rate; the `Engine` moves into the audio callback
//! and the `EngineHandle` is returned to the caller together with the running stream.
//!
//! Real-time rules (docs/PLAN.md): the data callback never allocates, locks, does I/O, logs or
//! panics. It renders into a pre-allocated interleaved-stereo scratch buffer sized for the
//! device's maximum buffer size (4096 frames when unknown) and copies into the device buffer
//! for 1, 2 or N channels. Oversized device buffers are truncated (the tail is silence) instead
//! of panicking.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig, SupportedBufferSize};
use engine::{Engine, EngineHandle};
use std::sync::atomic::{AtomicBool, Ordering};

/// Scratch size used when the device does not report a maximum buffer size, and the minimum
/// scratch size in any case (a few extra kilobytes buy robustness against odd hosts).
pub const FALLBACK_MAX_FRAMES: usize = 4096;

/// A running cpal output stream. Dropping it stops the audio.
pub struct AudioOutput {
    _stream: cpal::Stream,
    sample_rate: f32,
    channels: u16,
    sample_format: SampleFormat,
}

impl AudioOutput {
    /// Opens the default output device with its default config, builds the engine for that
    /// sample rate via `make_engine`, starts the stream and returns it with the UI-side handle.
    ///
    /// Errors are human-readable strings; the app logs them and keeps running without sound.
    pub fn start<F>(make_engine: F) -> Result<(AudioOutput, EngineHandle), String>
    where
        F: FnOnce(f32) -> (EngineHandle, Engine),
    {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "no default output device".to_string())?;
        let supported = device
            .default_output_config()
            .map_err(|e| format!("no default output config: {e}"))?;

        let sample_format = supported.sample_format();
        let channels = supported.channels();
        let sample_rate = supported.sample_rate() as f32;
        let max_frames = match *supported.buffer_size() {
            SupportedBufferSize::Range { max, .. } => (max as usize).max(FALLBACK_MAX_FRAMES),
            SupportedBufferSize::Unknown => FALLBACK_MAX_FRAMES,
        };
        let config: StreamConfig = supported.config();

        let (handle, engine) = make_engine(sample_rate);
        let renderer = Renderer::new(engine, channels as usize, max_frames);

        let stream = match sample_format {
            SampleFormat::F32 => build_stream::<f32>(&device, &config, renderer),
            SampleFormat::F64 => build_stream::<f64>(&device, &config, renderer),
            SampleFormat::I16 => build_stream::<i16>(&device, &config, renderer),
            SampleFormat::U16 => build_stream::<u16>(&device, &config, renderer),
            SampleFormat::I32 => build_stream::<i32>(&device, &config, renderer),
            SampleFormat::U32 => build_stream::<u32>(&device, &config, renderer),
            SampleFormat::I8 => build_stream::<i8>(&device, &config, renderer),
            SampleFormat::U8 => build_stream::<u8>(&device, &config, renderer),
            SampleFormat::I64 => build_stream::<i64>(&device, &config, renderer),
            SampleFormat::U64 => build_stream::<u64>(&device, &config, renderer),
            // 24-bit and anything new: ask the host for f32 instead (WASAPI shared mode and
            // most other hosts convert); if it refuses we report that error.
            other => build_stream::<f32>(&device, &config, renderer)
                .map_err(|e| format!("{e} (device format {other:?} is not supported directly)")),
        }?;

        stream
            .play()
            .map_err(|e| format!("failed to start output stream: {e}"))?;

        Ok((
            AudioOutput {
                _stream: stream,
                sample_rate,
                channels,
                sample_format,
            },
            handle,
        ))
    }

    /// The device sample rate the engine was created with.
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Number of output channels on the device stream.
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Sample format of the device stream (informational).
    pub fn sample_format(&self) -> SampleFormat {
        self.sample_format
    }

    /// `true` once the stream reported an error (the flag is sticky; the UI shows it).
    pub fn had_error(&self) -> bool {
        AUDIO_ERROR.load(Ordering::Relaxed)
    }
}

/// Set by the stream error callback (which cpal may run on the audio thread, so it must not log).
static AUDIO_ERROR: AtomicBool = AtomicBool::new(false);

fn build_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    mut renderer: Renderer,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    device
        .build_output_stream::<T, _, _>(
            *config,
            move |data: &mut [T], _info| renderer.fill(data),
            |_err| AUDIO_ERROR.store(true, Ordering::Relaxed),
            None,
        )
        .map_err(|e| format!("failed to build output stream: {e}"))
}

/// Audio-thread state: the engine plus the pre-allocated stereo scratch buffer.
///
/// Everything it does in [`Renderer::fill`] is allocation-free and panic-free.
pub struct Renderer {
    engine: Engine,
    channels: usize,
    max_frames: usize,
    scratch: Vec<f32>,
}

impl Renderer {
    /// Allocates the scratch buffer up front (`max_frames` stereo frames).
    pub fn new(engine: Engine, channels: usize, max_frames: usize) -> Self {
        let channels = channels.max(1);
        let max_frames = max_frames.max(1);
        Self {
            engine,
            channels,
            max_frames,
            scratch: vec![0.0; max_frames * 2],
        }
    }

    /// Renders one device buffer. Real-time safe.
    pub fn fill<T>(&mut self, out: &mut [T])
    where
        T: SizedSample + FromSample<f32>,
    {
        let channels = self.channels;
        let device_frames = out.len() / channels;
        let frames = device_frames.min(self.max_frames);

        let scratch = &mut self.scratch[..frames * 2];
        self.engine.render(scratch, frames);

        let (body, tail) = out.split_at_mut(frames * channels);
        let (stereo_frames, _) = scratch.as_chunks::<2>();
        match channels {
            1 => {
                for (dst, [l, r]) in body.iter_mut().zip(stereo_frames) {
                    *dst = T::from_sample(0.5 * (l + r));
                }
            }
            2 => {
                for (dst, src) in body.iter_mut().zip(scratch.iter()) {
                    *dst = T::from_sample(*src);
                }
            }
            _ => {
                for (frame, [l, r]) in body.chunks_exact_mut(channels).zip(stereo_frames) {
                    frame[0] = T::from_sample(*l);
                    frame[1] = T::from_sample(*r);
                    for extra in &mut frame[2..] {
                        *extra = T::from_sample(0.0);
                    }
                }
            }
        }
        // Device buffer larger than the scratch (or a ragged partial frame): silence, not panic.
        for s in tail {
            *s = T::from_sample(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::Message;

    fn renderer(channels: usize, max_frames: usize) -> (Renderer, EngineHandle) {
        let (mut handle, engine) = engine::new_engine(48_000.0);
        assert!(handle.send(Message::TestTone(true)));
        (Renderer::new(engine, channels, max_frames), handle)
    }

    #[test]
    fn stereo_fill_is_not_silent_with_test_tone() {
        let (mut r, _h) = renderer(2, 256);
        let mut out = vec![0.0f32; 256 * 2];
        r.fill(&mut out);
        assert!(out.iter().any(|s| s.abs() > 0.01));
        assert!(out.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn mono_and_multichannel_fill_do_not_panic() {
        let (mut r, _h) = renderer(1, 128);
        let mut mono = vec![0.0f32; 128];
        r.fill(&mut mono);
        assert!(mono.iter().any(|s| s.abs() > 0.01));

        let (mut r6, _h6) = renderer(6, 128);
        let mut six = vec![1.0f32; 128 * 6];
        r6.fill(&mut six);
        // Channels 3..6 are silence, 1..2 carry the tone.
        let (frames6, rest) = six.as_chunks::<6>();
        assert!(rest.is_empty());
        assert!(frames6.iter().all(|f| f[2..].iter().all(|s| *s == 0.0)));
        assert!(frames6.iter().any(|f| f[0].abs() > 0.01));
    }

    #[test]
    fn oversized_device_buffer_is_truncated_not_panicking() {
        let (mut r, _h) = renderer(2, 64);
        let mut out = vec![1.0f32; 300 * 2 + 1]; // bigger than scratch, plus a ragged sample
        r.fill(&mut out);
        assert!(out[64 * 2..].iter().all(|s| *s == 0.0));
    }

    #[test]
    fn integer_formats_convert() {
        let (mut r, _h) = renderer(2, 64);
        let mut out = vec![0i16; 64 * 2];
        r.fill(&mut out);
        assert!(out.iter().any(|s| *s != 0));
    }
}
