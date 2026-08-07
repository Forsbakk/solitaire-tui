//! Where every card sits on screen.
//!
//! This module is purely geometric — it knows pile *lengths* and the board spec
//! the game declared, not cards — and it is the single source of truth for the
//! board's shape. The renderer draws from a [`BoardLayout`] and mouse clicks are
//! resolved against the same one, so a click can never disagree with what is on
//! screen.
//!
//! A board is rows of slots. A row whose piles fan downwards grows to fill the
//! height left over; every other row is one card tall. Scorpion is a single
//! fanned row; Klondike puts stock, waste and foundations in a fixed row above
//! its fanned tableau.

use ratatui::layout::Rect;

use crate::board::{BoardSpec, Fan, PileId};

/// Width of a card box, `┌────┐`.
pub const CARD_W: u16 = 6;
/// Blank columns between card boxes.
pub const CARD_GAP: u16 = 1;
/// Rows in a fully drawn card box.
pub const CARD_H: u16 = 3;
/// Blank rows between board rows.
pub const ROW_GAP: u16 = 1;
/// Columns between the left edges of cards fanned rightwards.
///
/// Four, so a covered card keeps three cells — its whole face bar the trailing
/// space, which is what a ten needs to show its suit as well as its rank. Three
/// would leave a ten reading as bare `10`.
///
/// The cost is that a three-card fan is 14 cells wide, and a row cannot fit a
/// card, a gap, that fan, a gap and four more cards into the 48 columns seven
/// slots give it. Klondike's waste therefore sits flush against its first
/// foundation rather than a column clear of it.
pub const SPREAD_STEP: u16 = 4;
/// Below this a fanned row can no longer show a card, a marker and borders.
pub const MIN_FAN_H: u16 = 6;

/// How tightly cards are stacked. Chosen per frame for the whole board, so all
/// fanned piles stay visually aligned.
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
pub struct PileLayout {
    pub pile: PileId,
    pub fan: Fan,
    pub x: u16,
    pub top: u16,
    /// How wide this pile answers for. Wider than a card only when it fans
    /// rightwards.
    pub width: u16,
    /// Cards the player can address, in board order. Elided cards have no slot,
    /// and a rightward fan gives a slot only to its exposed card.
    pub slots: Vec<Slot>,
    /// Cards drawn side by side as `(card index, x)`, exposed card last. Empty
    /// for a pile with no cards; one entry for a pile that shows only its top.
    pub spread: Vec<(usize, u16)>,
    /// `(hidden card count, marker row)` when the pile did not fit.
    pub elision: Option<(usize, u16)>,
    pub height: u16,
    /// The last row this pile answers for when resolving a click. A fanned pile
    /// claims everything down to the bottom of the board, so clicking below a
    /// short pile still targets it as a drop.
    pub claim_bottom: u16,
}

/// What a click landed on: a pile always, a specific card when the click was
/// on one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hit {
    pub pile: PileId,
    pub card: Option<usize>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BoardLayout {
    pub area: Rect,
    pub density: Density,
    pub piles: Vec<PileLayout>,
}

impl BoardLayout {
    /// Resolve a screen position. Any row a pile claims, within its width,
    /// counts as that pile.
    pub fn hit(&self, x: u16, y: u16) -> Option<Hit> {
        if y < self.area.y || y >= self.area.y + self.area.height {
            return None;
        }
        let found = self.piles.iter().find(|pile| {
            x >= pile.x && x < pile.x + pile.width && y >= pile.top && y < pile.claim_bottom
        })?;
        let card = found
            .slots
            .iter()
            .find(|slot| slot.contains(y))
            .map(|slot| slot.index);
        Some(Hit {
            pile: found.pile,
            card,
        })
    }

    pub fn pile(&self, id: PileId) -> Option<&PileLayout> {
        self.piles.iter().find(|pile| pile.pile == id)
    }
}

/// How many cells a pile drawn with `fan` needs.
pub const fn fan_width(fan: Fan) -> u16 {
    match fan {
        Fan::Right(shown) if shown > 1 => (shown as u16 - 1) * SPREAD_STEP + CARD_W,
        _ => CARD_W,
    }
}

/// How wide `spec`'s widest row is, in cells.
///
/// Measured from each spot's own left edge rather than from the slot count, so
/// a pile that fans rightwards into the gap beside it costs nothing extra.
pub fn board_width(spec: &BoardSpec) -> u16 {
    (0..spec.rows())
        .filter_map(|row| {
            spec.row(row)
                .map(|spot| spot.slot * (CARD_W + CARD_GAP) + fan_width(spot.fan))
                .max()
        })
        .max()
        .unwrap_or(0)
}

/// The least height `spec` can be drawn in.
pub fn min_board_height(spec: &BoardSpec) -> u16 {
    let rows = spec.rows();
    if rows == 0 {
        return 0;
    }
    let stacked: u16 = (0..rows)
        .map(|row| {
            if is_fanned(spec, row) {
                MIN_FAN_H
            } else {
                CARD_H
            }
        })
        .sum();
    stacked + ROW_GAP * u16::from(rows - 1)
}

fn is_fanned(spec: &BoardSpec, row: u8) -> bool {
    spec.row(row).any(|spot| spot.fan == Fan::Down)
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

/// Lay out `spec`'s piles inside `area`.
///
/// `lengths` and `pins` are indexed by [`PileId`]. `pins` names one card per
/// pile that must stay visible even if the pile has to be elided — the cursor,
/// or the head of a held tail. Returns `None` when `area` is too small to draw a
/// usable board.
pub fn compute(
    spec: &BoardSpec,
    lengths: &[usize],
    area: Rect,
    pins: &[Option<usize>],
) -> Option<BoardLayout> {
    let rows = spec.rows();
    if rows == 0 || area.width < board_width(spec) || area.height < min_board_height(spec) {
        return None;
    }

    // Fixed rows take a card's height each; whatever is left over is shared by
    // the fanned rows, which is where the cards actually pile up.
    let fanned_rows = (0..rows).filter(|&row| is_fanned(spec, row)).count() as u16;
    let fixed: u16 = (0..rows)
        .filter(|&row| !is_fanned(spec, row))
        .map(|_| CARD_H)
        .sum();
    let gaps = ROW_GAP * u16::from(rows - 1);
    let fan_height = (area.height - fixed - gaps)
        .checked_div(fanned_rows)
        .unwrap_or(0);

    let longest = spec
        .spots
        .iter()
        .filter(|spot| spot.fan == Fan::Down)
        .map(|spot| lengths[spot.pile])
        .max()
        .unwrap_or(0);
    let density = if roomy_height(longest) <= fan_height {
        Density::Roomy
    } else {
        Density::Tight
    };

    let x0 = area.x + (area.width - board_width(spec)) / 2;
    let mut piles = Vec::with_capacity(spec.spots.len());
    let mut y = area.y;

    for row in 0..rows {
        let fanned = is_fanned(spec, row);
        let height = if fanned { fan_height } else { CARD_H };
        for spot in spec.row(row) {
            let x = x0 + spot.slot * (CARD_W + CARD_GAP);
            let mut pile = match spot.fan {
                Fan::Down => fanned_layout(
                    spot.pile,
                    lengths[spot.pile],
                    x,
                    y,
                    height,
                    density,
                    pins[spot.pile],
                ),
                Fan::None => spread_layout(spot.pile, lengths[spot.pile], 1, x, y),
                Fan::Right(shown) => spread_layout(spot.pile, lengths[spot.pile], shown, x, y),
            };
            // A fanned pile answers for every row beneath it, so a click below
            // a short pile still lands on it.
            pile.claim_bottom = if fanned {
                area.y + area.height
            } else {
                y + CARD_H
            };
            piles.push(pile);
        }
        y += height + ROW_GAP;
    }

    Some(BoardLayout {
        area,
        density,
        piles,
    })
}

/// A pile drawn as a row: its exposed card, optionally with the `show - 1`
/// cards beneath it peeking out to the left of it.
///
/// Only the exposed card gets a slot, so it is the only one a click can name —
/// the rest are there to be read, not taken. Its slot carries the *exposed*
/// index rather than zero, so picking up the waste picks up the card the player
/// can actually see.
fn spread_layout(pile: PileId, len: usize, show: usize, x: u16, top: u16) -> PileLayout {
    let show = show.max(1).min(len);
    let spread: Vec<(usize, u16)> = (0..show)
        .map(|i| (len - show + i, x + i as u16 * SPREAD_STEP))
        .collect();

    PileLayout {
        pile,
        fan: if show > 1 {
            Fan::Right(show)
        } else {
            Fan::None
        },
        x,
        top,
        width: spread.last().map_or(CARD_W, |&(_, last)| last - x + CARD_W),
        slots: match len.checked_sub(1) {
            Some(index) => vec![Slot {
                index,
                top,
                height: CARD_H,
            }],
            None => Vec::new(),
        },
        spread,
        elision: None,
        height: CARD_H,
        claim_bottom: top + CARD_H,
    }
}

fn fanned_layout(
    pile: PileId,
    len: usize,
    x: u16,
    top: u16,
    available: u16,
    density: Density,
    pin: Option<usize>,
) -> PileLayout {
    let base = PileLayout {
        pile,
        fan: Fan::Down,
        x,
        top,
        width: CARD_W,
        slots: Vec::new(),
        spread: Vec::new(),
        elision: None,
        height: CARD_H,
        claim_bottom: top + CARD_H,
    };

    if len == 0 {
        return base;
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
            PileLayout {
                slots,
                height: roomy_height(len),
                ..base
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
            PileLayout {
                slots,
                height: tight_height(len),
                ..base
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
            let hidden = tail_start - head - 1;

            if hidden == 0 {
                // The pin sits right above the tail: draw them contiguously.
                let visible = len - head;
                let slots = (head..len)
                    .enumerate()
                    .map(|(row, index)| Slot {
                        index,
                        top: top + 1 + row as u16,
                        height: if index + 1 == len { 2 } else { 1 },
                    })
                    .collect();
                return PileLayout {
                    slots,
                    height: visible as u16 + 2,
                    ..base
                };
            }

            let mut slots = Vec::with_capacity(tail + 1);
            slots.push(Slot {
                index: head,
                top: top + 1,
                height: 1,
            });
            let marker_row = top + 2;
            slots.extend((tail_start..len).enumerate().map(|(row, index)| Slot {
                index,
                top: top + 3 + row as u16,
                height: if index + 1 == len { 2 } else { 1 },
            }));

            PileLayout {
                slots,
                elision: Some((hidden, marker_row)),
                height: tail as u16 + 4,
                ..base
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Spot;

    fn area(width: u16, height: u16) -> Rect {
        Rect::new(0, 0, width, height)
    }

    /// Seven fanned piles in one row — Scorpion's board.
    fn tableau_spec() -> BoardSpec {
        BoardSpec::new(
            (0..7)
                .map(|i| Spot::new(i, 0, i as u16, Fan::Down))
                .collect(),
        )
    }

    /// A fixed row of four flat piles above seven fanned ones — Klondike's.
    fn two_row_spec() -> BoardSpec {
        let mut spots: Vec<Spot> = (0..4)
            .map(|i| Spot::new(i, 0, i as u16, Fan::None))
            .collect();
        spots.extend((0..7).map(|i| Spot::new(4 + i, 1, i as u16, Fan::Down)));
        BoardSpec::new(spots)
    }

    fn lengths(spec: &BoardSpec, value: usize) -> Vec<usize> {
        vec![value; spec.spots.len()]
    }

    fn no_pins(spec: &BoardSpec) -> Vec<Option<usize>> {
        vec![None; spec.spots.len()]
    }

    /// Every visible card must be reachable at its own rows, and no two cards
    /// may claim the same row — this is what keeps clicks honest.
    fn assert_slots_round_trip(layout: &BoardLayout) {
        for pile in &layout.piles {
            let mut previous_bottom = None;
            for slot in &pile.slots {
                if let Some(bottom) = previous_bottom {
                    assert!(slot.top >= bottom, "slots overlap in pile {}", pile.pile);
                }
                previous_bottom = Some(slot.bottom());

                for y in slot.top..slot.bottom() {
                    let hit = layout.hit(pile.x, y).expect("inside the board");
                    assert_eq!(hit.pile, pile.pile, "row {y} resolved to the wrong pile");
                    assert_eq!(
                        hit.card,
                        Some(slot.index),
                        "row {y} should resolve to card {}",
                        slot.index
                    );
                }
            }
            assert!(
                pile.top + pile.height <= layout.area.y + layout.area.height,
                "pile {} overflows the board area",
                pile.pile
            );
        }
    }

    #[test]
    fn a_roomy_board_gives_every_card_two_rows() {
        let spec = tableau_spec();
        let layout = compute(&spec, &lengths(&spec, 7), area(80, 40), &no_pins(&spec)).unwrap();
        assert_eq!(layout.density, Density::Roomy);

        let pile = &layout.piles[0];
        assert_eq!(pile.slots.len(), 7);
        assert_eq!(pile.slots[0].height, 2);
        assert_eq!(
            pile.slots[6].height, 3,
            "the last card shows its bottom border"
        );
        assert_eq!(pile.height, 15);
        assert!(pile.elision.is_none());
        assert_slots_round_trip(&layout);
    }

    #[test]
    fn a_single_fanned_row_is_laid_out_exactly_as_it_always_was() {
        // Scorpion's geometry is load-bearing for its rendering tests, so pin
        // the numbers rather than trusting the generalisation.
        let spec = tableau_spec();
        assert_eq!(board_width(&spec), 48);
        assert_eq!(min_board_height(&spec), 6);

        let layout = compute(&spec, &lengths(&spec, 7), area(80, 30), &no_pins(&spec)).unwrap();
        assert_eq!(layout.piles[0].x, 16, "the board is centred");
        assert_eq!(layout.piles[0].top, 0);
        for (i, pile) in layout.piles.iter().enumerate() {
            assert_eq!(pile.x, 16 + i as u16 * 7);
        }
    }

    #[test]
    fn tall_boards_fall_back_to_one_row_per_card() {
        let spec = tableau_spec();
        let mut lengths = lengths(&spec, 7);
        lengths[0] = 18;
        let layout = compute(&spec, &lengths, area(80, 24), &no_pins(&spec)).unwrap();
        assert_eq!(layout.density, Density::Tight);

        let pile = &layout.piles[0];
        assert_eq!(pile.slots.len(), 18, "still every card");
        assert!(pile.elision.is_none());
        assert_slots_round_trip(&layout);
    }

    #[test]
    fn piles_too_tall_to_fit_elide_their_middle() {
        let spec = tableau_spec();
        let mut lengths = lengths(&spec, 7);
        lengths[0] = 30;
        let layout = compute(&spec, &lengths, area(80, 20), &no_pins(&spec)).unwrap();
        let pile = &layout.piles[0];

        let (hidden, marker) = pile.elision.expect("pile was elided");
        assert_eq!(marker, layout.area.y + 2);
        assert_eq!(
            pile.slots.len() + hidden,
            30,
            "every card is shown or counted"
        );
        assert_eq!(pile.slots[0].index, 0, "the top card stays visible");
        assert_eq!(
            pile.slots.last().unwrap().index,
            29,
            "the exposed card is always visible"
        );
        assert_slots_round_trip(&layout);
    }

    #[test]
    fn a_pinned_card_survives_elision() {
        let spec = tableau_spec();
        let mut lengths = lengths(&spec, 7);
        lengths[0] = 30;
        let mut pins = no_pins(&spec);
        pins[0] = Some(4);
        let layout = compute(&spec, &lengths, area(80, 20), &pins).unwrap();
        let pile = &layout.piles[0];

        assert_eq!(pile.slots[0].index, 4, "the pinned card takes the top slot");
        assert_eq!(pile.slots.last().unwrap().index, 29);
        assert_slots_round_trip(&layout);
    }

    #[test]
    fn a_pin_next_to_the_tail_needs_no_marker() {
        let spec = tableau_spec();
        let mut lengths = vec![0; spec.spots.len()];
        lengths[0] = 30;

        let without = compute(&spec, &lengths, area(80, 20), &no_pins(&spec)).unwrap();
        let tail_start = without.piles[0].slots[1].index;

        let mut pins = no_pins(&spec);
        pins[0] = Some(tail_start - 1);
        let layout = compute(&spec, &lengths, area(80, 20), &pins).unwrap();
        let pile = &layout.piles[0];

        assert!(pile.elision.is_none(), "nothing left to hide");
        assert_eq!(pile.slots[0].index, tail_start - 1);
        assert_slots_round_trip(&layout);
    }

    #[test]
    fn empty_piles_keep_a_placeholder_and_accept_drops() {
        let spec = tableau_spec();
        let layout = compute(&spec, &lengths(&spec, 0), area(80, 24), &no_pins(&spec)).unwrap();
        let pile = &layout.piles[3];

        assert!(pile.slots.is_empty());
        assert_eq!(pile.height, CARD_H);

        let hit = layout.hit(pile.x, layout.area.y + 1).unwrap();
        assert_eq!(hit.pile, 3);
        assert_eq!(hit.card, None, "no card, but still a drop target");
    }

    #[test]
    fn clicks_map_to_the_pile_under_them() {
        let spec = tableau_spec();
        let layout = compute(&spec, &lengths(&spec, 7), area(80, 30), &no_pins(&spec)).unwrap();

        for (i, pile) in layout.piles.iter().enumerate() {
            assert_eq!(layout.hit(pile.x, layout.area.y).unwrap().pile, i);
            assert_eq!(
                layout.hit(pile.x + CARD_W - 1, layout.area.y).unwrap().pile,
                i
            );
            // The gap between piles belongs to neither.
            if i + 1 < layout.piles.len() {
                assert!(layout.hit(pile.x + CARD_W, layout.area.y).is_none());
            }
        }
        assert!(layout.hit(0, layout.area.y + layout.area.height).is_none());
    }

    #[test]
    fn a_cramped_area_has_no_layout() {
        let spec = tableau_spec();
        let (l, p) = (lengths(&spec, 7), no_pins(&spec));
        assert!(compute(&spec, &l, area(board_width(&spec) - 1, 24), &p).is_none());
        assert!(compute(&spec, &l, area(80, min_board_height(&spec) - 1), &p).is_none());
        assert!(compute(&spec, &l, area(board_width(&spec), MIN_FAN_H), &p).is_some());
    }

    #[test]
    fn a_fixed_row_sits_above_the_fanned_one_and_takes_a_card_of_height() {
        let spec = two_row_spec();
        assert_eq!(min_board_height(&spec), CARD_H + ROW_GAP + MIN_FAN_H);

        let layout = compute(&spec, &lengths(&spec, 5), area(80, 30), &no_pins(&spec)).unwrap();
        let top_row = &layout.piles[..4];
        let tableau = &layout.piles[4..];

        for pile in top_row {
            assert_eq!(pile.top, 0);
            assert_eq!(pile.height, CARD_H);
            assert_eq!(pile.slots.len(), 1, "only the exposed card is drawn");
            assert_eq!(
                pile.slots[0].index, 4,
                "and it is addressed as the exposed card, not the bottom one"
            );
        }
        for pile in tableau {
            assert_eq!(pile.top, CARD_H + ROW_GAP);
            assert_eq!(pile.slots.len(), 5);
        }
        assert_slots_round_trip(&layout);
    }

    #[test]
    fn a_click_below_the_fixed_row_lands_on_the_pile_beneath_it() {
        // The foundation at slot 1 and the tableau pile at slot 1 share an x
        // range, so only the row can tell them apart.
        let spec = two_row_spec();
        let layout = compute(&spec, &lengths(&spec, 5), area(80, 30), &no_pins(&spec)).unwrap();
        let x = layout.pile(1).unwrap().x;
        assert_eq!(layout.pile(5).unwrap().x, x, "they line up");

        assert_eq!(layout.hit(x, 1).unwrap().pile, 1, "the fixed row");
        assert_eq!(
            layout.hit(x, CARD_H + ROW_GAP + 1).unwrap().pile,
            5,
            "the fanned row below it"
        );
        assert!(
            layout.hit(x, CARD_H).is_none(),
            "the gap belongs to neither"
        );
    }

    #[test]
    fn a_fanned_pile_claims_the_empty_rows_below_it_but_a_flat_one_does_not() {
        let spec = two_row_spec();
        let mut lengths = lengths(&spec, 1);
        lengths[4] = 1;
        let layout = compute(&spec, &lengths, area(80, 30), &no_pins(&spec)).unwrap();

        let tableau = layout.pile(4).unwrap();
        let bottom = layout.area.y + layout.area.height - 1;
        assert_eq!(
            layout.hit(tableau.x, bottom).unwrap().pile,
            4,
            "clicking well below a short pile still targets it"
        );

        let foundation = layout.pile(3).unwrap();
        assert!(
            layout.hit(foundation.x, CARD_H).is_none(),
            "a flat pile answers only for its own three rows"
        );
    }

    #[test]
    fn a_rightward_fan_draws_several_cards_but_offers_only_the_exposed_one() {
        let spec = BoardSpec::new(vec![Spot::new(0, 0, 0, Fan::Right(3))]);
        let layout = compute(&spec, &[8], area(80, 12), &[None]).unwrap();
        let pile = &layout.piles[0];

        assert_eq!(
            pile.spread,
            vec![
                (5, pile.x),
                (6, pile.x + SPREAD_STEP),
                (7, pile.x + 2 * SPREAD_STEP)
            ],
            "the last three cards, left to right"
        );
        assert_eq!(pile.slots.len(), 1, "only one card is addressable");
        assert_eq!(pile.slots[0].index, 7, "and it is the exposed one");
        assert_eq!(pile.width, 2 * SPREAD_STEP + CARD_W);
        assert_eq!(pile.height, CARD_H, "a fanned row is still one card tall");
    }

    #[test]
    fn every_cell_of_a_rightward_fan_resolves_to_the_card_in_play() {
        let spec = BoardSpec::new(vec![Spot::new(0, 0, 0, Fan::Right(3))]);
        let layout = compute(&spec, &[8], area(80, 12), &[None]).unwrap();
        let pile = &layout.piles[0];

        for x in pile.x..pile.x + pile.width {
            let hit = layout.hit(x, pile.top + 1).expect("inside the fan");
            assert_eq!(hit.pile, 0);
            assert_eq!(
                hit.card,
                Some(7),
                "clicking a covered card at {x} must offer the exposed one"
            );
        }
        assert!(
            layout.hit(pile.x + pile.width, pile.top + 1).is_none(),
            "and the fan claims no more than its own width"
        );
    }

    #[test]
    fn a_short_fan_is_no_wider_than_a_single_card() {
        let spec = BoardSpec::new(vec![Spot::new(0, 0, 0, Fan::Right(3))]);
        // Two cards held, three slots asked for: it shows what it has.
        let layout = compute(&spec, &[1], area(80, 12), &[None]).unwrap();
        assert_eq!(layout.piles[0].spread, vec![(0, layout.piles[0].x)]);
        assert_eq!(layout.piles[0].width, CARD_W);
        assert_eq!(layout.piles[0].fan, Fan::None, "nothing left to fan");
    }

    #[test]
    fn a_fan_reaching_into_the_next_slot_costs_no_width_and_overlaps_nothing() {
        // Klondike's shape: a three-card fan in slot 1 spreading across the gap
        // at slot 2, with foundations from slot 3. The board stays 7 slots wide.
        let mut spots = vec![
            Spot::new(0, 0, 0, Fan::None),
            Spot::new(1, 0, 1, Fan::Right(3)),
        ];
        spots.extend((0..4).map(|i| Spot::new(2 + i, 0, 3 + i as u16, Fan::None)));
        spots.extend((0..7).map(|i| Spot::new(6 + i, 1, i as u16, Fan::Down)));
        let spec = BoardSpec::new(spots);

        assert_eq!(board_width(&spec), 7 * CARD_W + 6 * CARD_GAP);

        let lengths = vec![5; 13];
        let layout = compute(&spec, &lengths, area(80, 30), &vec![None; 13]).unwrap();
        let waste = layout.pile(1).unwrap();
        let foundation = layout.pile(2).unwrap();
        assert!(
            waste.x + waste.width <= foundation.x,
            "the fan overlaps the foundations: {} vs {}",
            waste.x + waste.width,
            foundation.x
        );
        assert_slots_round_trip(&layout);
    }

    #[test]
    fn every_pile_in_the_spec_gets_a_layout() {
        let spec = two_row_spec();
        let layout = compute(&spec, &lengths(&spec, 3), area(80, 30), &no_pins(&spec)).unwrap();
        assert_eq!(layout.piles.len(), spec.spots.len());
        for spot in &spec.spots {
            assert!(layout.pile(spot.pile).is_some(), "pile {}", spot.pile);
        }
    }
}
