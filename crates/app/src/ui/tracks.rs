//! The instrument track rows of the CHANNEL RACK (M10, part 2; Deck since M12): the names
//! the rack's track rows report, the row context menu (FL's right-click on a channel
//! button) and the inline DELETE confirm. The rows themselves are drawn by
//! [`crate::ui::sequencer`] (the TRACKS section under the DRUMS section).
//!
//! Row controls (WINNER section 5 "CHANNEL RACK"): the mute LED (click mutes, right-click
//! solos), `S`, the VOL / PAN mini knobs bound to the track's mixer strip, the insert number
//! box, the NAME button with the track's colour bar (a click selects the track, a
//! double-click renames inline, Shift + double-click solos, a right-click opens the menu)
//! and the sixteen note cells of the track's lane. The menu: Rename · Duplicate · Delete ·
//! Move up · Move down · Next colour · Link to free insert · Clear notes. Delete asks with
//! "Delete <name>?" YES / NO in the add row. Everything goes through `AppState`
//! (`add_track`, `duplicate_track`, `remove_track`, `rename_track`, `set_track_colour`,
//! `select_track`, `move_track`, `toggle_mute`, `toggle_solo`, `set_route`,
//! `link_track_to_free_insert`), so undo and the engine messages are the state's business.
//!
//! Accessible names (tests): rows [`row_name`] (`"Track row 1: Synth"`, a selected button),
//! the LEDs [`led_name`] (`"Synth LED"`, a check box lit while audible), `S` [`solo_name`]
//! (`"Solo track Synth"`), the knobs [`level_name`] / [`pan_name`] (`"Level track Synth"`,
//! sliders), the number boxes [`insert_box_name`] (`"Insert box Synth"`; the picker's
//! entries are `ui::mixer::pick_name`), the cells `"Synth step 1"`, the menu entries
//! [`RENAME_LABEL`], [`DUP_LABEL`], [`DEL_LABEL`], [`MOVE_UP_NAME`], [`MOVE_DOWN_NAME`],
//! [`COLOUR_LABEL`], [`LINK_NAME`], [`CLEAR_NOTES_LABEL`], the add row [`ADD_NAME`], the
//! confirm [`delete_question`] with [`YES_LABEL`] / [`NO_LABEL`].

use egui::{Id, Ui};
use engine::TrackId;

/// Accessible name of the add row's button.
pub const ADD_NAME: &str = "Add track";
/// The add row's text.
pub const ADD_LABEL: &str = "Add track";
/// The menu entries.
pub const RENAME_LABEL: &str = "Rename";
pub const DUP_LABEL: &str = "Duplicate";
pub const DEL_LABEL: &str = "Delete";
pub const MOVE_UP_NAME: &str = "Move up";
pub const MOVE_DOWN_NAME: &str = "Move down";
pub const COLOUR_LABEL: &str = "Next colour";
pub const LINK_NAME: &str = "Link to free insert";
pub const CLEAR_NOTES_LABEL: &str = "Clear notes";
/// The drum row menu's entries.
pub const MUTE_LABEL: &str = "Mute";
pub const UNMUTE_LABEL: &str = "Unmute";
pub const SOLO_LABEL: &str = "Solo";
pub const UNSOLO_LABEL: &str = "Unsolo";
pub const CLEAR_ROW_LABEL: &str = "Clear row";
/// The confirm's YES / NO buttons.
pub const YES_LABEL: &str = "Yes";
pub const NO_LABEL: &str = "No";
/// The hint in the add row's tooltip.
pub const HINT: &str = "Keys play the selected track";

/// Accessible name of the row of the `index`-th (0-based) active track: `"Track row 1: Synth"`.
#[must_use]
pub fn row_name(index: usize, name: &str) -> String {
    format!("Track row {}: {name}", index + 1)
}

/// Accessible name of a row's mute LED: `"Synth LED"` (lit while audible).
#[must_use]
pub fn led_name(name: &str) -> String {
    format!("{name} LED")
}

/// Accessible name of a track's `S` button: `"Solo track Synth"` (the mixer's strips are
/// `"Solo Synth"`).
#[must_use]
pub fn solo_name(name: &str) -> String {
    format!("Solo track {name}")
}

/// Accessible name of a drum row's `S` button: `"Solo row Kick"`.
#[must_use]
pub fn drum_solo_name(name: &str) -> String {
    format!("Solo row {name}")
}

/// Accessible name of a track's VOL knob: `"Level track Synth"` (a slider; the mixer's
/// fader is `"Synth level"`).
#[must_use]
pub fn level_name(name: &str) -> String {
    format!("Level track {name}")
}

/// Accessible name of a track's PAN knob: `"Pan track Synth"`.
#[must_use]
pub fn pan_name(name: &str) -> String {
    format!("Pan track {name}")
}

/// Accessible name of a drum row's VOL knob: `"Kick volume"`.
#[must_use]
pub fn drum_volume_name(name: &str) -> String {
    format!("{name} volume")
}

/// Accessible name of a drum row's TUNE knob: `"Kick tune"`.
#[must_use]
pub fn drum_tune_name(name: &str) -> String {
    format!("{name} tune")
}

/// The inline confirm's question: `"Delete Synth?"`.
#[must_use]
pub fn delete_question(name: &str) -> String {
    format!("Delete {name}?")
}

/// Accessible name of a track's insert number box: `"Insert box Synth"` (the mixer's source
/// strip box is `"Insert of Synth"`, a drum row's `"Row insert Kick"`).
#[must_use]
pub fn insert_box_name(name: &str) -> String {
    format!("Insert box {name}")
}

/// Where the pending DEL confirm lives (`ui.data`, the slot waiting for YES / NO).
fn pending_delete_id() -> Id {
    Id::NULL.with("tracks_panel_pending_delete")
}

/// The inline rename editor's id for `slot` (`neu::value_edit`): global, so it is the same
/// from any `Ui`.
#[must_use]
pub fn rename_id(slot: TrackId) -> Id {
    Id::NULL.with(("tracks_panel_rename", slot))
}

/// The slot whose DEL confirm is showing, if any.
#[must_use]
pub fn pending_delete(ui: &Ui) -> Option<TrackId> {
    ui.data(|d| d.get_temp::<TrackId>(pending_delete_id()))
}

/// Sets (or clears) the slot whose DEL confirm shows.
pub fn set_pending_delete(ui: &mut Ui, slot: Option<TrackId>) {
    ui.data_mut(|d| {
        if let Some(slot) = slot {
            d.insert_temp(pending_delete_id(), slot);
        } else {
            d.remove_temp::<TrackId>(pending_delete_id());
        }
    });
}

/// Whether the inline rename editor of `slot` is open.
#[must_use]
pub fn is_renaming(ui: &Ui, slot: TrackId) -> bool {
    neu::is_editing(ui, rename_id(slot))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessible_names() {
        assert_eq!(row_name(0, "Synth"), "Track row 1: Synth");
        assert_eq!(led_name("Lead"), "Lead LED");
        assert_eq!(solo_name("Lead"), "Solo track Lead");
        assert_eq!(drum_solo_name("Kick"), "Solo row Kick");
        assert_eq!(level_name("Lead"), "Level track Lead");
        assert_eq!(pan_name("Lead"), "Pan track Lead");
        assert_eq!(drum_volume_name("Kick"), "Kick volume");
        assert_eq!(drum_tune_name("Kick"), "Kick tune");
        assert_eq!(delete_question("Lead"), "Delete Lead?");
        assert_eq!(insert_box_name("Lead"), "Insert box Lead");
    }
}
