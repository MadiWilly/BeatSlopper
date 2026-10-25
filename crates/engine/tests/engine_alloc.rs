//! Real-time rule check for the M10 engine: `render` (messages included) never allocates
//! once the engine exists, with all eight tracks active and every track message in flight.
//!
//! Same counting global allocator as `tests/fx_m8.rs` (per thread, so parallel tests do not
//! disturb each other); it lives in its own binary because a global allocator is per binary.

mod common;

use core::cell::Cell;
use std::alloc::{GlobalAlloc, Layout, System};

use common::{SAMPLE_RATE, all_finite};
use engine::presets;
use engine::{Channel, ChannelParam, DrumRow, MAX_TRACKS, Message, Note, ParamId, new_engine};

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct CountingAllocator;

impl CountingAllocator {
    fn bump() {
        let _ = ALLOCATIONS.try_with(|c| c.set(c.get() + 1));
    }
}

// SAFETY: every method forwards to `System` unchanged; the counter is a plain TLS `Cell`
// that itself never allocates.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        Self::bump();
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        Self::bump();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        Self::bump();
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn allocations() -> usize {
    ALLOCATIONS.with(Cell::get)
}

#[test]
fn eight_tracks_render_is_allocation_free() {
    let (mut handle, mut engine) = new_engine(SAMPLE_RATE);
    for t in 0..MAX_TRACKS as u8 {
        engine.set_track_active(t, true);
        presets::ALL[usize::from(t) % presets::ALL.len()].apply_to_track(&mut engine, t);
        engine.set_track_param(t, ParamId::Osc1Unison, 7.0);
        engine.set_track_param(t, ParamId::LofiMix, 0.5);
        engine.set_track_param(t, ParamId::ChorusMix, 0.5);
        engine.set_track_param(t, ParamId::SidechainAmount, 0.6);
        for i in 0..4u8 {
            engine.set_note(0, t, i, Note::new(2 * i, 4, 48 + 3 * i + t, 0.9));
        }
    }
    engine.set_drum_step(DrumRow::Kick, 0, true, true);
    engine.set_drum_step(DrumRow::Kick, 8, true, true);
    engine.set_drum_step(DrumRow::Snare, 4, true, false);
    engine.set_param(ParamId::DelayMix, 0.5);
    engine.set_param(ParamId::ReverbMix, 0.5);
    let mut out = vec![0.0_f32; 512 * 2];
    engine.play();
    // One warm-up block so lazily initialised state (if any) is behind us.
    engine.render(&mut out, 512);

    let before = allocations();
    for block in 0..200_u32 {
        match block % 8 {
            0 => {
                handle.send(Message::SwapTracks((block / 8) as u8 % 8, 3));
            }
            1 => {
                handle.send(Message::CopyTrack {
                    from: 1,
                    to: (block / 8) as u8 % 8,
                });
            }
            2 => {
                handle.send(Message::SetTrackActive {
                    track: 7,
                    on: block % 16 == 2,
                });
            }
            3 => {
                handle.send(Message::SetEditTrack((block / 8) as u8 % 8));
                handle.send(Message::NoteOn {
                    note: 60,
                    velocity: 0.8,
                });
            }
            4 => {
                handle.send(Message::NoteOff { note: 60 });
                handle.send(Message::SetTrackParam {
                    track: 2,
                    id: ParamId::FilterCutoff,
                    value: 900.0,
                });
            }
            5 => {
                handle.send(Message::SetNote {
                    pattern: 0,
                    track: 5,
                    index: 0,
                    note: Note::new(1, 2, 70, 0.7),
                });
                handle.send(Message::ClearNotes {
                    pattern: 1,
                    track: 5,
                });
            }
            6 => {
                handle.send(Message::SetMixer {
                    channel: Channel::Track(4),
                    param: ChannelParam::Pan,
                    value: -0.5,
                });
                handle.send(Message::SetParam(ParamId::ReverbSize, 0.7));
            }
            _ => {
                handle.send(Message::AllNotesOff);
                handle.send(Message::ClearAll);
                handle.send(Message::SetSynthStep {
                    step: 3,
                    note: Some(50),
                    accent: true,
                });
            }
        }
        engine.render(&mut out, 512);
        let _ = engine.meters();
        let _ = engine.position();
    }
    let after = allocations();
    assert!(all_finite(&out));
    assert_eq!(after, before, "allocations during render / messages");
    // Sanity check that the counter works at all.
    let v: Vec<f32> = vec![0.0; 16];
    assert!(allocations() > after);
    drop(v);
}
