//! Mod 3 — two decks, a grid of piles, and foundations built by threes.
//!
//! * Two 52-card decks (104 cards). The eight Aces are held on their own pile
//!   and are never built upon — they simply leave the grid.
//! * The board has a 4×8 grid of 32 piles, plus a talon and the aces pile.
//!   One card is dealt face-up to each of the 32 positions; the remaining 72
//!   cards stay in the talon.
//! * Row 1 builds 2→5→8→J in the same suit.
//! * Row 2 builds 3→6→9→Q in the same suit.
//! * Row 3 builds 4→7→10→K in the same suit.
//! * Row 4 (the working row) holds cards as a staging area. A player move may
//!   only land on an empty working-row pile. Pressing `d` deals one card from
//!   the talon onto each of the eight working-row piles (even non-empty ones).
//!   When a player move empties a pile there, the talon auto-fills it.
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
         The fourth row is a working area. Press d to deal one card onto each \
         of its eight piles. Emptying a working-row pile auto-refills it from \
         the talon."
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

        // Row 0: talon at left, aces pile at right.
        spots.push(Spot::new(STOCK, 0, 0, Fan::None));
        spots.push(Spot::new(ACES, 0, 7, Fan::None));

        // Rows 1–3: the foundations fan down so every card in a stack can be read.
        // Row 1: FOUND_2 (2→5→8→J).
        for i in 0..ROWS {
            spots.push(Spot::new(FOUND_2 + i, 1, i as u16, Fan::Down));
        }
        // Row 2: FOUND_3 (3→6→9→Q).
        for i in 0..ROWS {
            spots.push(Spot::new(FOUND_3 + i, 2, i as u16, Fan::Down));
        }
        // Row 3: FOUND_4 (4→7→10→K).
        for i in 0..ROWS {
            spots.push(Spot::new(FOUND_4 + i, 3, i as u16, Fan::Down));
        }
        // Row 4: working row. Fan::Down so talon deals can stack.
        for i in 0..ROWS {
            spots.push(Spot::new(TABLEAU + i, 4, i as u16, Fan::Down));
        }

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

        match to {
            STOCK => Err(MoveError::WrongTarget),
            ACES => {
                if card.rank == ACE {
                    Ok(())
                } else {
                    Err(MoveError::Mismatch)
                }
            }
            t if (FOUND_2..FOUND_2 + ROWS).contains(&t) => foundation_check(target, card, 2),
            t if (FOUND_3..FOUND_3 + ROWS).contains(&t) => foundation_check(target, card, 3),
            t if (FOUND_4..FOUND_4 + ROWS).contains(&t) => foundation_check(target, card, 4),
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

    /// Wraps every player move with an auto-fill step for any working-row pile
    /// that the move leaves empty, as long as the talon still has cards.
    fn action_for(&self, board: &Board, mv: Move) -> Action {
        with_autofill(board, Action::moving(board, mv.from, mv.index, mv.to))
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
        let source = board.pile(from);
        let card = source.exposed()?;
        let last = source.len() - 1;

        let dest = if card.rank == ACE {
            // Send to the aces pile if it still has room.
            (board.pile(ACES).len() < 8).then_some(ACES)?
        } else {
            let (mut range, base): (std::ops::Range<PileId>, u8) = match card.rank % 3 {
                2 => (FOUND_2..FOUND_2 + ROWS, 2),
                0 => (FOUND_3..FOUND_3 + ROWS, 3),
                1 => (FOUND_4..FOUND_4 + ROWS, 4),
                _ => unreachable!(),
            };
            range.find(|&p| foundation_takes(board, p, card, base))?
        };

        Some(Action::moving(board, from, last, dest))
    }

    fn is_won(&self, board: &Board) -> bool {
        board.pile(ACES).len() == 8
            && (FOUND_2..FOUND_2 + ROWS).all(|p| is_complete_mod3(board.pile(p), 2))
            && (FOUND_3..FOUND_3 + ROWS).all(|p| is_complete_mod3(board.pile(p), 3))
            && (FOUND_4..FOUND_4 + ROWS).all(|p| is_complete_mod3(board.pile(p), 4))
    }

    fn is_useful(&self, board: &Board, mv: Move) -> bool {
        // Moving FROM a sequence foundation is only worthwhile when placing an
        // Ace onto the aces pile (which can happen if an Ace was dealt there).
        if board.pile(mv.from).kind() == PileKind::Foundation {
            return mv.to == ACES && board.pile(mv.from).exposed().is_some_and(|c| c.rank == ACE);
        }
        true
    }

    fn rank(&self, board: &Board, mv: Move) -> i32 {
        if mv.to == ACES {
            return 80;
        }
        if (FOUND_2..FOUND_4 + ROWS).contains(&mv.to) {
            // Continuing an existing sequence beats starting a new one slightly,
            // to prefer making progress over claiming new slots.
            return if board.pile(mv.to).is_empty() {
                90
            } else {
                100
            };
        }
        // Parking to an empty working-row pile.
        10
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

/// Whether `card` may be placed on `pile`, which builds from `base` upward by 3.
fn foundation_takes(board: &Board, pile: PileId, card: Card, base: u8) -> bool {
    match board.pile(pile).exposed() {
        None => card.rank == base,
        Some(top) => top.suit == card.suit && card.rank == top.rank + 3,
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
        Some(top) => {
            if top.suit == card.suit && card.rank == top.rank + 3 {
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

/// Append auto-fill steps: for each empty working-row pile after `base`, deal
/// from the talon. Mirroring Spider's run-filing so one undo reverses all of it.
fn with_autofill(board: &Board, base: Action) -> Action {
    let mut scratch = board.clone();
    scratch.apply(&base);
    let mut steps = base.steps;

    for i in 0..ROWS {
        if scratch.pile(TABLEAU + i).is_empty() && !scratch.pile(STOCK).is_empty() {
            let step = Step {
                from: STOCK,
                to: TABLEAU + i,
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
    fn auto_fill_refills_emptied_working_row_pile() {
        // All 8 tableau piles have a card so only the moved one becomes empty.
        let board = make_board(
            [
                &[c(2, Hearts)],
                &[c(5, Spades)],
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
            &[c(7, Clubs), c(3, Diamonds)], // stock has cards
            &[],
        );
        let mv = Move::new(0, 0, FOUND_2); // move 2♥ to a FOUND_2 pile
        let action = Mod3.action_for(&board, mv);

        // Move + exactly one auto-fill step (only pile 0 became empty).
        assert_eq!(
            action.steps.len(),
            2,
            "expected move + auto-fill; got {} steps",
            action.steps.len()
        );

        let mut after = board.clone();
        after.apply(&action);

        // Pile 0 was emptied then auto-filled from stock.
        assert_eq!(after.pile(TABLEAU).len(), 1, "auto-fill put a card back");
        // Stock shrank by 1 (the auto-fill card).
        assert_eq!(after.pile(STOCK).len(), board.pile(STOCK).len() - 1);

        // Full undo round-trips.
        after.undo(&action);
        assert_eq!(after, board);
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
