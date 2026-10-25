//! On-screen piano keyboard (`design/round3/WINNER.md` section 5, "KEYBOARD strip").
//!
//! Two octaves of flat keys: white keys are 56 x 64 (no gap between them: 14 keys = 784),
//! black keys 32 x 38 centred on the boundary between the two white keys they sit between
//! (standard layout: no black key between E-F and B-C). A key is a `border_2` outline with
//! radius 0 0 4 4 over a `panel_3` to `panel_4` gradient; its note name (micro, `text_3`) and
//! its computer-key keycap (16 h) are stacked at the bottom (gap 2, padding-bottom 4). Black
//! keys use the black-key gradient, border and keycap colours of the theme. A held key is
//! `accent` with `accent_press` border and `on_accent` text, its keycap transparent.
//!
//! Mouse: the whole keyboard is one `Sense::click_and_drag` widget; the pointer position is hit
//! tested against the black keys first, then the white keys. A press reports `pressed`, a
//! release (anywhere) reports `released` for the note that was pressed, and dragging onto
//! another key while the button is down releases the old note and presses the new one.

use egui::epaint::CornerRadiusF32;
use egui::{
    Align2, Color32, Id, Pos2, Rect, Response, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo,
    pos2, vec2,
};

use crate::Theme;
use crate::chrome::{KEYCAP_HEIGHT, paint_keycap_coloured};
use crate::flat::vertical_gradient_rect_corners;
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

/// Width of a white key in points (56).
pub const WHITE_KEY_WIDTH: f32 = 56.0;
/// Height of a white key in points (64).
pub const WHITE_KEY_HEIGHT: f32 = 64.0;
/// Gap between white keys in points (0: Deck keys touch).
pub const KEY_GAP: f32 = 0.0;
/// Width of a black key in points (32).
pub const BLACK_KEY_WIDTH: f32 = 32.0;
/// Height of a black key in points (38).
pub const BLACK_KEY_HEIGHT: f32 = 38.0;
/// Horizontal distance between the left edges of two neighbouring white keys.
pub const WHITE_KEY_PITCH: f32 = WHITE_KEY_WIDTH + KEY_GAP;
/// Generic key sizes for other uses (40 x 64 white, 24 x 38 black).
pub const GENERIC_WHITE_KEY: egui::Vec2 = egui::Vec2::new(40.0, 64.0);
pub const GENERIC_BLACK_KEY: egui::Vec2 = egui::Vec2::new(24.0, 38.0);
/// Corner radius of the bottom corners (4).
pub const KEY_RADIUS: f32 = 4.0;

/// Bottom padding of a key's text stack.
const PAD_BOTTOM: f32 = 4.0;
/// Gap between the note name and the keycap.
const TEXT_GAP: f32 = 2.0;

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

/// Total width of a keyboard with `white_keys` white keys (14 -> 784).
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
        // The boundary between white key `i` and the next.
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

/// Draws two octaves of piano keys. Held keys light up in the accent colour.
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

/// Paints one key with its note name and keycap.
fn paint_key(ui: &Ui, theme: &Theme, model: &PianoModel<'_>, key: &PianoKey, hovered: bool) {
    let painter = ui.painter();
    let held = model.held[usize::from(key.note)];
    let corners = CornerRadiusF32 {
        nw: 0.0,
        ne: 0.0,
        sw: KEY_RADIUS,
        se: KEY_RADIUS,
    };
    // (top, bottom, border, note colour, keycap face, keycap border, keycap text)
    let (top, bottom, border, note_colour, cap_face, cap_border, cap_text) = if held {
        (
            theme.accent,
            theme.accent,
            theme.accent_press,
            theme.on_accent,
            Color32::TRANSPARENT,
            crate::theme::with_alpha(theme.on_accent, 0.5),
            theme.on_accent,
        )
    } else if key.black {
        let [cap_face, cap_border, cap_text] = theme.key_black_keycap;
        (
            theme.key_black_face[0],
            theme.key_black_face[1],
            theme.key_black_border,
            theme.key_black_text,
            cap_face,
            cap_border,
            cap_text,
        )
    } else {
        (
            theme.panel_3,
            theme.panel_4,
            theme.border_2,
            if hovered { theme.text_2 } else { theme.text_3 },
            theme.panel_2,
            theme.border_2,
            theme.text_3,
        )
    };
    painter.add(vertical_gradient_rect_corners(
        key.rect, corners, top, bottom,
    ));
    painter.rect_stroke(
        key.rect,
        egui::CornerRadius {
            nw: 0,
            ne: 0,
            sw: KEY_RADIUS as u8,
            se: KEY_RADIUS as u8,
        },
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );

    // Text stack from the bottom: keycap, 2 pt gap, note name.
    let cx = key.rect.center().x;
    let mut y = key.rect.max.y - PAD_BOTTOM;
    if let Some(cap) = model.labels[usize::from(key.note)] {
        let mut buf = [0u8; 4];
        let cap = cap.encode_utf8(&mut buf);
        let w = crate::chrome::keycap_size(painter, cap).x;
        paint_keycap_coloured(
            painter,
            pos2(cx - w * 0.5, y - KEYCAP_HEIGHT * 0.5),
            cap,
            cap_face,
            cap_border,
            cap_text,
        );
        y -= KEYCAP_HEIGHT + TEXT_GAP;
    }
    text::paint_text(
        painter,
        pos2(cx, y),
        Align2::CENTER_BOTTOM,
        &note_name(key.note),
        TextKind::Micro,
        note_colour,
    );
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
        assert_eq!(keyboard_width(14), 784.0);
        assert_eq!(whites[13].rect.max.x, 784.0);
    }

    #[test]
    fn black_keys_are_centred_on_the_boundary() {
        let keys = key_layout(pos2(10.0, 20.0), 60, 14);
        let c_sharp = keys.iter().find(|k| k.note == 61).unwrap();
        // Boundary between white 0 and 1: x = 10 + 56.
        assert_eq!(c_sharp.rect.center().x, 10.0 + WHITE_KEY_PITCH);
        assert_eq!(c_sharp.rect.min.x, 10.0 + WHITE_KEY_PITCH - 16.0);
        assert_eq!(c_sharp.rect.min.y, 20.0);
        assert_eq!(c_sharp.rect.size(), vec2(BLACK_KEY_WIDTH, BLACK_KEY_HEIGHT));
    }

    #[test]
    fn hit_test_prefers_black_keys() {
        let keys = key_layout(Pos2::ZERO, 60, 14);
        // Top of the first white key, where C#4 (40..72 wide, 38 tall) overlaps C4 and D4.
        assert_eq!(key_at(&keys, pos2(20.0, 30.0)), Some(60));
        assert_eq!(key_at(&keys, pos2(50.0, 30.0)), Some(61));
        assert_eq!(key_at(&keys, pos2(50.0, 50.0)), Some(60)); // below the black key
        assert_eq!(key_at(&keys, pos2(60.0, 50.0)), Some(62));
        assert_eq!(key_at(&keys, pos2(-1.0, 50.0)), None);
    }

    #[test]
    fn layout_stops_at_the_top_of_the_midi_range() {
        let keys = key_layout(Pos2::ZERO, 120, 14);
        assert!(keys.iter().all(|k| k.note <= 127));
        assert_eq!(keys.iter().filter(|k| !k.black).count(), 5); // 120 122 124 125 127
    }
}
