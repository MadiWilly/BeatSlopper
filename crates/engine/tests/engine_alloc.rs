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
use engine::{
    Channel, ChannelParam, DrumRow, INSERT_COUNT, InsertParam, MAX_TRACKS, Message, Note, ParamId,
    SlotKind, new_engine,
};

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
    // M11: every insert carries a compressor, a second slot of a rotating kind and an EQ3,
    // the drums are grouped onto insert 14 which feeds insert 16.
    let kinds = [
        SlotKind::Drive,
        SlotKind::Lofi,
        SlotKind::Chorus,
        SlotKind::Eq3,
    ];
    for n in 1..=INSERT_COUNT as u8 {
        engine.set_slot_kind(n, 0, SlotKind::Compressor);
        engine.set_slot_param(n, 0, 0, -30.0);
        engine.set_slot_kind(n, 1, kinds[usize::from(n) % kinds.len()]);
        engine.set_slot_kind(n, 2, SlotKind::Eq3);
        engine.set_insert(n, InsertParam::Eq3Low, 2.0);
    }
    for row in DrumRow::ALL {
        engine.set_route(Channel::from_row(row), 14);
    }
    engine.set_insert(14, InsertParam::Route, 16.0);
    let mut out = vec![0.0_f32; 512 * 2];
    engine.play();
    // One warm-up block so lazily initialised state (if any) is behind us.
    engine.render(&mut out, 512);

    let before = allocations();
    // Every send is asserted (the message text is only formatted on failure, so a passing
    // assert does not allocate): a dropped message would make the render below vacuously
    // allocation-free under a message it never saw.
    for block in 0..200_u32 {
        let sent = match block % 8 {
            0 => handle.send(Message::SwapTracks((block / 8) as u8 % 8, 3)),
            1 => handle.send(Message::CopyTrack {
                from: 1,
                to: (block / 8) as u8 % 8,
            }),
            2 => {
                handle.send(Message::SetTrackActive {
                    track: 7,
                    on: block % 16 == 2,
                }) && handle.send(Message::SetSlotKind {
                    insert: (block / 8) as u8 % 16 + 1,
                    slot: 1,
                    kind: if block % 16 == 2 {
                        SlotKind::Lofi
                    } else {
                        SlotKind::Chorus
                    },
                })
            }
            3 => {
                handle.send(Message::SetEditTrack((block / 8) as u8 % 8))
                    && handle.send(Message::NoteOn {
                        note: 60,
                        velocity: 0.8,
                    })
            }
            4 => {
                handle.send(Message::NoteOff { note: 60 })
                    && handle.send(Message::SetTrackParam {
                        track: 2,
                        id: ParamId::FilterCutoff,
                        value: 900.0,
                    })
            }
            5 => {
                handle.send(Message::SetNote {
                    pattern: 0,
                    track: 5,
                    index: 0,
                    note: Note::new(1, 2, 70, 0.7),
                }) && handle.send(Message::ClearNotes {
                    pattern: 1,
                    track: 5,
                })
            }
            6 => {
                handle.send(Message::SetMixer {
                    channel: Channel::Track(4),
                    param: ChannelParam::Pan,
                    value: -0.5,
                }) && handle.send(Message::SetParam(ParamId::ReverbSize, 0.7))
                    && handle.send(Message::SetInsert {
                        insert: 16,
                        param: InsertParam::Level,
                        value: -1.0,
                    })
                    && handle.send(Message::SetRoute {
                        channel: Channel::Track(1),
                        insert: 15,
                    })
            }
            _ => {
                handle.send(Message::AllNotesOff)
                    && handle.send(Message::ClearAll)
                    && handle.send(Message::SetSynthStep {
                        step: 3,
                        note: Some(50),
                        accent: true,
                    })
                    && handle.send(Message::SetSlotParam {
                        insert: 6,
                        slot: 0,
                        index: 0,
                        value: -24.0,
                    })
                    && handle.send(Message::SetInsert {
                        insert: 14,
                        param: InsertParam::Route,
                        value: 15.0,
                    })
                    && handle.send(Message::SetInsert {
                        insert: 15,
                        param: InsertParam::Eq3Mid,
                        value: 3.0,
                    })
            }
        };
        assert!(sent, "queue full at block {block}");
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
