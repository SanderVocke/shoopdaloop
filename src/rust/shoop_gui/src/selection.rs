//! Which loops the next action applies to.
//!
//! Separate from the UI so the rules can be tested: what a click does depends on the
//! modifiers, and getting that subtly wrong is the kind of thing that is annoying to use
//! and invisible in a screenshot.
//!
//! The rules follow what a file manager does, because that is what people already expect:
//! a plain click replaces the selection, a toggle-click adds or removes one, and a range
//! click extends from the last thing clicked.

use std::collections::BTreeSet;

/// How a click was modified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    /// No modifier: this becomes the only selection.
    Plain,
    /// Ctrl or Cmd: add if absent, remove if present.
    Toggle,
    /// Shift: everything between the anchor and here.
    Range,
}

/// A loop's place in the grid, which is what a range is defined over.
///
/// Column-major, because the grid is read down a track and then across: a range within one
/// track is the common case and should be contiguous.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cell {
    pub track: usize,
    pub row: usize,
}

#[derive(Debug, Default, Clone)]
pub struct Selection {
    cells: BTreeSet<Cell>,
    /// Where a range extends from: the last plainly or toggle-clicked cell.
    anchor: Option<Cell>,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
    pub fn len(&self) -> usize {
        self.cells.len()
    }
    pub fn contains(&self, cell: Cell) -> bool {
        self.cells.contains(&cell)
    }
    /// Selected cells, ordered down each track and then across.
    pub fn cells(&self) -> impl Iterator<Item = &Cell> {
        self.cells.iter()
    }
    pub fn clear(&mut self) {
        self.cells.clear();
        self.anchor = None;
    }

    /// Applies a click.
    pub fn click(&mut self, cell: Cell, how: Click) {
        match how {
            Click::Plain => {
                self.cells.clear();
                self.cells.insert(cell);
                self.anchor = Some(cell);
            }
            Click::Toggle => {
                if !self.cells.remove(&cell) {
                    self.cells.insert(cell);
                }
                // The anchor moves even when deselecting, so a following range extends
                // from where the user last pointed rather than from somewhere older.
                self.anchor = Some(cell);
            }
            Click::Range => match self.anchor {
                // Without an anchor a range has no meaning, so treat it as a plain click
                // rather than doing nothing, which would feel broken.
                None => {
                    self.cells.clear();
                    self.cells.insert(cell);
                    self.anchor = Some(cell);
                }
                Some(anchor) => {
                    // Added to rather than replacing, so several ranges can be built up.
                    for c in cells_between(anchor, cell) {
                        self.cells.insert(c);
                    }
                }
            },
        }
    }

    /// Selects every cell in a grid of the given shape.
    pub fn select_all(&mut self, n_tracks: usize, n_rows: usize) {
        self.cells.clear();
        for track in 0..n_tracks {
            for row in 0..n_rows {
                self.cells.insert(Cell { track, row });
            }
        }
        self.anchor = Some(Cell { track: 0, row: 0 });
    }
}

/// Every cell in the rectangle the two corners describe.
///
/// A rectangle rather than a reading-order run: selecting the same two rows across three
/// tracks is what a looper user means by "these", and a reading-order range would drag in
/// whole tracks in between.
fn cells_between(a: Cell, b: Cell) -> impl Iterator<Item = Cell> {
    let (t0, t1) = (a.track.min(b.track), a.track.max(b.track));
    let (r0, r1) = (a.row.min(b.row), a.row.max(b.row));
    (t0..=t1).flat_map(move |track| (r0..=r1).map(move |row| Cell { track, row }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(track: usize, row: usize) -> Cell {
        Cell { track, row }
    }

    #[test]
    fn a_plain_click_replaces_the_selection() {
        let mut s = Selection::default();
        s.click(cell(0, 0), Click::Plain);
        s.click(cell(1, 2), Click::Plain);

        assert_eq!(s.len(), 1);
        assert!(s.contains(cell(1, 2)));
        assert!(!s.contains(cell(0, 0)));
    }

    #[test]
    fn a_toggle_click_adds_then_removes() {
        let mut s = Selection::default();
        s.click(cell(0, 0), Click::Toggle);
        s.click(cell(1, 1), Click::Toggle);
        assert_eq!(s.len(), 2);

        s.click(cell(0, 0), Click::Toggle);
        assert_eq!(s.len(), 1);
        assert!(s.contains(cell(1, 1)));
    }

    #[test]
    fn a_range_selects_the_rectangle_between_the_corners() {
        let mut s = Selection::default();
        s.click(cell(0, 0), Click::Plain);
        s.click(cell(2, 1), Click::Range);

        // Three tracks by two rows, not a reading-order run.
        assert_eq!(s.len(), 6);
        for track in 0..=2 {
            for row in 0..=1 {
                assert!(s.contains(cell(track, row)), "{track},{row} missing");
            }
        }
        assert!(!s.contains(cell(0, 2)));
    }

    #[test]
    fn a_range_works_in_either_direction() {
        let mut s = Selection::default();
        s.click(cell(2, 3), Click::Plain);
        s.click(cell(1, 2), Click::Range);

        assert_eq!(s.len(), 4);
        assert!(s.contains(cell(1, 2)));
        assert!(s.contains(cell(2, 3)));
    }

    #[test]
    fn a_range_without_an_anchor_behaves_like_a_plain_click() {
        let mut s = Selection::default();
        s.click(cell(1, 1), Click::Range);
        assert_eq!(s.len(), 1);
        assert!(s.contains(cell(1, 1)));
    }

    #[test]
    fn ranges_accumulate_rather_than_replacing() {
        let mut s = Selection::default();
        s.click(cell(0, 0), Click::Plain);
        s.click(cell(0, 1), Click::Range);
        assert_eq!(s.len(), 2);

        // A new anchor elsewhere, then another range: both survive.
        s.click(cell(3, 0), Click::Toggle);
        s.click(cell(3, 1), Click::Range);
        assert_eq!(s.len(), 4);
        assert!(s.contains(cell(0, 0)));
        assert!(s.contains(cell(3, 1)));
    }

    #[test]
    fn a_toggle_moves_the_anchor_even_when_deselecting() {
        let mut s = Selection::default();
        s.click(cell(0, 0), Click::Plain);
        // Deselect it; the anchor should still be here.
        s.click(cell(0, 0), Click::Toggle);
        assert!(s.is_empty());

        s.click(cell(0, 2), Click::Range);
        // Extended from 0,0 rather than from nothing.
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn select_all_covers_the_grid() {
        let mut s = Selection::default();
        s.select_all(4, 4);
        assert_eq!(s.len(), 16);
    }

    #[test]
    fn clearing_forgets_the_anchor_too() {
        let mut s = Selection::default();
        s.click(cell(1, 1), Click::Plain);
        s.clear();
        assert!(s.is_empty());

        // With no anchor, a range is a plain click.
        s.click(cell(3, 3), Click::Range);
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn cells_are_ordered_down_tracks_then_across() {
        let mut s = Selection::default();
        s.click(cell(1, 1), Click::Toggle);
        s.click(cell(0, 2), Click::Toggle);
        s.click(cell(0, 0), Click::Toggle);

        let order: Vec<_> = s.cells().copied().collect();
        assert_eq!(order, vec![cell(0, 0), cell(0, 2), cell(1, 1)]);
    }
}
