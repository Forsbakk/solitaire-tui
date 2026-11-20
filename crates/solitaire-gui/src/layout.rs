//! Where every card sits on the canvas, in CSS pixels.
//!
//! This is the graphical frontend's single source of truth for board shape,
//! as `ui/layout.rs` is the terminal's: `render.rs` draws from a [`Layout`]
//! and the pointer resolves against the same one through [`Layout::hit`], so
//! a press always lands on the card that was drawn under it.
//!
//! It is built from the game's `BoardSpec` alone — a game never says anything
//! about pixels. Card size is whichever is smaller of what the widest row and
//! what the rows' expected depth allow; fanned piles then squeeze their overlap
//! to fit the height they were given, face-down cards first.
//!
//! Every card of every placed pile has a rect, including the ones a stacked
//! pile hides — the animator needs to know where a card *was* even when it was
//! not visible. What is actually drawn starts at [`PileGeom::visible_from`].

use solitaire_tui_core::board::{Board, BoardSpec, Fan, PileId};
use solitaire_tui_core::ui::layout::Hit;

/// Card height over width — a poker card is 88 × 63 mm.
pub const ASPECT: f64 = 1.4;
/// Space between columns, as a fraction of the card width.
const GAP: f64 = 0.12;
/// Space around the table, as a fraction of the card width.
const MARGIN: f64 = 0.16;
/// Space between rows, as a fraction of the card width.
const ROW_GAP: f64 = 0.2;
/// How far a face-down card peeks from under the next, as a fraction of the
/// card height — just enough to count them.
const DOWN_STEP: f64 = 0.09;
/// The least a face-down card may be squeezed to.
const MIN_DOWN_STEP: f64 = 0.035;
/// How far a face-up card peeks — enough to read its corner index.
const UP_STEP: f64 = 0.235;
/// Below this width a card is drawn with one large index instead of a full
/// face (`cards.rs`), which needs a deeper strip to show.
pub const SMALL_CARD: f64 = 58.0;
const SMALL_UP_STEP: f64 = 0.3;

/// The face-up step for cards `card_w` wide, as a fraction of their height.
fn up_step(card_w: f64) -> f64 {
    if card_w < SMALL_CARD {
        SMALL_UP_STEP
    } else {
        UP_STEP
    }
}
/// The most cards a fanned row above the working row ever holds: Mod3's
/// foundations are a base and three builds. Such a row is given that much
/// room up front, so it never grows and the rows below it never move.
const STACK_ROW_CARDS: f64 = 4.0;
/// Cards stop growing here; a huge monitor gets a roomier table, not posters.
const MAX_W: f64 = 140.0;
/// Below this the table is not worth drawing at all.
const MIN_W: f64 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn offset(&self, dx: f64, dy: f64) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }

    /// How much of `self` and `other` coincide, as an area.
    pub fn overlap(&self, other: &Self) -> f64 {
        let w = self.right().min(other.right()) - self.x.max(other.x);
        let h = self.bottom().min(other.bottom()) - self.y.max(other.y);
        if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
    }

    pub fn lerp(&self, to: &Self, t: f64) -> Self {
        let mix = |a: f64, b: f64| a + (b - a) * t;
        Self::new(
            mix(self.x, to.x),
            mix(self.y, to.y),
            mix(self.w, to.w),
            mix(self.h, to.h),
        )
    }

    pub fn distance(&self, other: &Self) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}

/// One placed pile.
#[derive(Clone, Debug, PartialEq)]
pub struct PileGeom {
    pub pile: PileId,
    pub fan: Fan,
    /// Where an empty pile's outline goes, and where its bottom card sits.
    pub base: Rect,
    /// The area a press or a drop answers to. A fanned pile in the last row
    /// claims down to the bottom of the table, so a press below a short column
    /// still lands on it.
    pub claim: Rect,
    /// A rect for every card in the pile, bottom first.
    pub cards: Vec<Rect>,
    /// The first card worth drawing; everything before it is hidden under it.
    pub visible_from: usize,
}

impl PileGeom {
    /// The card a press at `(x, y)` names: the topmost one drawn under it.
    /// A stacked pile only ever offers its exposed card — the rest of a
    /// fanned waste is there to be read, not taken.
    fn card_at(&self, x: f64, y: f64) -> Option<usize> {
        let last = self.cards.len().checked_sub(1)?;
        match self.fan {
            Fan::Down => (0..=last).rev().find(|&i| self.cards[i].contains(x, y)),
            Fan::None | Fan::Right(_) => (y < self.base.bottom()).then_some(last),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub width: f64,
    pub height: f64,
    pub card_w: f64,
    pub card_h: f64,
    pub piles: Vec<PileGeom>,
}

impl Layout {
    /// The pile, and card if any, under `(x, y)`.
    pub fn hit(&self, x: f64, y: f64) -> Option<Hit> {
        let pile = self.piles.iter().find(|p| p.claim.contains(x, y))?;
        Some(Hit {
            pile: pile.pile,
            card: pile.card_at(x, y),
        })
    }

    /// Where a card dragged to `held` would be dropped: the pile it covers
    /// most of, so a drop does not need the pointer itself to be on target.
    pub fn drop_target(&self, held: &Rect) -> Option<PileId> {
        self.piles
            .iter()
            .map(|p| {
                // The exposed card is what a drop lands on, so measure against
                // it as well as the claim — whichever overlaps more.
                let top = p.cards.last().copied().unwrap_or(p.base);
                (p.pile, held.overlap(&p.claim).max(held.overlap(&top)))
            })
            .filter(|&(_, area)| area > 0.0)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(pile, _)| pile)
    }

    pub fn pile(&self, id: PileId) -> Option<&PileGeom> {
        self.piles.iter().find(|p| p.pile == id)
    }

    pub fn card(&self, pile: PileId, index: usize) -> Option<Rect> {
        self.pile(pile)?.cards.get(index).copied()
    }
}

/// How many card widths the widest row takes. A pile fanning rightwards
/// spreads into the slot beside it, as Klondike's waste does into the gap
/// before the foundations.
fn units(spec: &BoardSpec) -> f64 {
    spec.spots
        .iter()
        .map(|spot| {
            let extra = matches!(spot.fan, Fan::Right(n) if n > 1);
            f64::from(spot.slot) + 1.0 + f64::from(u8::from(extra))
        })
        .fold(0.0, f64::max)
}

fn is_fanned(spec: &BoardSpec, row: u8) -> bool {
    spec.row(row).any(|spot| spot.fan == Fan::Down)
}

/// How tall the table wants to be, in card widths, so the card size can be
/// capped by height before any card is dealt. A working row is planned for a
/// handful of face-up cards on a few face-down ones, beyond which it squeezes;
/// a fanned row above it (Mod3's foundations) for its full
/// [`STACK_ROW_CARDS`], which it is always given.
fn planned_height(spec: &BoardSpec) -> f64 {
    let rows = spec.rows();
    let fanned: Vec<u8> = (0..rows).filter(|&r| is_fanned(spec, r)).collect();
    let deep = if fanned.len() > 1 { 1.8 } else { 2.6 };
    let rows_h: f64 = (0..rows)
        .map(|row| match fanned.iter().position(|&r| r == row) {
            Some(i) if i + 1 == fanned.len() => ASPECT * deep,
            Some(_) => ASPECT * (1.0 + (STACK_ROW_CARDS - 1.0) * UP_STEP),
            None => ASPECT,
        })
        .sum();
    rows_h + ROW_GAP * f64::from(rows.saturating_sub(1)) + 2.0 * MARGIN
}

/// Lay `spec` out in a `width` × `height` table. `None` when that is too small
/// to be worth drawing.
pub fn compute(spec: &BoardSpec, board: &Board, width: f64, height: f64) -> Option<Layout> {
    let rows = spec.rows();
    let units = units(spec);
    if rows == 0 || units == 0.0 {
        return None;
    }
    let by_width = width / (units + (units - 1.0) * GAP + 2.0 * MARGIN);
    let by_height = height / planned_height(spec);
    let card_w = by_width.min(by_height).min(MAX_W).floor();
    if card_w < MIN_W {
        return None;
    }
    let card_h = (card_w * ASPECT).round();
    let gap = card_w * GAP;
    let margin = card_w * MARGIN;
    let row_gap = card_w * ROW_GAP;

    // Fixed rows take a card's height. A fanned row above the working row
    // (Mod3's foundations) is reserved at its full depth whatever it holds,
    // and whatever height is left goes to the last fanned row — the working
    // row — so no row ever moves as the piles above it fill. The heights
    // depend on the spec alone, never on the board.
    let fanned: Vec<u8> = (0..rows).filter(|&r| is_fanned(spec, r)).collect();
    let fixed = card_h * f64::from(rows - fanned.len() as u8);
    let space = (height - 2.0 * margin - row_gap * f64::from(rows - 1) - fixed)
        .max(card_h * fanned.len() as f64);
    let upper = fanned.len().saturating_sub(1);
    let full = card_h * (1.0 + (STACK_ROW_CARDS - 1.0) * up_step(card_w));
    // Too short for every upper row at full depth plus a card for the working
    // row: the upper rows shrink alike, and `fan_down` squeezes into them.
    let stack_h = if upper == 0 {
        0.0
    } else {
        full.min((space - card_h) / upper as f64).max(card_h)
    };
    let mut heights = vec![stack_h; upper];
    if !fanned.is_empty() {
        heights.push((space - stack_h * upper as f64).max(card_h));
    }

    let board_w = units * card_w + (units - 1.0) * gap;
    let x0 = ((width - board_w) / 2.0).max(margin).floor();
    let mut piles = Vec::with_capacity(spec.spots.len());
    let mut y = margin;

    for row in 0..rows {
        let fan_index = fanned.iter().position(|&r| r == row);
        let row_h = fan_index.map_or(card_h, |i| heights[i]);
        let is_last_fanned = fan_index.is_some_and(|i| i + 1 == fanned.len());
        for spot in spec.row(row) {
            let x = (x0 + f64::from(spot.slot) * (card_w + gap)).round();
            let base = Rect::new(x, y.round(), card_w, card_h);
            let len = board.pile(spot.pile).len();
            let (cards, visible_from, reach) = match spot.fan {
                Fan::Down => {
                    let cards = fan_down(board, spot.pile, base, row_h);
                    (cards, 0, card_w)
                }
                Fan::None => (vec![base; len], len.saturating_sub(1), card_w),
                Fan::Right(shown) => {
                    let shown = shown.max(1);
                    let step = if shown > 1 {
                        ((card_w + gap) / (shown - 1) as f64).min(card_w * 0.45)
                    } else {
                        0.0
                    };
                    let first = len.saturating_sub(shown);
                    let cards = (0..len)
                        .map(|i| base.offset(i.saturating_sub(first) as f64 * step, 0.0))
                        .collect();
                    (cards, first, card_w + step * (shown - 1) as f64)
                }
            };
            let claim_bottom = if is_last_fanned { height } else { y + row_h };
            piles.push(PileGeom {
                pile: spot.pile,
                fan: spot.fan,
                base,
                claim: Rect::new(x, base.y, reach, claim_bottom - base.y),
                cards,
                visible_from,
            });
        }
        y += row_h + row_gap;
    }

    Some(Layout {
        width,
        height,
        card_w,
        card_h,
        piles,
    })
}

/// A fanned pile's cards, squeezed to fit `room` from the top of `base`:
/// face-down cards give way first, then face-up ones.
fn fan_down(board: &Board, pile: PileId, base: Rect, room: f64) -> Vec<Rect> {
    let p = board.pile(pile);
    let len = p.len();
    if len == 0 {
        return Vec::new();
    }
    let h = base.h;
    let down = p.face_down().min(len - 1) as f64;
    let up = (len - 1) as f64 - down;
    let spare = (room - h).max(0.0);

    let mut down_step = h * DOWN_STEP;
    let mut up_step = h * up_step(base.w);
    if down * down_step + up * up_step > spare {
        down_step = if up > 0.0 {
            ((spare - up * up_step) / down.max(1.0)).max(h * MIN_DOWN_STEP)
        } else {
            spare / down.max(1.0)
        }
        .min(h * DOWN_STEP);
        if up > 0.0 && down * down_step + up * up_step > spare {
            up_step = ((spare - down * down_step) / up).max(0.0);
        }
    }

    let mut y = base.y;
    (0..len)
        .map(|i| {
            let rect = Rect::new(base.x, y.round(), base.w, h);
            y += if p.is_face_up(i) { up_step } else { down_step };
            rect
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use solitaire_tui_core::rules;

    const SIZES: [(f64, f64); 4] = [
        (1280.0, 760.0),
        (375.0, 600.0),
        (812.0, 300.0),
        (2560.0, 1300.0),
    ];

    fn layouts() -> impl Iterator<Item = (String, Board, Layout)> {
        rules::games().into_iter().flat_map(|game| {
            SIZES.into_iter().map(move |(w, h)| {
                let board = game.deal(42);
                let spec = game.spec(&board);
                let layout = compute(&spec, &board, w, h)
                    .unwrap_or_else(|| panic!("{} fits {w}×{h}", game.name()));
                (format!("{} at {w}×{h}", game.name()), board, layout)
            })
        })
    }

    /// The point a player would press to take card `i`: the middle of the strip
    /// of it left showing, or of the whole card when it is on top.
    fn visible_point(pile: &PileGeom, i: usize) -> (f64, f64) {
        let rect = pile.cards[i];
        let shown_to = pile
            .cards
            .get(i + 1)
            .filter(|_| pile.fan == Fan::Down)
            .map_or(rect.bottom(), |next| next.y);
        (rect.x + rect.w / 2.0, (rect.y + shown_to) / 2.0)
    }

    #[test]
    fn every_card_drawn_in_a_fan_resolves_back_to_itself() {
        for (name, _, layout) in layouts() {
            for pile in &layout.piles {
                for i in pile.visible_from..pile.cards.len() {
                    let next_y = pile.cards.get(i + 1).map(|r| r.y);
                    if pile.fan == Fan::Down && next_y == Some(pile.cards[i].y) {
                        continue; // squeezed to nothing: not pressable, not drawn apart
                    }
                    let (x, y) = visible_point(pile, i);
                    let expected = match pile.fan {
                        Fan::Down => i,
                        _ => pile.cards.len() - 1,
                    };
                    assert_eq!(
                        layout.hit(x, y),
                        Some(Hit {
                            pile: pile.pile,
                            card: Some(expected)
                        }),
                        "{name}: pile {} card {i}",
                        pile.pile
                    );
                }
            }
        }
    }

    #[test]
    fn nothing_overlaps_a_neighbouring_pile_or_leaves_the_table() {
        for (name, _, layout) in layouts() {
            for (a, pile) in layout.piles.iter().enumerate() {
                for card in &pile.cards {
                    assert!(card.x >= 0.0 && card.right() <= layout.width, "{name}");
                    assert!(
                        card.y >= 0.0 && card.bottom() <= layout.height + 0.5,
                        "{name}: {card:?}"
                    );
                }
                for other in &layout.piles[a + 1..] {
                    assert_eq!(
                        pile.claim.overlap(&other.claim),
                        0.0,
                        "{name}: {} and {}",
                        pile.pile,
                        other.pile
                    );
                }
            }
        }
    }

    #[test]
    fn a_press_below_a_short_column_still_names_it() {
        let game = rules::find("klondike").unwrap();
        let board = game.deal(42);
        let layout = compute(&game.spec(&board), &board, 1280.0, 760.0).unwrap();
        let first = layout.pile(0).unwrap();
        let x = first.base.x + 5.0;
        assert_eq!(
            layout.hit(x, 740.0),
            Some(Hit {
                pile: 0,
                card: None
            })
        );
    }

    #[test]
    fn a_long_column_is_squeezed_to_fit() {
        let game = rules::find("spider").unwrap();
        let mut board = game.deal(7);
        // Deal the whole stock out so the columns get as long as they can.
        let stock = board
            .first_of_kind(solitaire_tui_core::board::PileKind::Stock)
            .unwrap();
        while let Some(action) = game.activate(&board, stock) {
            board.apply(&action);
        }
        let layout = compute(&game.spec(&board), &board, 375.0, 560.0).unwrap();
        for pile in &layout.piles {
            if let Some(last) = pile.cards.last() {
                assert!(
                    last.bottom() <= 560.5,
                    "pile {} ends at {}",
                    pile.pile,
                    last.bottom()
                );
            }
        }
    }

    #[test]
    fn mod3_foundation_rows_do_not_move_as_they_fill() {
        use solitaire_tui_core::board::Action;
        let game = rules::find("mod3").unwrap();
        let opening = game.deal(42);
        let mut full = opening.clone();
        // Grow the first foundation pile of each row to its full four cards.
        // The layout takes no notice of legality, only of depth.
        for first in [8, 16, 24] {
            for from in first + 1..first + 4 {
                full.apply(&Action::moving(&full, from, 0, first));
            }
        }
        for (w, h) in SIZES {
            let before = compute(&game.spec(&opening), &opening, w, h).unwrap();
            let after = compute(&game.spec(&full), &full, w, h).unwrap();
            for (a, b) in before.piles.iter().zip(&after.piles) {
                assert_eq!(a.base, b.base, "pile {} at {w}×{h}", a.pile);
            }
        }
    }

    #[test]
    fn a_card_dragged_over_a_column_drops_on_it() {
        let game = rules::find("klondike").unwrap();
        let board = game.deal(42);
        let layout = compute(&game.spec(&board), &board, 1280.0, 760.0).unwrap();
        let target = layout.pile(3).unwrap().cards.last().unwrap();
        let held = target.offset(target.w * 0.3, target.h * 0.2);
        assert_eq!(layout.drop_target(&held), Some(3));
    }
}
