//! On-screen piano keyboard (`.keys` / `.key` in `design/preview-a.html`).
//!
//! Two octaves of soft keys: white keys are 78x100 raised-m surface tiles with radius 12, black
//! keys are 44x62 `theme.key_black` tiles with radius 10 and raised-s shadows, painted on top
//! and centred on the 6 pt gap between the two white keys they sit between (standard layout: no
//! black key between E-F and B-C). Each key carries its computer-key cap (12/500 tracked) and
//! its note name (11/400, secondary) stacked at the bottom like the mock. A held key is OnGlow
//! (accent fill, accent glow) with on-accent text.
//!
//! Mouse: the whole keyboard is one `Sense::click_and_drag` widget; the pointer position is hit
//! tested against the black keys first, then the white keys. A press reports `pressed`, a
//! release (anywhere) reports `released` for the note that was pressed, and dragging onto
//! another key while the button is down releases the old note and presses the new one.

use egui::{Align2, Color32, Id, Pos2, Rect, Response, Role, Sense, Ui, WidgetInfo, pos2, vec2};

use crate::Theme;
use crate::shadow::{Depth, paint_soft_rect, paint_soft_rect_with_fill};
use crate::text::{self, TextKind};

/// What the piano shows: which notes are held and which computer key cap each note carries.
pub struct PianoModel<'a> {
    /// MIDI note of the leftmost C.
    pub base_note: u8,
    /// Number of white keys (14 = two octaves).
    pub white_keys: usize,
    /// Held state per MIDI note.
    pub held: &'a [bool; 128],
    /// Computer-key cap per MIDI note (uppercase letter, or ';').
    pub labels: &'a [Option<char>; 128],
}

/// Mouse interaction reported by [`piano_keys`] for this frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PianoOutput {
    /// A key was pressed with the mouse this frame.
    pub pressed: Option<u8>,
    /// A mouse-held key was released this frame.
    pub released: Option<u8>,
}

/// Width of a white key in points.
pub const WHITE_KEY_WIDTH: f32 = 78.0;
/// Height of a white key in points.
pub const WHITE_KEY_HEIGHT: f32 = 100.0;
/// Gap between white keys in points.
pub const KEY_GAP: f32 = 6.0;
/// Width of a black key in points.
pub const BLACK_KEY_WIDTH: f32 = 44.0;
/// Height of a black key in points.
pub const BLACK_KEY_HEIGHT: f32 = 62.0;
/// Horizontal distance between the left edges of two neighbouring white keys.
pub const WHITE_KEY_PITCH: f32 = WHITE_KEY_WIDTH + KEY_GAP;

/// Bottom padding of a white key's text stack (`.key` `padding-bottom`).
const WHITE_PAD_BOTTOM: f32 = 10.0;
/// Bottom padding of a black key's text stack (`.key.b` `padding-bottom`).
const BLACK_PAD_BOTTOM: f32 = 8.0;
/// Gap between the cap and the note name (`.key` `gap`).
const TEXT_GAP: f32 = 3.0;

/// Semitone offsets of the white keys within an octave (C D E F G A B).
const WHITE_SEMITONES: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
/// Note names by semitone within an octave.
const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Geometry of one key as laid out by [`key_layout`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PianoKey {
    /// MIDI note of the key.
    pub note: u8,
    /// Where the key is painted (black keys overlap the white keys below them).
    pub rect: Rect,
    /// `true` for a black (sharp) key.
    pub black: bool,
}

/// Mouse-held note for a keyboard, kept in egui's temporary memory between frames.
#[derive(Debug, Clone, Copy, Default)]
struct MouseHeld(Option<u8>);

/// Total width of a keyboard with `white_keys` white keys (14 -> 1170).
pub fn keyboard_width(white_keys: usize) -> f32 {
    (white_keys as f32 * WHITE_KEY_PITCH - KEY_GAP).max(0.0)
}

/// Size of the keyboard widget for `white_keys` white keys.
pub fn keyboard_size(white_keys: usize) -> egui::Vec2 {
    vec2(keyboard_width(white_keys), WHITE_KEY_HEIGHT)
}

/// Note name for a MIDI note, with the octave such that 60 is "C4" (e.g. "C#4", "A4").
pub fn note_name(note: u8) -> String {
    let octave = i32::from(note / 12) - 1;
    format!("{}{octave}", NOTE_NAMES[usize::from(note % 12)])
}

/// Lays out the keys of a keyboard whose top-left corner is `origin`: all white keys first
/// (left to right), then the black keys. Notes above 127 are not laid out.
///
/// White key `i` covers notes `base_note + 12 * (i / 7) + WHITE_SEMITONES[i % 7]`; a black key
/// (that white note + 1) sits on the boundary after every white key except E and B, and only
/// when a white key follows (so the keyboard never ends in a half key).
pub fn key_layout(origin: Pos2, base_note: u8, white_keys: usize) -> Vec<PianoKey> {
    let mut keys = Vec::with_capacity(white_keys * 2);
    let white_note = |i: usize| -> Option<u8> {
        let semis = 12 * (i / 7) + usize::from(WHITE_SEMITONES[i % 7]);
        let note = usize::from(base_note) + semis;
        u8::try_from(note).ok().filter(|&n| n <= 127)
    };
    for i in 0..white_keys {
        let Some(note) = white_note(i) else { break };
        let x = origin.x + i as f32 * WHITE_KEY_PITCH;
        keys.push(PianoKey {
            note,
            rect: Rect::from_min_size(pos2(x, origin.y), vec2(WHITE_KEY_WIDTH, WHITE_KEY_HEIGHT)),
            black: false,
        });
    }
    for i in 0..white_keys.saturating_sub(1) {
        let degree = i % 7;
        if degree == 2 || degree == 6 {
            continue; // E-F and B-C have no black key between them
        }
        let Some(white) = white_note(i) else { break };
        let Some(note) = white.checked_add(1) else {
            break;
        };
        if note > 127 {
            break;
        }
        // The boundary is the centre of the gap after white key `i`.
        let boundary_x = origin.x + (i + 1) as f32 * WHITE_KEY_PITCH - KEY_GAP * 0.5;
        keys.push(PianoKey {
            note,
            rect: Rect::from_min_size(
                pos2(boundary_x - BLACK_KEY_WIDTH * 0.5, origin.y),
                vec2(BLACK_KEY_WIDTH, BLACK_KEY_HEIGHT),
            ),
            black: true,
        });
    }
    keys
}

/// The key under `pos`, black keys taking priority over the white keys they overlap.
pub fn key_at(keys: &[PianoKey], pos: Pos2) -> Option<u8> {
    keys.iter()
        .filter(|k| k.black)
        .chain(keys.iter().filter(|k| !k.black))
        .find(|k| k.rect.contains(pos))
        .map(|k| k.note)
}

/// Draws two octaves of soft piano keys. Held keys light up in the accent colour.
///
/// Mouse presses and releases are reported in the returned [`PianoOutput`]; the widget keeps
/// track of the note the mouse is holding so that the release is reported for that note even
/// when the pointer has moved off the key. Dragging onto another key while the button is down
/// releases the previous note and presses the new one (glissando).
pub fn piano_keys(ui: &mut Ui, theme: &Theme, model: &PianoModel<'_>) -> PianoOutput {
    let (rect, response) =
        ui.allocate_exact_size(keyboard_size(model.white_keys), Sense::click_and_drag());
    let keys = key_layout(rect.min, model.base_note, model.white_keys);
    let output = handle_mouse(ui, &response, &keys);

    if ui.is_rect_visible(rect) {
        let hover_note = response.hover_pos().and_then(|p| key_at(&keys, p));
        for key in keys.iter().filter(|k| !k.black) {
            paint_key(ui, theme, model, key, hover_note == Some(key.note));
        }
        for key in keys.iter().filter(|k| k.black) {
            paint_key(ui, theme, model, key, hover_note == Some(key.note));
        }
    }

    response.widget_info(|| WidgetInfo::labeled(Role::Group, ui.is_enabled(), "Piano keys"));
    output
}

/// Updates the mouse-held note from this frame's pointer state and reports the transitions.
fn handle_mouse(ui: &Ui, response: &Response, keys: &[PianoKey]) -> PianoOutput {
    let state_id: Id = response.id.with("mouse_held");
    let MouseHeld(mut held) = ui
        .data(|d| d.get_temp::<MouseHeld>(state_id))
        .unwrap_or_default();
    let mut out = PianoOutput::default();

    let primary_down = ui.input(|i| i.pointer.primary_down());
    let hit = response
        .interact_pointer_pos()
        .and_then(|p| key_at(keys, p));

    if let Some(current) = held {
        if !primary_down || response.drag_stopped() {
            out.released = Some(current);
            held = None;
        } else if response.is_pointer_button_down_on() && hit != Some(current) {
            // Glissando: moved onto another key (or off the keyboard) while holding.
            out.released = Some(current);
            out.pressed = hit;
            held = hit;
        }
    } else if response.is_pointer_button_down_on() && primary_down {
        if let Some(note) = hit {
            out.pressed = Some(note);
            held = Some(note);
        }
    } else if response.clicked() {
        // Press and release in the same frame: report both for the key under the pointer.
        if let Some(note) = hit {
            out.pressed = Some(note);
            out.released = Some(note);
        }
    }

    ui.data_mut(|d| d.insert_temp(state_id, MouseHeld(held)));
    out
}

/// Paints one key with its cap and note name.
fn paint_key(ui: &Ui, theme: &Theme, model: &PianoModel<'_>, key: &PianoKey, hovered: bool) {
    let painter = ui.painter();
    let held = model.held[usize::from(key.note)];
    let (cap_color, note_color) = if held {
        let radius = if key.black {
            theme.radius_key_black
        } else {
            theme.radius_key_white
        };
        paint_soft_rect(painter, key.rect, radius, Depth::OnGlow, theme);
        (theme.on_accent, theme.on_accent)
    } else if key.black {
        paint_soft_rect_with_fill(
            painter,
            key.rect,
            theme.radius_key_black,
            Depth::RaisedS,
            theme.key_black,
            theme,
        );
        (theme.text_primary, black_key_note_color(theme, hovered))
    } else {
        paint_soft_rect(
            painter,
            key.rect,
            theme.radius_key_white,
            Depth::RaisedM,
            theme,
        );
        let note_color = if hovered {
            theme.text_primary
        } else {
            theme.text_secondary
        };
        (theme.text_primary, note_color)
    };

    // Text stack from the bottom: note name, 3 pt gap, cap.
    let pad = if key.black {
        BLACK_PAD_BOTTOM
    } else {
        WHITE_PAD_BOTTOM
    };
    let cx = key.rect.center().x;
    let name = note_name(key.note);
    let note_rect = text::paint_text(
        painter,
        pos2(cx, key.rect.max.y - pad),
        Align2::CENTER_BOTTOM,
        &name,
        TextKind::Label,
        note_color,
    );
    if let Some(cap) = model.labels[usize::from(key.note)] {
        let mut buf = [0u8; 4];
        let cap = cap.encode_utf8(&mut buf);
        text::paint_text(
            painter,
            pos2(cx, note_rect.min.y - TEXT_GAP),
            Align2::CENTER_BOTTOM,
            cap,
            TextKind::Title,
            cap_color,
        );
    }
}

/// Note-name colour on a black key: the secondary text colour reads on `key_black` in both
/// themes; hover lifts it to the primary colour like the white keys.
fn black_key_note_color(theme: &Theme, hovered: bool) -> Color32 {
    if hovered {
        theme.text_primary
    } else {
        theme.text_secondary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_names_follow_midi() {
        assert_eq!(note_name(60), "C4");
        assert_eq!(note_name(61), "C#4");
        assert_eq!(note_name(69), "A4");
        assert_eq!(note_name(71), "B4");
        assert_eq!(note_name(72), "C5");
        assert_eq!(note_name(0), "C-1");
    }

    #[test]
    fn two_octaves_have_14_white_and_10_black_keys() {
        let keys = key_layout(Pos2::ZERO, 60, 14);
        let whites: Vec<_> = keys.iter().filter(|k| !k.black).collect();
        let blacks: Vec<_> = keys.iter().filter(|k| k.black).collect();
        assert_eq!(whites.len(), 14);
        assert_eq!(blacks.len(), 10);
        assert_eq!(
            whites.iter().map(|k| k.note).collect::<Vec<_>>(),
            [60, 62, 64, 65, 67, 69, 71, 72, 74, 76, 77, 79, 81, 83]
        );
        assert_eq!(
            blacks.iter().map(|k| k.note).collect::<Vec<_>>(),
            [61, 63, 66, 68, 70, 73, 75, 78, 80, 82]
        );
        assert_eq!(keyboard_width(14), 1170.0);
        assert_eq!(whites[13].rect.max.x, 1170.0);
    }

    #[test]
    fn black_keys_are_centred_on_the_gap() {
        let keys = key_layout(pos2(10.0, 20.0), 60, 14);
        let c_sharp = keys.iter().find(|k| k.note == 61).unwrap();
        // Boundary between white 0 and 1: x = 10 + 84 - 3.
        assert_eq!(c_sharp.rect.center().x, 10.0 + WHITE_KEY_PITCH - 3.0);
        assert_eq!(c_sharp.rect.min.y, 20.0);
        assert_eq!(c_sharp.rect.size(), vec2(BLACK_KEY_WIDTH, BLACK_KEY_HEIGHT));
    }

    #[test]
    fn hit_test_prefers_black_keys() {
        let keys = key_layout(Pos2::ZERO, 60, 14);
        // Top of the first white key, where C#4 overlaps D4's and C4's edges.
        assert_eq!(key_at(&keys, pos2(39.0, 50.0)), Some(60));
        assert_eq!(key_at(&keys, pos2(81.0, 30.0)), Some(61));
        assert_eq!(key_at(&keys, pos2(81.0, 90.0)), None); // the gap, below the black key
        assert_eq!(key_at(&keys, pos2(90.0, 90.0)), Some(62));
        assert_eq!(key_at(&keys, pos2(-1.0, 50.0)), None);
    }

    #[test]
    fn layout_stops_at_the_top_of_the_midi_range() {
        let keys = key_layout(Pos2::ZERO, 120, 14);
        assert!(keys.iter().all(|k| k.note <= 127));
        assert_eq!(keys.iter().filter(|k| !k.black).count(), 5); // 120 122 124 125 127
    }
}
