//! Drag-painting of sequencer steps (QOL pass): press a cell and drag across others to set
//! them all to the first cell's new state, like FL Studio's step sequencer.
//!
//! A [`PaintStroke`] starts on the press: it remembers the row, the state the first cell was
//! set to (`on`) and which steps it has painted. While the button stays down, every cell of
//! that row the pointer passes over is painted to `on` once ([`PaintStroke::wants`]). A
//! stroke is one undo step (the caller pushes it when the stroke starts). A right-button
//! stroke paints "off" (erase). The bass lane erases only; its notes are entered by key.

use engine::DrumRow;

/// Which row a stroke paints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintRow {
    Drum(DrumRow),
    Bass,
}

/// One press-and-drag over a row of cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintStroke {
    /// The row the stroke started on; other rows are left alone.
    pub row: PaintRow,
    /// The state every cell of the stroke is set to.
    pub on: bool,
    /// Absolute steps already painted (so a cell is set once per stroke).
    painted: Vec<usize>,
}

impl PaintStroke {
    /// A stroke on `row` that sets cells to `on`.
    #[must_use]
    pub fn new(row: PaintRow, on: bool) -> Self {
        Self {
            row,
            on,
            painted: Vec::new(),
        }
    }

    /// Whether cell `step` of `row`, currently `current_on`, should be painted by this
    /// stroke: same row, not painted yet, and not already in the stroke's state.
    #[must_use]
    pub fn wants(&self, row: PaintRow, step: usize, current_on: bool) -> bool {
        self.row == row && !self.painted.contains(&step) && current_on != self.on
    }

    /// Records `step` as painted (also for the first cell, whose press set it).
    pub fn mark(&mut self, step: usize) {
        if !self.painted.contains(&step) {
            self.painted.push(step);
        }
    }

    /// Steps painted so far, in order.
    #[must_use]
    pub fn painted(&self) -> &[usize] {
        &self.painted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stroke_paints_each_cell_of_its_row_once() {
        let kick = PaintRow::Drum(DrumRow::Kick);
        let mut stroke = PaintStroke::new(kick, true);
        stroke.mark(1);
        assert!(!stroke.wants(kick, 1, true), "the pressed cell is done");
        assert!(stroke.wants(kick, 2, false));
        assert!(!stroke.wants(kick, 2, true), "already in the target state");
        stroke.mark(2);
        assert!(!stroke.wants(kick, 2, false), "painted once per stroke");
        assert!(stroke.wants(kick, 3, false));
        assert!(
            !stroke.wants(PaintRow::Drum(DrumRow::Snare), 3, false),
            "other rows are left alone"
        );
        assert!(!stroke.wants(PaintRow::Bass, 3, false));
        stroke.mark(3);
        stroke.mark(3);
        assert_eq!(stroke.painted(), &[1, 2, 3]);
    }

    #[test]
    fn an_erase_stroke_turns_cells_off() {
        let snare = PaintRow::Drum(DrumRow::Snare);
        let mut stroke = PaintStroke::new(snare, false);
        stroke.mark(4);
        assert!(stroke.wants(snare, 5, true));
        assert!(!stroke.wants(snare, 6, false), "already off");
        let bass = PaintStroke::new(PaintRow::Bass, false);
        assert!(bass.wants(PaintRow::Bass, 0, true));
        assert!(!bass.wants(snare, 0, true));
    }
}
