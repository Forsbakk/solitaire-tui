//! Mod 3 — two decks, a grid of piles, and foundations built by threes.
//!
//! The rules follow KDE KPatience's `mod3.cpp`, which is the reference for any
//! question about how this game should play.
//!
//! * Two 52-card decks (104 cards). The eight Aces are held on their own pile
//!   and are never built upon — they simply leave the grid.
//! * The board has a 4×8 grid of 32 piles, plus a talon and the aces pile.
//!   One card is dealt face-up to each of the 32 positions; the remaining 72
//!   cards stay in the talon.
//! * Row 1 builds 2→5→8→J in the same suit.
//! * Row 2 builds 3→6→9→Q in the same suit.
//! * Row 3 builds 4→7→10→K in the same suit.
//! * An empty spot in rows 1–3 takes only that row's base card. A pile is
//!   built on only if its bottom card is that base: a card dealt into the
//!   wrong spot is *misplaced*, can never be built on, and is there to be
//!   moved off. The top card of any of these piles may be moved.
//! * Row 4 (the working row) holds cards as a staging area. A player move may
//!   only land on an empty working-row pile. Pressing `d` deals one card from
//!   the talon onto each of the eight working-row piles (even non-empty ones).
//!   A working-row pile that a move empties is refilled from the talon at
//!   once, so free cells only appear once the talon is spent.
//! * You win when all 24 sequence piles are complete and all 8 Aces are placed.

use crate::board::{
    Action, Board, BoardSpec, Fan, Move, MoveError, Pile, PileId, PileKind, Spot, Step,
};
use crate::card::{ACE, Card, shuffled_decks};
use crate::rules::{Rules, StatusItem, StatusTone};

const ROWS: usize = 8;

/// Working row (staging / buffer area). Piles 0–7.
const TABLEAU: PileId = 0;
/// Sequence foundations: 2→5→8→J. Piles 8–15.
const FOUND_2: PileId = TABLEAU + ROWS;
/// Sequence foundations: 3→6→9→Q. Piles 16–23.
const FOUND_3: PileId = FOUND_2 + ROWS;
/// Sequence foundations: 4→7→10→K. Piles 24–31.
const FOUND_4: PileId = FOUND_3 + ROWS;
/// Talon. Pile 32.
const STOCK: PileId = FOUND_4 + ROWS;
/// Dedicated pile for Aces. Pile 33.
const ACES: PileId = STOCK + 1;

/// Cards in one complete mod-3 sequence.
const SEQ_LEN: usize = 4;

#[derive(Default)]
pub struct Mod3;

impl Rules for Mod3 {
    fn name(&self) -> &'static str {
        "Mod3"
    }

    fn blurb(&self) -> &'static str {
        "Two decks, 32 piles. Build foundations upward by three in the same \
         suit: 2–5–8–J in the first row, 3–6–9–Q in the second, 4–7–10–K in \
         the third. Aces leave for their own pile.\n\n\
         A card dealt into the wrong spot can never be built on — move it to \
         where it belongs, and the spot it leaves takes only its row's base \
         card.\n\n\
         The fourth row is a working area. Press d to deal one card onto each \
         of its eight piles. A working-row pile that empties is refilled from \
         the talon; once the talon is spent, it stays open as a free cell."
    }

    fn help(&self) -> &'static str {
        "  d   deal one card onto each working-row pile"
    }

    fn deal(&self, seed: u64) -> Board {
        let mut deck = shuffled_decks(seed, 2).into_iter();

        // Working row: 8 piles, 1 card each.
        let mut piles: Vec<Pile> = (0..ROWS)
            .map(|_| {
                let card = deck.next().unwrap();
                Pile::tableau(0, &[card])
            })
            .collect();

        // Three foundation rows (FOUND_2, FOUND_3, FOUND_4): 8 piles each, 1 card each.
        for _ in 0..3 * ROWS {
            let card = deck.next().unwrap();
            piles.push(Pile::holding(PileKind::Foundation, 0, &[card]));
        }

        // Talon: remaining 72 cards, face down.
        let stock: Vec<_> = deck.collect();
        debug_assert_eq!(stock.len(), 72);
        piles.push(Pile::holding(PileKind::Stock, stock.len(), &stock));

        // Aces pile: starts empty.
        piles.push(Pile::new(PileKind::Foundation));

        Board::new(seed, piles)
    }

    fn spec(&self, _board: &Board) -> BoardSpec {
        let mut spots = Vec::new();

        // As in KPatience, the aces and the talon take a ninth column rather
        // than a row of their own: aces beside the first foundation row, the
        // talon beside the working row it deals onto. A header row would cost
        // four lines of terminal height for two piles.
        //
        // Rows 0–2: the foundations fan down so every card in a stack can be
        // read — FOUND_2 (2→5→8→J), FOUND_3 (3→6→9→Q), FOUND_4 (4→7→10→K).
        for (row, first) in [FOUND_2, FOUND_3, FOUND_4].into_iter().enumerate() {
            for i in 0..ROWS {
                spots.push(Spot::new(first + i, row as u8, i as u16, Fan::Down));
            }
        }
        spots.push(Spot::new(ACES, 0, ROWS as u16, Fan::None));
        // Row 3: working row. Fan::Down so talon deals can stack.
        for i in 0..ROWS {
            spots.push(Spot::new(TABLEAU + i, 3, i as u16, Fan::Down));
        }
        spots.push(Spot::new(STOCK, 3, ROWS as u16, Fan::None));

        BoardSpec::new(spots)
    }

    fn check(&self, board: &Board, mv: Move) -> Result<(), MoveError> {
        let Move { from, index, to } = mv;

        if from == to {
            return Err(MoveError::SamePile);
        }
        // Stock and aces pile are never player-move sources.
        if from == STOCK || from == ACES {
            return Err(MoveError::WrongTarget);
        }

        let source = board.pile(from);
        if source.is_empty() || index >= source.len() {
            return Err(MoveError::NoSuchCard);
        }
        if !source.is_face_up(index) {
            return Err(MoveError::FaceDown);
        }
        // Only the exposed (top) card may move.
        if index != source.len() - 1 {
            return Err(MoveError::NotExposed);
        }

        let card = source.cards()[index];
        let target = board.pile(to);

        if let Some(base) = base_of(to) {
            return foundation_check(target, card, base);
        }
        match to {
            STOCK => Err(MoveError::WrongTarget),
            ACES => {
                if card.rank == ACE {
                    Ok(())
                } else {
                    Err(MoveError::Mismatch)
                }
            }
            t if (TABLEAU..TABLEAU + ROWS).contains(&t) => {
                if target.is_empty() {
                    Ok(())
                } else {
                    Err(MoveError::Occupied)
                }
            }
            _ => Err(MoveError::WrongTarget),
        }
    }

    /// Every move is followed by refilling the pile it left, if that was a
    /// working-row pile it emptied.
    fn action_for(&self, board: &Board, mv: Move) -> Action {
        with_refill(board, Action::moving(board, mv.from, mv.index, mv.to))
    }

    /// Deal one card from the talon onto each of the eight working-row piles.
    fn activate(&self, board: &Board, pile: PileId) -> Option<Action> {
        if pile != STOCK {
            return None;
        }
        let stock_len = board.pile(STOCK).len();
        if stock_len == 0 {
            return None;
        }
        let count = stock_len.min(ROWS);
        let steps = (0..count)
            .map(|i| Step {
                from: STOCK,
                to: TABLEAU + i,
                count: 1,
                reverse: false,
                was_face_up: false,
                face_up: true,
                flip_source: false,
                recycle: false,
            })
            .collect();
        Some(Action::new(steps))
    }

    fn send_home(&self, board: &Board, from: PileId) -> Option<Action> {
        if from == STOCK || from == ACES {
            return None;
        }
        // A card on a correctly based pile is already home.
        if base_of(from).is_some() && !is_misplaced(board.pile(from), from) {
            return None;
        }
        let card = board.pile(from).exposed()?;
        let last = board.pile(from).len() - 1;

        // Asking `check` keeps `f` from ever disagreeing with a drag.
        let mut homes = if card.rank == ACE {
            ACES..ACES + 1
        } else {
            let first = match card.rank % 3 {
                2 => FOUND_2,
                0 => FOUND_3,
                _ => FOUND_4,
            };
            first..first + ROWS
        };
        let dest = homes.find(|&to| self.check(board, Move::new(from, last, to)).is_ok())?;

        Some(self.action_for(board, Move::new(from, last, dest)))
    }

    fn is_won(&self, board: &Board) -> bool {
        board.pile(ACES).len() == 8
            && (FOUND_2..FOUND_2 + ROWS).all(|p| is_complete_mod3(board.pile(p), 2))
            && (FOUND_3..FOUND_3 + ROWS).all(|p| is_complete_mod3(board.pile(p), 3))
            && (FOUND_4..FOUND_4 + ROWS).all(|p| is_complete_mod3(board.pile(p), 4))
    }

    fn is_useful(&self, board: &Board, mv: Move) -> bool {
        let source = board.pile(mv.from);
        if base_of(mv.from).is_some() {
            // A misplaced card is there to be moved off — home, to the aces,
            // or out of the way into the working row. A card on a correctly
            // based pile may legally move too, but only ever backwards.
            return is_misplaced(source, mv.from);
        }
        // Sliding a lone working-row card into another empty pile changes
        // nothing; with cards beneath it, it exposes the next one.
        !(base_of(mv.to).is_none() && mv.to != ACES && source.len() == 1)
    }

    fn rank(&self, board: &Board, mv: Move) -> i32 {
        // Moving a misplaced card frees a spot for its row's base card, on
        // top of whatever the move itself achieves.
        let frees = if is_misplaced(board.pile(mv.from), mv.from) {
            5
        } else {
            0
        };
        if mv.to == ACES {
            return 80 + frees;
        }
        if base_of(mv.to).is_some() {
            // Continuing an existing sequence beats starting a new one slightly,
            // to prefer making progress over claiming new slots.
            return frees
                + if board.pile(mv.to).is_empty() {
                    90
                } else {
                    100
                };
        }
        // Parking to an empty working-row pile.
        10 + frees
    }

    fn has_progress_move(&self, board: &Board) -> bool {
        !board.pile(STOCK).is_empty() || !self.suggestions(board).is_empty()
    }

    fn status(&self, board: &Board) -> Vec<StatusItem> {
        let deals_left = board.pile(STOCK).len().div_ceil(ROWS);
        let aces = board.pile(ACES).len();
        vec![
            match deals_left {
                0 => StatusItem::new("deals", "spent", StatusTone::Muted),
                n => StatusItem::new("deals", n.to_string(), StatusTone::Good),
            },
            StatusItem::new(
                "aces",
                format!("{aces}/8"),
                if aces == 8 {
                    StatusTone::Muted
                } else {
                    StatusTone::Good
                },
            ),
        ]
    }
}

/// The rank a foundation row's piles are founded on, or `None` for a pile
/// outside the three foundation rows.
fn base_of(pile: PileId) -> Option<u8> {
    match pile {
        p if (FOUND_2..FOUND_2 + ROWS).contains(&p) => Some(2),
        p if (FOUND_3..FOUND_3 + ROWS).contains(&p) => Some(3),
        p if (FOUND_4..FOUND_4 + ROWS).contains(&p) => Some(4),
        _ => None,
    }
}

/// Whether foundation pile `id` holds a card dealt into the wrong spot: one
/// whose bottom card is not its row's base, so it can never be built on.
fn is_misplaced(pile: &Pile, id: PileId) -> bool {
    match (base_of(id), pile.cards().first()) {
        (Some(base), Some(bottom)) => bottom.rank != base,
        _ => false,
    }
}

/// Whether `card` may land on `target`, which builds from `base` in its row.
fn foundation_check(target: &Pile, card: Card, base: u8) -> Result<(), MoveError> {
    // Aces never belong on a sequence foundation.
    if card.rank == ACE {
        return Err(MoveError::Mismatch);
    }
    match target.exposed() {
        None => {
            if card.rank == base {
                Ok(())
            } else {
                Err(MoveError::Mismatch)
            }
        }
        // KPatience builds only on a pile founded on its row's base; a card
        // dealt into the wrong spot takes nothing.
        Some(top) => {
            let founded = target.cards()[0].rank == base;
            if founded && top.suit == card.suit && card.rank == top.rank + 3 {
                Ok(())
            } else {
                Err(MoveError::Mismatch)
            }
        }
    }
}

/// Whether `pile` holds a complete mod-3 sequence starting at `base`.
fn is_complete_mod3(pile: &Pile, base: u8) -> bool {
    let cards = pile.cards();
    cards.len() == SEQ_LEN
        && cards[0].rank == base
        && cards
            .windows(2)
            .all(|w| w[1].rank == w[0].rank + 3 && w[1].suit == w[0].suit)
}

/// Append a refill to `base`: a working-row pile it leaves empty takes the
/// talon's top card, as KPatience does. The step is part of the same action,
/// so one undo takes back the move and the refill together.
fn with_refill(board: &Board, base: Action) -> Action {
    let mut scratch = board.clone();
    scratch.apply(&base);
    let mut steps = base.steps;

    for pile in TABLEAU..TABLEAU + ROWS {
        if scratch.pile(pile).is_empty() && !scratch.pile(STOCK).is_empty() {
            let step = Step {
                from: STOCK,
                to: pile,
                count: 1,
                reverse: false,
                was_face_up: false,
                face_up: true,
                flip_source: false,
                recycle: false,
            };
            scratch.apply(&Action::single(step));
            steps.push(step);
        }
    }

    Action::new(steps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Suit::{Clubs, Diamonds, Hearts, Spades};
    use crate::card::{ACE, Card, Suit};
    use std::collections::HashMap;

    fn c(rank: u8, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    /// Build a board for unit tests, given all 34 piles as slices.
    fn make_board(
        tableau: [&[Card]; 8],
        found2: [&[Card]; 8],
        found3: [&[Card]; 8],
        found4: [&[Card]; 8],
        stock: &[Card],
        aces: &[Card],
    ) -> Board {
        let mut piles: Vec<Pile> = tableau
            .iter()
            .map(|cards| Pile::tableau(0, cards))
            .collect();
        for row in [found2, found3, found4] {
            for cards in row {
                piles.push(Pile::holding(PileKind::Foundation, 0, cards));
            }
        }
        piles.push(Pile::holding(PileKind::Stock, stock.len(), stock));
        piles.push(Pile::holding(PileKind::Foundation, 0, aces));
        Board::new(0, piles)
    }

    fn empty8f() -> [&'static [Card]; 8] {
        [&[], &[], &[], &[], &[], &[], &[], &[]]
    }

    #[test]
    fn deal_has_the_right_shape() {
        let board = Mod3.deal(42);

        // Each of the 32 grid piles has exactly 1 face-up card.
        for i in 0..32 {
            let pile = board.pile(i);
            assert_eq!(pile.len(), 1, "grid pile {i} should have 1 card");
            assert!(pile.is_face_up(0), "pile {i} card should be face up");
        }

        // Talon holds the remaining 72 cards.
        assert_eq!(board.pile(STOCK).len(), 72);

        // Aces pile starts empty.
        assert!(board.pile(ACES).is_empty());

        // All 104 cards dealt exactly twice (two decks).
        let mut counts: HashMap<Card, usize> = HashMap::new();
        for i in 0..=ACES {
            for &card in board.pile(i).cards() {
                *counts.entry(card).or_default() += 1;
            }
        }
        assert_eq!(counts.len(), 52, "52 distinct card faces");
        assert!(
            counts.values().all(|&n| n == 2),
            "each face appears exactly twice"
        );
    }

    #[test]
    fn the_deal_is_pinned_to_its_seed() {
        // Guards the seed → board mapping. Changing the shuffle or deal order
        // breaks this test, which is the guarantee working.
        let board = Mod3.deal(42);
        assert_eq!(board.pile(TABLEAU).exposed(), Some(c(12, Hearts)));
        assert_eq!(board.pile(FOUND_2).exposed(), Some(c(5, Spades)));
        assert_eq!(board.pile(FOUND_3).exposed(), Some(c(5, Hearts)));
        assert_eq!(
            board.pile(STOCK).cards().last().copied(),
            Some(c(10, Spades))
        );
        assert_eq!(board.pile(STOCK).len(), 72);
    }

    #[test]
    fn foundation_2_accepts_base_rank_on_empty_pile() {
        let board = make_board(
            [&[c(2, Hearts)], &[], &[], &[], &[], &[], &[], &[]],
            empty8f(),
            empty8f(),
            empty8f(),
            &[],
            &[],
        );
        // 2♥ → empty FOUND_2 pile: OK.
        assert_eq!(Mod3.check(&board, Move::new(0, 0, FOUND_2)), Ok(()));
        // 5♥ → empty FOUND_2 pile: wrong (not base rank).
        let board2 = make_board(
            [&[c(5, Hearts)], &[], &[], &[], &[], &[], &[], &[]],
            empty8f(),
            empty8f(),
            empty8f(),
            &[],
            &[],
        );
        assert_eq!(
            Mod3.check(&board2, Move::new(0, 0, FOUND_2)),
            Err(MoveError::Mismatch)
        );
    }

    #[test]
    fn foundation_2_accepts_continuation_same_suit_plus_three() {
        // FOUND_2 pile already has 2♥; 5♥ may land on it.
        let board = make_board(
            [&[c(5, Hearts)], &[], &[], &[], &[], &[], &[], &[]],
            [&[c(2, Hearts)], &[], &[], &[], &[], &[], &[], &[]],
            empty8f(),
            empty8f(),
            &[],
            &[],
        );
        assert_eq!(Mod3.check(&board, Move::new(0, 0, FOUND_2)), Ok(()));
    }

    #[test]
    fn foundation_rejects_wrong_suit_or_wrong_rank() {
        // FOUND_2 has 2♥; 5♠ (wrong suit) should fail.
        let board = make_board(
            [&[c(5, Spades)], &[], &[], &[], &[], &[], &[], &[]],
            [&[c(2, Hearts)], &[], &[], &[], &[], &[], &[], &[]],
            empty8f(),
            empty8f(),
            &[],
            &[],
        );
        assert_eq!(
            Mod3.check(&board, Move::new(0, 0, FOUND_2)),
            Err(MoveError::Mismatch)
        );
        // FOUND_2 has 2♥; 8♥ (skips 5) should also fail.
        let board2 = make_board(
            [&[c(8, Hearts)], &[], &[], &[], &[], &[], &[], &[]],
            [&[c(2, Hearts)], &[], &[], &[], &[], &[], &[], &[]],
            empty8f(),
            empty8f(),
            &[],
            &[],
        );
        assert_eq!(
            Mod3.check(&board2, Move::new(0, 0, FOUND_2)),
            Err(MoveError::Mismatch)
        );
    }

    #[test]
    fn working_row_only_accepts_empty_target() {
        // Occupied target → Occupied error.
        let board = make_board(
            [
                &[c(5, Hearts)],
                &[c(3, Clubs)],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
            ],
            empty8f(),
            empty8f(),
            empty8f(),
            &[],
            &[],
        );
        assert_eq!(
            Mod3.check(&board, Move::new(0, 0, TABLEAU + 1)),
            Err(MoveError::Occupied)
        );
        // Empty target → OK.
        assert_eq!(Mod3.check(&board, Move::new(0, 0, TABLEAU + 2)), Ok(()));
    }

    #[test]
    fn only_aces_go_to_the_aces_pile() {
        let board = make_board(
            [
                &[c(ACE, Spades)],
                &[c(2, Hearts)],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
            ],
            empty8f(),
            empty8f(),
            empty8f(),
            &[],
            &[],
        );
        assert_eq!(Mod3.check(&board, Move::new(0, 0, ACES)), Ok(()));
        assert_eq!(
            Mod3.check(&board, Move::new(1, 0, ACES)),
            Err(MoveError::Mismatch)
        );
    }

    #[test]
    fn stock_and_aces_are_not_player_move_sources() {
        let board = Mod3.deal(1);
        // Cannot move from stock.
        assert_eq!(
            Mod3.check(&board, Move::new(STOCK, 0, 0)),
            Err(MoveError::WrongTarget)
        );
    }

    #[test]
    fn stock_deals_one_card_onto_each_working_row_pile() {
        let board = Mod3.deal(7);
        let action = Mod3.activate(&board, STOCK).expect("stock is not empty");

        let mut after = board.clone();
        after.apply(&action);

        // Each of the 8 tableau piles should have one more card.
        for i in 0..ROWS {
            assert_eq!(
                after.pile(TABLEAU + i).len(),
                board.pile(TABLEAU + i).len() + 1,
                "pile {i} should have grown by one"
            );
        }
        // Stock shrank by 8.
        assert_eq!(after.pile(STOCK).len(), board.pile(STOCK).len() - 8);

        // Undo restores the board.
        after.undo(&action);
        assert_eq!(after, board);
    }

    #[test]
    fn an_emptied_working_row_pile_is_refilled_from_the_talon() {
        let board = make_board(
            [
                &[c(2, Hearts)],
                &[c(ACE, Spades)],
                &[c(7, Clubs)],
                &[c(9, Diamonds)],
                &[c(4, Hearts)],
                &[c(6, Spades)],
                &[c(8, Clubs)],
                &[c(10, Diamonds)],
            ],
            empty8f(),
            empty8f(),
            empty8f(),
            &[c(7, Clubs), c(3, Diamonds)], // the talon still has cards
            &[],
        );

        // A foundation move, an Ace home, and `f` all refill their gap.
        let moves = [
            (TABLEAU, Mod3.action_for(&board, Move::new(0, 0, FOUND_2))),
            (TABLEAU + 1, Mod3.action_for(&board, Move::new(1, 0, ACES))),
            (
                TABLEAU + 1,
                Mod3.send_home(&board, 1).expect("the ace has a home"),
            ),
        ];
        for (emptied, action) in moves {
            assert_eq!(action.steps.len(), 2, "the move, then one refill");
            let mut after = board.clone();
            after.apply(&action);
            assert_eq!(after.pile(STOCK).len(), board.pile(STOCK).len() - 1);
            assert_eq!(after.pile(emptied).exposed(), Some(c(3, Diamonds)));
            assert!((TABLEAU..TABLEAU + ROWS).all(|p| !after.pile(p).is_empty()));
            after.undo(&action);
            assert_eq!(after, board);
        }
    }

    #[test]
    fn once_the_talon_is_spent_an_emptied_pile_stays_open() {
        let board = make_board(
            [&[c(2, Hearts)], &[], &[], &[], &[], &[], &[], &[]],
            empty8f(),
            empty8f(),
            empty8f(),
            &[],
            &[],
        );
        let action = Mod3.action_for(&board, Move::new(0, 0, FOUND_2));
        assert_eq!(action.steps.len(), 1, "nothing left to refill with");
    }

    #[test]
    fn a_misplaced_card_cannot_be_built_on() {
        // 6♠ dealt into the 2-row, and 5♠ — right row, wrong spot — beside it.
        let board = make_board(
            [
                &[c(9, Spades)],
                &[c(8, Spades)],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
            ],
            [
                &[c(6, Spades)],
                &[c(5, Spades)],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
            ],
            empty8f(),
            empty8f(),
            &[],
            &[],
        );
        assert_eq!(
            Mod3.check(&board, Move::new(0, 0, FOUND_2)),
            Err(MoveError::Mismatch)
        );
        assert_eq!(
            Mod3.check(&board, Move::new(1, 0, FOUND_2 + 1)),
            Err(MoveError::Mismatch)
        );
    }

    #[test]
    fn a_misplaced_card_is_hinted_out_of_its_row() {
        // 5♠ dealt into a bare 2-row spot; 2♠ is founded beside it.
        let board = make_board(
            [&[c(9, Hearts)], &[], &[], &[], &[], &[], &[], &[]],
            [
                &[c(5, Spades)],
                &[c(2, Spades)],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
            ],
            empty8f(),
            empty8f(),
            &[],
            &[],
        );
        let hints = Mod3.suggestions(&board);
        assert_eq!(hints.first(), Some(&Move::new(FOUND_2, 0, FOUND_2 + 1)));
        // Parking it in the working row is offered too, just ranked lower.
        assert!(hints.contains(&Move::new(FOUND_2, 0, TABLEAU + 1)));
        // The founded 2♠ is home, and `f` leaves it there.
        assert!(hints.iter().all(|mv| mv.from != FOUND_2 + 1));
        assert!(Mod3.send_home(&board, FOUND_2 + 1).is_none());
        assert!(Mod3.send_home(&board, FOUND_2).is_some());
    }

    #[test]
    fn a_lone_working_card_is_not_hinted_into_an_empty_pile() {
        let board = make_board(
            [
                &[c(9, Hearts)],
                &[c(7, Clubs), c(10, Hearts)],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
            ],
            empty8f(),
            empty8f(),
            empty8f(),
            &[],
            &[],
        );
        let hints = Mod3.suggestions(&board);
        assert!(hints.iter().all(|mv| mv.from != TABLEAU), "{hints:?}");
        assert!(hints.contains(&Move::new(TABLEAU + 1, 1, TABLEAU + 2)));
    }

    #[test]
    fn a_complete_board_is_a_win() {
        // Build a winning board: 8 complete sequences per row, 8 aces placed,
        // empty tableau and stock.
        let seq2 =
            |suit: Suit| -> Vec<Card> { [2, 5, 8, 11].iter().map(|&r| c(r, suit)).collect() };
        let seq3 =
            |suit: Suit| -> Vec<Card> { [3, 6, 9, 12].iter().map(|&r| c(r, suit)).collect() };
        let seq4 =
            |suit: Suit| -> Vec<Card> { [4, 7, 10, 13].iter().map(|&r| c(r, suit)).collect() };

        let suits = [Spades, Hearts, Diamonds, Clubs];
        let mut piles: Vec<Pile> = Vec::new();

        // Tableau: 8 empty piles.
        for _ in 0..8 {
            piles.push(Pile::new(PileKind::Tableau));
        }
        // FOUND_2: 4 suits × 2 decks.
        for &s in &suits {
            piles.push(Pile::holding(PileKind::Foundation, 0, &seq2(s)));
        }
        for &s in &suits {
            piles.push(Pile::holding(PileKind::Foundation, 0, &seq2(s)));
        }
        // FOUND_3: 4 suits × 2 decks.
        for &s in &suits {
            piles.push(Pile::holding(PileKind::Foundation, 0, &seq3(s)));
        }
        for &s in &suits {
            piles.push(Pile::holding(PileKind::Foundation, 0, &seq3(s)));
        }
        // FOUND_4: 4 suits × 2 decks.
        for &s in &suits {
            piles.push(Pile::holding(PileKind::Foundation, 0, &seq4(s)));
        }
        for &s in &suits {
            piles.push(Pile::holding(PileKind::Foundation, 0, &seq4(s)));
        }
        // Stock: empty.
        piles.push(Pile::new(PileKind::Stock));
        // Aces: all 8.
        let aces: Vec<Card> = suits.iter().flat_map(|&s| [c(ACE, s), c(ACE, s)]).collect();
        piles.push(Pile::holding(PileKind::Foundation, 0, &aces));

        let board = Board::new(0, piles);
        assert!(Mod3.is_won(&board));
    }

    #[test]
    fn incomplete_board_is_not_a_win() {
        // A partial win (only 7 aces placed) should not be counted.
        let seq2 =
            |suit: Suit| -> Vec<Card> { [2, 5, 8, 11].iter().map(|&r| c(r, suit)).collect() };
        let seq3 =
            |suit: Suit| -> Vec<Card> { [3, 6, 9, 12].iter().map(|&r| c(r, suit)).collect() };
        let seq4 =
            |suit: Suit| -> Vec<Card> { [4, 7, 10, 13].iter().map(|&r| c(r, suit)).collect() };

        let suits = [Spades, Hearts, Diamonds, Clubs];
        let mut piles: Vec<Pile> = Vec::new();
        for _ in 0..8 {
            piles.push(Pile::new(PileKind::Tableau));
        }
        for &s in &suits {
            piles.push(Pile::holding(PileKind::Foundation, 0, &seq2(s)));
        }
        for &s in &suits {
            piles.push(Pile::holding(PileKind::Foundation, 0, &seq2(s)));
        }
        for &s in &suits {
            piles.push(Pile::holding(PileKind::Foundation, 0, &seq3(s)));
        }
        for &s in &suits {
            piles.push(Pile::holding(PileKind::Foundation, 0, &seq3(s)));
        }
        for &s in &suits {
            piles.push(Pile::holding(PileKind::Foundation, 0, &seq4(s)));
        }
        for &s in &suits {
            piles.push(Pile::holding(PileKind::Foundation, 0, &seq4(s)));
        }
        piles.push(Pile::new(PileKind::Stock));
        // Only 7 aces on the aces pile.
        let aces: Vec<Card> = suits
            .iter()
            .flat_map(|&s| [c(ACE, s), c(ACE, s)])
            .take(7)
            .collect();
        piles.push(Pile::holding(PileKind::Foundation, 0, &aces));

        let board = Board::new(0, piles);
        assert!(!Mod3.is_won(&board));
    }

    #[test]
    fn send_home_finds_the_right_foundation() {
        // 2♥ on the working row; one empty FOUND_2 pile available.
        let board = make_board(
            [&[c(2, Hearts)], &[], &[], &[], &[], &[], &[], &[]],
            empty8f(),
            empty8f(),
            empty8f(),
            &[],
            &[],
        );
        let action = Mod3.send_home(&board, TABLEAU).expect("should find a home");
        let mut after = board.clone();
        after.apply(&action);
        // Card moved to one of the FOUND_2 piles.
        let placed =
            (FOUND_2..FOUND_2 + ROWS).any(|p| after.pile(p).exposed() == Some(c(2, Hearts)));
        assert!(placed, "2♥ should be on a FOUND_2 pile");
    }

    #[test]
    fn send_home_places_aces_on_the_aces_pile() {
        let board = make_board(
            [&[c(ACE, Clubs)], &[], &[], &[], &[], &[], &[], &[]],
            empty8f(),
            empty8f(),
            empty8f(),
            &[],
            &[],
        );
        let action = Mod3.send_home(&board, TABLEAU).expect("Ace has a home");
        let mut after = board.clone();
        after.apply(&action);
        assert_eq!(after.pile(ACES).len(), 1);
        assert_eq!(after.pile(ACES).exposed(), Some(c(ACE, Clubs)));
    }

    #[test]
    fn undo_round_trips_a_played_game() {
        let start = Mod3.deal(2024);
        let mut board = start.clone();
        let mut history = Vec::new();

        for step in 0..300 {
            // Prefer suggestions; fall back to activate.
            let moves = Mod3.suggestions(&board);
            if moves.is_empty() {
                match Mod3.activate(&board, STOCK) {
                    Some(action) => {
                        board.apply(&action);
                        history.push(action);
                    }
                    None => break,
                }
                continue;
            }
            let mv = moves[(step * 7 + 3) % moves.len()];
            let action = Mod3.action_for(&board, mv);
            board.apply(&action);
            history.push(action);
        }

        assert!(history.len() > 10, "played {} moves", history.len());
        while let Some(action) = history.pop() {
            board.undo(&action);
        }
        assert_eq!(board, start, "undo must fully restore the opening deal");
    }
}
