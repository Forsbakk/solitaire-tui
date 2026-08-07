//! Where every card sits on screen.
//!
//! This module is purely geometric — it knows column *lengths*, not cards — and
//! it is the single source of truth for the board's shape. The renderer draws
//! from a [`BoardLayout`] and mouse clicks are resolved against the same one, so
//! a click can never disagree with what is on screen.

use ratatui::layout::Rect;

use crate::game::COLUMNS;

/// Width of a card box, `┌────┐`.
pub const CARD_W: u16 = 6;
/// Blank columns between card boxes.
pub const CARD_GAP: u16 = 1;
/// Rows in a fully drawn card box.
pub const CARD_H: u16 = 3;

pub const BOARD_W: u16 = COLUMNS as u16 * CARD_W + (COLUMNS as u16 - 1) * CARD_GAP;
/// Below this the elided layout can no longer show a card, a marker and borders.
pub const MIN_BOARD_H: u16 = 6;

/// How tightly cards are stacked. Chosen per frame for the whole board, so all
/// seven columns stay visually aligned.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Density {
    /// Two rows per covered card: a border row and its face.
    Roomy,
    /// One row per covered card: faces only, under a single top border.
    Tight,
}

/// The rows a single card answers for, including any border rows it owns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Slot {
    pub index: usize,
    pub top: u16,
    pub height: u16,
}

impl Slot {
    pub const fn contains(&self, y: u16) -> bool {
        y >= self.top && y < self.bottom()
    }

    pub const fn bottom(&self) -> u16 {
        self.top + self.height
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ColumnLayout {
    pub x: u16,
    pub top: u16,
    /// Visible cards, in board order. Elided cards have no slot.
    pub slots: Vec<Slot>,
    /// `(hidden card count, marker row)` when the column did not fit.
    pub elision: Option<(usize, u16)>,
    pub height: u16,
}

/// What a click landed on: a column always, a specific card when the click was
/// on one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hit {
    pub column: usize,
    pub card: Option<usize>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BoardLayout {
    pub area: Rect,
    pub density: Density,
    pub columns: Vec<ColumnLayout>,
}

impl BoardLayout {
    /// Resolve a screen position. Any row inside a column's width counts as
    /// that column, so clicking below a pile still targets it as a drop.
    pub fn hit(&self, x: u16, y: u16) -> Option<Hit> {
        if y < self.area.y || y >= self.area.y + self.area.height {
            return None;
        }
        let column = self
            .columns
            .iter()
            .position(|c| x >= c.x && x < c.x + CARD_W)?;
        let card = self.columns[column]
            .slots
            .iter()
            .find(|slot| slot.contains(y))
            .map(|slot| slot.index);
        Some(Hit { column, card })
    }
}

const fn roomy_height(cards: usize) -> u16 {
    if cards == 0 {
        CARD_H
    } else {
        2 * cards as u16 + 1
    }
}

const fn tight_height(cards: usize) -> u16 {
    if cards == 0 { CARD_H } else { cards as u16 + 2 }
}

/// Lay out the board inside `area`.
///
/// `pins` names one card per column that must stay visible even if the column
/// has to be elided — the cursor, or the head of a held tail. Returns `None`
/// when `area` is too small to draw a usable board.
pub fn compute(
    lengths: &[usize; COLUMNS],
    area: Rect,
    pins: &[Option<usize>; COLUMNS],
) -> Option<BoardLayout> {
    if area.width < BOARD_W || area.height < MIN_BOARD_H {
        return None;
    }

    let longest = *lengths.iter().max().unwrap_or(&0);
    let density = if roomy_height(longest) <= area.height {
        Density::Roomy
    } else {
        Density::Tight
    };

    let x0 = area.x + (area.width - BOARD_W) / 2;
    let columns = lengths
        .iter()
        .enumerate()
        .map(|(i, &len)| {
            let x = x0 + i as u16 * (CARD_W + CARD_GAP);
            column_layout(len, x, area.y, area.height, density, pins[i])
        })
        .collect();

    Some(BoardLayout {
        area,
        density,
        columns,
    })
}

fn column_layout(
    len: usize,
    x: u16,
    top: u16,
    available: u16,
    density: Density,
    pin: Option<usize>,
) -> ColumnLayout {
    if len == 0 {
        return ColumnLayout {
            x,
            top,
            slots: Vec::new(),
            elision: None,
            height: CARD_H,
        };
    }

    match density {
        // Card `i` owns its own top border row and its face row; the last card
        // also owns the closing border.
        Density::Roomy => {
            let slots = (0..len)
                .map(|i| Slot {
                    index: i,
                    top: top + 2 * i as u16,
                    height: if i + 1 == len { 3 } else { 2 },
                })
                .collect();
            ColumnLayout {
                x,
                top,
                slots,
                elision: None,
                height: roomy_height(len),
            }
        }
        Density::Tight if tight_height(len) <= available => {
            let slots = (0..len)
                .map(|i| Slot {
                    index: i,
                    top: top + 1 + i as u16,
                    // The last card also answers for the closing border row.
                    height: if i + 1 == len { 2 } else { 1 },
                })
                .collect();
            ColumnLayout {
                x,
                top,
                slots,
                elision: None,
                height: tight_height(len),
            }
        }
        // Too tall even for one row per card: show the pinned card (or the
        // top of the pile), a marker, and as much of the tail as fits. The
        // exposed card is always among them — it is the only drop target.
        Density::Tight => {
            let tail = (available - 4) as usize;
            let tail_start = len - tail;
            let head = match pin {
                Some(p) if p < tail_start => p,
                _ => 0,
            };

            let mut slots = Vec::with_capacity(tail + 1);
            let hidden = tail_start - head - 1;

            if hidden == 0 {
                // The pin sits right above the tail: draw them contiguously.
                let visible = len - head;
                for (row, index) in (head..len).enumerate() {
                    slots.push(Slot {
                        index,
                        top: top + 1 + row as u16,
                        height: if index + 1 == len { 2 } else { 1 },
                    });
                }
                return ColumnLayout {
                    x,
                    top,
                    slots,
                    elision: None,
                    height: visible as u16 + 2,
                };
            }

            slots.push(Slot {
                index: head,
                top: top + 1,
                height: 1,
            });
            let marker_row = top + 2;
            for (row, index) in (tail_start..len).enumerate() {
                slots.push(Slot {
                    index,
                    top: top + 3 + row as u16,
                    height: if index + 1 == len { 2 } else { 1 },
                });
            }

            ColumnLayout {
                x,
                top,
                slots,
                elision: Some((hidden, marker_row)),
                height: tail as u16 + 4,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(width: u16, height: u16) -> Rect {
        Rect::new(0, 0, width, height)
    }

    const NO_PINS: [Option<usize>; COLUMNS] = [None; COLUMNS];

    fn lengths(values: [usize; COLUMNS]) -> [usize; COLUMNS] {
        values
    }

    /// Every visible card must be reachable at its own rows, and no two cards
    /// may claim the same row — this is what keeps clicks honest.
    fn assert_slots_round_trip(layout: &BoardLayout) {
        for column in &layout.columns {
            let mut previous_bottom = None;
            for slot in &column.slots {
                if let Some(bottom) = previous_bottom {
                    assert!(
                        slot.top >= bottom,
                        "slots overlap in column at x {}",
                        column.x
                    );
                }
                previous_bottom = Some(slot.bottom());

                for y in slot.top..slot.bottom() {
                    let hit = layout.hit(column.x, y).expect("inside the board");
                    assert_eq!(
                        hit.card,
                        Some(slot.index),
                        "row {y} should resolve to card {}",
                        slot.index
                    );
                }
            }
            assert!(
                column.height <= layout.area.height,
                "column overflows the board area"
            );
        }
    }

    #[test]
    fn a_roomy_board_gives_every_card_two_rows() {
        let layout = compute(&lengths([7; COLUMNS]), area(80, 40), &NO_PINS).unwrap();
        assert_eq!(layout.density, Density::Roomy);

        let column = &layout.columns[0];
        assert_eq!(column.slots.len(), 7);
        assert_eq!(column.slots[0].height, 2);
        assert_eq!(
            column.slots[6].height, 3,
            "the last card shows its bottom border"
        );
        assert_eq!(column.height, 15);
        assert!(column.elision.is_none());
        assert_slots_round_trip(&layout);
    }

    #[test]
    fn tall_boards_fall_back_to_one_row_per_card() {
        let layout = compute(&lengths([18, 7, 7, 7, 7, 7, 7]), area(80, 24), &NO_PINS).unwrap();
        assert_eq!(layout.density, Density::Tight);

        let column = &layout.columns[0];
        assert_eq!(column.slots.len(), 18, "still every card");
        assert!(column.elision.is_none());
        assert_slots_round_trip(&layout);
    }

    #[test]
    fn columns_too_tall_to_fit_elide_their_middle() {
        let layout = compute(&lengths([30, 7, 7, 7, 7, 7, 7]), area(80, 20), &NO_PINS).unwrap();
        let column = &layout.columns[0];

        let (hidden, marker) = column.elision.expect("column was elided");
        assert_eq!(marker, layout.area.y + 2);
        assert_eq!(
            column.slots.len() + hidden,
            30,
            "every card is shown or counted"
        );
        assert_eq!(column.slots[0].index, 0, "the top card stays visible");
        assert_eq!(
            column.slots.last().unwrap().index,
            29,
            "the exposed card is always visible"
        );
        assert_slots_round_trip(&layout);
    }

    #[test]
    fn a_pinned_card_survives_elision() {
        let mut pins = NO_PINS;
        pins[0] = Some(4);
        let layout = compute(&lengths([30, 7, 7, 7, 7, 7, 7]), area(80, 20), &pins).unwrap();
        let column = &layout.columns[0];

        assert_eq!(
            column.slots[0].index, 4,
            "the pinned card takes the top slot"
        );
        assert_eq!(column.slots.last().unwrap().index, 29);
        assert_slots_round_trip(&layout);
    }

    #[test]
    fn a_pin_next_to_the_tail_needs_no_marker() {
        let layout_without =
            compute(&lengths([30, 0, 0, 0, 0, 0, 0]), area(80, 20), &NO_PINS).unwrap();
        let tail_start = layout_without.columns[0].slots[1].index;

        let mut pins = NO_PINS;
        pins[0] = Some(tail_start - 1);
        let layout = compute(&lengths([30, 0, 0, 0, 0, 0, 0]), area(80, 20), &pins).unwrap();
        let column = &layout.columns[0];

        assert!(column.elision.is_none(), "nothing left to hide");
        assert_eq!(column.slots[0].index, tail_start - 1);
        assert_slots_round_trip(&layout);
    }

    #[test]
    fn empty_columns_keep_a_placeholder_and_accept_drops() {
        let layout = compute(&lengths([0; COLUMNS]), area(80, 24), &NO_PINS).unwrap();
        let column = &layout.columns[3];

        assert!(column.slots.is_empty());
        assert_eq!(column.height, CARD_H);

        let hit = layout.hit(column.x, layout.area.y + 1).unwrap();
        assert_eq!(hit.column, 3);
        assert_eq!(hit.card, None, "no card, but still a drop target");
    }

    #[test]
    fn clicks_map_to_the_column_under_them() {
        let layout = compute(&lengths([7; COLUMNS]), area(80, 30), &NO_PINS).unwrap();

        for (i, column) in layout.columns.iter().enumerate() {
            assert_eq!(layout.hit(column.x, layout.area.y).unwrap().column, i);
            assert_eq!(
                layout
                    .hit(column.x + CARD_W - 1, layout.area.y)
                    .unwrap()
                    .column,
                i
            );
            // The gap between columns belongs to neither.
            if i + 1 < COLUMNS {
                assert!(layout.hit(column.x + CARD_W, layout.area.y).is_none());
            }
        }
        assert!(layout.hit(0, layout.area.y + layout.area.height).is_none());
    }

    #[test]
    fn a_cramped_area_has_no_layout() {
        assert!(compute(&lengths([7; COLUMNS]), area(BOARD_W - 1, 24), &NO_PINS).is_none());
        assert!(compute(&lengths([7; COLUMNS]), area(80, MIN_BOARD_H - 1), &NO_PINS).is_none());
        assert!(compute(&lengths([7; COLUMNS]), area(BOARD_W, MIN_BOARD_H), &NO_PINS).is_some());
    }
}
