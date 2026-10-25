//! MIDI input (M4): a `midir` input port on its own thread feeding the engine directly.
//!
//! Two halves:
//!
//! - [`parse_midi`] / [`parse_midi_all`]: a pure, allocation-free parser over the bytes midir
//!   hands the callback. Note on (`0x9n`) and note off (`0x8n`) on any channel become
//!   [`NoteEvent`]s (velocity scaled `v / 127`; note on with velocity 0 is a note off). The
//!   parser keeps a [`RunningStatus`] so data bytes that arrive without a status byte reuse
//!   the last channel-voice status. Everything else (control change, program change,
//!   aftertouch, pitch bend, system common, sysex, real-time such as clock `0xF8` and active
//!   sensing `0xFE`) is skipped while still being tracked correctly for running status.
//! - [`MidiInput`]: [`MidiInput::list_ports`] and [`MidiInput::connect`]. The connection's
//!   callback parses each message, pushes `Message::NoteOn` / `Message::NoteOff` straight to
//!   the engine's MIDI queue (the [`MidiHandle`], never through the UI frame loop) and mirrors
//!   the [`NoteEvent`] to the UI over a bounded `std::sync::mpsc` channel (`try_send`, so a
//!   full or dropped receiver is ignored). The callback never allocates or logs. Dropping the
//!   [`MidiConnection`] disconnects; [`MidiConnection::close`] disconnects *and* returns the
//!   [`MidiHandle`] so the app can connect to another port later.

use std::fmt;
use std::sync::mpsc::SyncSender;

use engine::{Message, MidiHandle};
use midir::Ignore;

use crate::input::NoteEvent;

/// midir client name (what other software sees).
pub const CLIENT_NAME: &str = "BeatSlopper";

/// Capacity of the MIDI -> UI note-event channel (events beyond this between two frames are
/// dropped for the on-screen keys only; the engine already has them).
pub const UI_QUEUE_CAPACITY: usize = 1024;

/// The last channel-voice status byte seen, for running status.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunningStatus {
    status: Option<u8>,
}

impl RunningStatus {
    /// No status remembered yet.
    pub const fn new() -> Self {
        Self { status: None }
    }

    /// The remembered status byte (`0x80..=0xEF`), if any.
    #[must_use]
    pub fn status(&self) -> Option<u8> {
        self.status
    }

    /// Forgets the remembered status (system common / sysex do this per the MIDI spec).
    pub fn clear(&mut self) {
        self.status = None;
    }
}

/// Number of data bytes that follow a channel-voice status byte.
fn data_len(status: u8) -> usize {
    match status & 0xF0 {
        0xC0 | 0xD0 => 1, // program change, channel pressure
        _ => 2,           // note off/on, poly aftertouch, control change, pitch bend
    }
}

/// Turns a complete channel-voice message into a note event (only note on / off produce one).
fn note_event(status: u8, data1: u8, data2: u8) -> Option<NoteEvent> {
    let note = data1 & 0x7F;
    match status & 0xF0 {
        0x80 => Some(NoteEvent::Off { note }),
        0x90 if data2 & 0x7F == 0 => Some(NoteEvent::Off { note }),
        0x90 => Some(NoteEvent::On {
            note,
            velocity: f32::from(data2 & 0x7F) / 127.0,
        }),
        _ => None,
    }
}

/// Parses every note event in `bytes` (one midir message, which may carry several events
/// under running status) and hands each to `sink`, in order. Pure and allocation-free.
///
/// * Real-time bytes (`0xF8..=0xFF`) may appear anywhere and are skipped without disturbing
///   the message around them.
/// * System common and sysex (`0xF0..=0xF7`) clear the running status and end parsing of
///   this buffer (sysex payloads are ignored; midir is asked to drop them anyway).
/// * A trailing partial message (status without enough data bytes) produces nothing.
pub fn parse_midi_all(bytes: &[u8], running: &mut RunningStatus, mut sink: impl FnMut(NoteEvent)) {
    let mut data = [0u8; 2];
    let mut have = 0usize;
    for &byte in bytes {
        if byte >= 0xF8 {
            continue; // real-time: clock, start, stop, active sensing, reset
        }
        if byte >= 0xF0 {
            running.clear();
            return;
        }
        if byte >= 0x80 {
            running.status = Some(byte);
            have = 0;
            continue;
        }
        let Some(status) = running.status else {
            continue; // data byte with nothing to attach it to
        };
        data[have] = byte;
        have += 1;
        if have == data_len(status) {
            if let Some(event) = note_event(status, data[0], data[1]) {
                sink(event);
            }
            have = 0;
        }
    }
}

/// Parses `bytes` and returns the first note event in it, if any (see [`parse_midi_all`]).
#[must_use]
pub fn parse_midi(bytes: &[u8], running: &mut RunningStatus) -> Option<NoteEvent> {
    let mut first = None;
    parse_midi_all(bytes, running, |event| {
        if first.is_none() {
            first = Some(event);
        }
    });
    first
}

/// Failure to open a port. `midi` carries the [`MidiHandle`] back when it was not consumed
/// (unknown port, MIDI subsystem unavailable); it is `None` only when midir itself failed to
/// open an existing port, in which case the handle is gone with the failed connection.
pub struct ConnectError {
    /// Human-readable reason.
    pub message: String,
    /// The engine queue handle, when it could be recovered.
    pub midi: Option<MidiHandle>,
}

impl fmt::Debug for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectError")
            .field("message", &self.message)
            .field("midi", &self.midi.is_some())
            .finish()
    }
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ConnectError {}

/// Everything the midir callback needs; moved to the MIDI thread as midir's `data`.
struct CallbackData {
    midi: MidiHandle,
    ui_tx: SyncSender<NoteEvent>,
    running: RunningStatus,
    wake: Option<Box<dyn Fn() + Send>>,
}

/// An open MIDI input port. Dropping it disconnects.
pub struct MidiConnection {
    conn: Option<midir::MidiInputConnection<CallbackData>>,
    port_name: String,
}

impl MidiConnection {
    /// Name of the connected port.
    #[must_use]
    pub fn port_name(&self) -> &str {
        &self.port_name
    }

    /// Disconnects and returns the engine queue handle for the next connection.
    pub fn close(mut self) -> MidiHandle {
        let conn = self.conn.take().expect("connection is only taken by close");
        let (_input, data) = conn.close();
        data.midi
    }
}

impl fmt::Debug for MidiConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MidiConnection")
            .field("port_name", &self.port_name)
            .finish()
    }
}

/// MIDI input port discovery and connection (stateless; midir keeps the OS handles).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MidiInput;

impl MidiInput {
    /// Names of the input ports currently present (empty when the MIDI subsystem is
    /// unavailable). Ports whose name cannot be read are skipped.
    #[must_use]
    pub fn list_ports() -> Vec<String> {
        let Ok(input) = midir::MidiInput::new(CLIENT_NAME) else {
            return Vec::new();
        };
        input
            .ports()
            .iter()
            .filter_map(|port| input.port_name(port).ok())
            .collect()
    }

    /// Opens the input port called `port_name`. Every note on / off it sends goes to the
    /// engine through `midi` and to the UI through `ui_tx`; `wake` (if any) is called after
    /// each message that produced events, so the UI can request a repaint.
    pub fn connect(
        port_name: &str,
        midi: MidiHandle,
        ui_tx: SyncSender<NoteEvent>,
        wake: Option<Box<dyn Fn() + Send>>,
    ) -> Result<MidiConnection, ConnectError> {
        let mut input = match midir::MidiInput::new(CLIENT_NAME) {
            Ok(input) => input,
            Err(err) => {
                return Err(ConnectError {
                    message: format!("MIDI unavailable: {err}"),
                    midi: Some(midi),
                });
            }
        };
        input.ignore(Ignore::All);
        let Some(port) = input
            .ports()
            .into_iter()
            .find(|port| input.port_name(port).is_ok_and(|name| name == port_name))
        else {
            return Err(ConnectError {
                message: format!("MIDI port not found: {port_name}"),
                midi: Some(midi),
            });
        };
        let data = CallbackData {
            midi,
            ui_tx,
            running: RunningStatus::new(),
            wake,
        };
        match input.connect(&port, "beatslopper-in", on_message, data) {
            Ok(conn) => Ok(MidiConnection {
                conn: Some(conn),
                port_name: port_name.to_owned(),
            }),
            Err(err) => Err(ConnectError {
                message: format!("cannot open MIDI port {port_name}: {err}"),
                midi: None,
            }),
        }
    }
}

/// The midir callback: parse, push to the engine, mirror to the UI, wake the UI. No
/// allocation, no logging (this runs on midir's input thread).
fn on_message(_timestamp: u64, bytes: &[u8], data: &mut CallbackData) {
    let CallbackData {
        midi,
        ui_tx,
        running,
        wake,
    } = data;
    let mut any = false;
    parse_midi_all(bytes, running, |event| {
        let msg = match event {
            NoteEvent::On { note, velocity } => Message::NoteOn { note, velocity },
            NoteEvent::Off { note } => Message::NoteOff { note },
        };
        midi.send(msg);
        // Full or disconnected: the engine already has the note; only the key light is lost.
        let _ = ui_tx.try_send(event);
        any = true;
    });
    if any && let Some(wake) = wake {
        wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(bytes: &[u8]) -> Option<NoteEvent> {
        parse_midi(bytes, &mut RunningStatus::new())
    }

    fn all(bytes: &[u8], running: &mut RunningStatus) -> Vec<NoteEvent> {
        let mut out = Vec::new();
        parse_midi_all(bytes, running, |e| out.push(e));
        out
    }

    #[test]
    fn note_on_and_off_with_scaled_velocity() {
        assert_eq!(
            parse(&[0x90, 60, 127]),
            Some(NoteEvent::On {
                note: 60,
                velocity: 1.0
            })
        );
        let Some(NoteEvent::On { note, velocity }) = parse(&[0x90, 69, 64]) else {
            panic!("expected note on");
        };
        assert_eq!(note, 69);
        assert!((velocity - 64.0 / 127.0).abs() < 1e-6);
        assert_eq!(parse(&[0x80, 60, 64]), Some(NoteEvent::Off { note: 60 }));
        assert_eq!(parse(&[0x80, 60, 0]), Some(NoteEvent::Off { note: 60 }));
    }

    #[test]
    fn note_on_with_velocity_zero_is_note_off() {
        assert_eq!(parse(&[0x90, 60, 0]), Some(NoteEvent::Off { note: 60 }));
    }

    #[test]
    fn channel_is_ignored() {
        for channel in 0..16u8 {
            assert_eq!(
                parse(&[0x90 | channel, 48, 127]),
                Some(NoteEvent::On {
                    note: 48,
                    velocity: 1.0
                }),
                "channel {channel}"
            );
            assert_eq!(
                parse(&[0x80 | channel, 48, 10]),
                Some(NoteEvent::Off { note: 48 }),
                "channel {channel}"
            );
        }
    }

    #[test]
    fn running_status_reuses_the_last_status() {
        let mut running = RunningStatus::new();
        assert_eq!(
            parse_midi(&[0x90, 60, 100], &mut running),
            Some(NoteEvent::On {
                note: 60,
                velocity: 100.0 / 127.0
            })
        );
        assert_eq!(running.status(), Some(0x90));
        // Data bytes only: still a note on (and velocity 0 is still an off).
        assert_eq!(
            parse_midi(&[62, 127], &mut running),
            Some(NoteEvent::On {
                note: 62,
                velocity: 1.0
            })
        );
        assert_eq!(
            parse_midi(&[62, 0], &mut running),
            Some(NoteEvent::Off { note: 62 })
        );
        // Running status across a different status byte follows that one instead.
        assert_eq!(
            parse_midi(&[0x81, 62, 0], &mut running),
            Some(NoteEvent::Off { note: 62 })
        );
        assert_eq!(
            parse_midi(&[64, 0], &mut running),
            Some(NoteEvent::Off { note: 64 })
        );
    }

    #[test]
    fn several_events_in_one_buffer_under_running_status() {
        let mut running = RunningStatus::new();
        let events = all(&[0x90, 60, 100, 64, 100, 67, 0], &mut running);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].note(), 60);
        assert_eq!(events[1].note(), 64);
        assert_eq!(events[2], NoteEvent::Off { note: 67 });
        // `parse_midi` returns the first one.
        assert_eq!(
            parse(&[0x90, 60, 100, 64, 100]).map(NoteEvent::note),
            Some(60)
        );
    }

    #[test]
    fn other_channel_messages_are_ignored_but_tracked() {
        let mut running = RunningStatus::new();
        assert_eq!(parse_midi(&[0xB0, 7, 100], &mut running), None); // CC volume
        assert_eq!(parse_midi(&[7, 101], &mut running), None); // running CC
        assert_eq!(parse_midi(&[0xE0, 0, 64], &mut running), None); // pitch bend
        assert_eq!(parse_midi(&[0xC0, 5], &mut running), None); // program change (1 byte)
        assert_eq!(parse_midi(&[6], &mut running), None); // running program change
        assert_eq!(parse_midi(&[0xD0, 50], &mut running), None); // channel pressure
        assert_eq!(parse_midi(&[0xA0, 60, 50], &mut running), None); // poly aftertouch
        // A note arrives and parses normally afterwards.
        assert_eq!(
            parse_midi(&[0x90, 60, 1], &mut running),
            Some(NoteEvent::On {
                note: 60,
                velocity: 1.0 / 127.0
            })
        );
    }

    #[test]
    fn realtime_bytes_are_skipped_even_inside_a_message() {
        assert_eq!(parse(&[0xF8]), None); // clock
        assert_eq!(parse(&[0xFE]), None); // active sensing
        assert_eq!(parse(&[0xFA, 0xFC, 0xFF]), None); // start, stop, reset
        assert_eq!(
            parse(&[0x90, 0xF8, 60, 0xFE, 100]),
            Some(NoteEvent::On {
                note: 60,
                velocity: 100.0 / 127.0
            })
        );
        // Real-time bytes do not disturb running status.
        let mut running = RunningStatus::new();
        assert!(parse_midi(&[0x90, 60, 100], &mut running).is_some());
        assert_eq!(parse_midi(&[0xF8], &mut running), None);
        assert_eq!(
            parse_midi(&[62, 100], &mut running).map(NoteEvent::note),
            Some(62)
        );
    }

    #[test]
    fn sysex_and_system_common_are_ignored_and_clear_running_status() {
        let mut running = RunningStatus::new();
        assert!(parse_midi(&[0x90, 60, 100], &mut running).is_some());
        assert_eq!(
            parse_midi(&[0xF0, 0x7E, 0x7F, 0x09, 0x01, 0xF7], &mut running),
            None
        );
        assert_eq!(running.status(), None);
        assert_eq!(parse_midi(&[62, 100], &mut running), None); // no status to attach to
        assert_eq!(parse_midi(&[0xF2, 0, 0], &mut running), None); // song position
        assert_eq!(parse_midi(&[0xF1, 0], &mut running), None); // MTC quarter frame
        assert_eq!(parse_midi(&[0xF6], &mut running), None); // tune request
    }

    #[test]
    fn partial_messages_produce_nothing() {
        assert_eq!(parse(&[]), None);
        assert_eq!(parse(&[0x90]), None);
        assert_eq!(parse(&[0x90, 60]), None);
        assert_eq!(parse(&[60, 100]), None); // data with no status ever seen
        // A partial message does not leak into the next buffer's data (midir hands whole
        // messages, so a new buffer starts fresh).
        let mut running = RunningStatus::new();
        assert_eq!(parse_midi(&[0x90, 60], &mut running), None);
        assert_eq!(
            parse_midi(&[64, 100], &mut running),
            Some(NoteEvent::On {
                note: 64,
                velocity: 100.0 / 127.0
            })
        );
    }

    #[test]
    fn high_bit_data_is_never_out_of_range() {
        // Defensive: data bytes are masked to 7 bits.
        let mut running = RunningStatus::new();
        running.status = Some(0x90);
        let event = note_event(0x90, 0x7F, 0x7F).unwrap();
        assert_eq!(event.note(), 127);
        assert!(running.status().is_some());
    }

    #[test]
    fn list_ports_does_not_panic() {
        let ports = MidiInput::list_ports();
        for name in &ports {
            assert!(!name.is_empty());
        }
    }

    #[test]
    fn connecting_to_an_unknown_port_returns_the_handle() {
        let (mut handle, _engine) = engine::new_engine(48_000.0);
        let midi = handle.take_midi_handle().unwrap();
        let (tx, _rx) = std::sync::mpsc::sync_channel(4);
        let err = MidiInput::connect("no such port, surely 0xDEADBEEF", midi, tx, None)
            .expect_err("unknown port must fail");
        assert!(err.midi.is_some(), "the handle must come back");
        assert!(err.message.contains("no such port"), "{err}");
        assert!(format!("{err:?}").contains("ConnectError"));
    }
}
