//! Spiderette — Spider on one deck and seven columns.
//!
//! Every rule is [`super::spider`]'s: build **down regardless of suit**, lift
//! only a **same-suit run** as a unit, any card takes an empty column, the
//! stock deals one card onto every column at once and refuses while a column
//! stands empty, and a finished King-to-Ace run files itself. The whole
//! difference is a [`Setup`] — one deck instead of two, seven columns instead
//! of ten, and so four runs to find rather than eight.
//!
//! One consequence of the smaller deck is worth knowing at the table: 52 cards
//! less the 28 dealt leaves 24, which is three full rounds of seven and a last
//! round of three. That short deal covers the leftmost three columns only.

use crate::board::{Action, Board, BoardSpec, Move, MoveError, PileId};
use crate::rules::spider::{self, Setup};
use crate::rules::{Rules, StatusItem};

const SETUP: Setup = Setup::SPIDERETTE;

#[derive(Default)]
pub struct Spiderette;

impl Rules for Spiderette {
    fn name(&self) -> &'static str {
        "Spiderette"
    }

    fn blurb(&self) -> &'static str {
        "Spider on a single deck. Seven columns dealt 1 to 7 cards. Build down \
         regardless of suit, but only a run of one suit may be lifted as a \
         unit, and any card takes an empty column.\n\n\
         The stock deals a card onto every column at once and will not deal \
         while a column stands empty, so an empty column is both the room you \
         need and the thing you must give up to deal. A finished King-to-Ace \
         run files itself. Win by filing all four."
    }

    fn help(&self) -> &'static str {
        spider::HELP
    }

    fn deal(&self, seed: u64) -> Board {
        spider::deal(SETUP, seed)
    }

    fn spec(&self, _board: &Board) -> BoardSpec {
        spider::spec(SETUP)
    }

    fn check(&self, board: &Board, mv: Move) -> Result<(), MoveError> {
        spider::check(board, mv)
    }

    fn action_for(&self, board: &Board, mv: Move) -> Action {
        spider::action_for(SETUP, board, mv)
    }

    fn activate(&self, board: &Board, pile: PileId) -> Option<Action> {
        spider::activate(SETUP, board, pile)
    }

    fn stock_refusal(&self, board: &Board) -> &'static str {
        spider::stock_refusal(SETUP, board)
    }

    fn is_won(&self, board: &Board) -> bool {
        spider::is_won(SETUP, board)
    }

    fn is_useful(&self, board: &Board, mv: Move) -> bool {
        spider::is_useful(board, mv)
    }

    fn rank(&self, board: &Board, mv: Move) -> i32 {
        spider::rank(board, mv)
    }

    fn has_progress_move(&self, board: &Board) -> bool {
        spider::stock_deals(SETUP, board) || !self.suggestions(board).is_empty()
    }

    fn status(&self, board: &Board) -> Vec<StatusItem> {
        spider::status(SETUP, board)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{Pile, PileKind};
    use crate::card::Suit::{Clubs, Diamonds, Hearts, Spades};
    use crate::card::{ACE, Card, KING, Suit};
    use crate::rules::spider::Spider;
    use std::collections::HashSet;

    const COLUMNS: usize = 7;
    const FOUNDATION: PileId = COLUMNS;
    const STOCK: PileId = COLUMNS + 4;

    fn c(rank: u8, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    fn filed(suit: Suit) -> Pile {
        let cards: Vec<_> = (ACE..=KING).map(|rank| c(rank, suit)).collect();
        Pile::holding(PileKind::Foundation, 0, &cards)
    }

    #[test]
    fn the_deal_stairs_one_to_seven_like_klondike() {
        let board = Spiderette.deal(42);
        assert_eq!(
            board.piles().len(),
            12,
            "seven columns, four homes, a stock"
        );

        for column in 0..COLUMNS {
            let pile = board.pile(column);
            assert_eq!(pile.len(), column + 1, "column {column}");
            assert_eq!(pile.face_down(), column, "only the last card shows");
        }
        assert_eq!(board.pile(STOCK).len(), 24, "and 24 left to deal");
    }

    #[test]
    fn the_deal_uses_one_deck_so_every_card_is_unique() {
        // A `HashSet` is the right tool here and the wrong one in `spider.rs`:
        // on one deck a card value really does name a single card.
        let board = Spiderette.deal(42);
        let cards: HashSet<Card> = board
            .piles()
            .iter()
            .flat_map(Pile::cards)
            .copied()
            .collect();
        assert_eq!(cards.len(), 52);
    }

    #[test]
    fn it_builds_and_lifts_exactly_as_spider_does() {
        let mut piles = vec![
            Pile::tableau(0, &[c(10, Spades), c(9, Spades), c(8, Hearts)]),
            Pile::tableau(0, &[c(9, Diamonds)]),
            Pile::tableau(0, &[c(10, Clubs)]),
        ];
        piles.resize(COLUMNS, Pile::tableau(0, &[]));
        piles.resize(STOCK, Pile::new(PileKind::Foundation));
        piles.push(Pile::new(PileKind::Stock));
        let board = Board::new(0, piles);

        for mv in [
            Move::new(0, 2, 1), // the lone 8♥ onto the 9♦
            Move::new(0, 1, 1), // 9♠8♥, which is not a suited run
            Move::new(0, 0, 2), // 10♠ onto the 10♣
            Move::new(1, 0, 3), // anything into an empty column
        ] {
            assert_eq!(
                Spiderette.check(&board, mv),
                Spider::default().check(&board, mv),
                "{mv:?}",
            );
        }
    }

    #[test]
    fn the_last_deal_covers_only_the_leftmost_three_columns() {
        let mut board = Spiderette.deal(42);
        let mut dealt = Vec::new();

        // 24 cards over seven columns is three full rounds and a short one.
        while let Some(action) = Spiderette.activate(&board, STOCK) {
            dealt.push(action.steps.len());
            board.apply(&action);
        }
        assert_eq!(dealt, vec![7, 7, 7, 3], "and the short deal is last");
        assert!(board.pile(STOCK).is_empty());
    }

    #[test]
    fn four_finished_runs_are_a_win() {
        let mut piles: Vec<Pile> = Vec::new();
        piles.resize(COLUMNS, Pile::tableau(0, &[]));
        piles.extend(Suit::ALL.iter().map(|&suit| filed(suit)));
        piles.push(Pile::new(PileKind::Stock));
        assert!(Spiderette.is_won(&Board::new(0, piles.clone())));

        piles[FOUNDATION] = Pile::new(PileKind::Foundation);
        assert!(!Spiderette.is_won(&Board::new(0, piles)), "three is not it");
    }

    #[test]
    fn the_deal_is_pinned_to_its_seed() {
        let board = Spiderette.deal(42);
        // The same stair as Klondike's off the same seed, since both deal
        // 1 to 7 off the front of one shuffled deck.
        assert_eq!(board.pile(0).cards(), &[c(7, Spades)]);
        assert_eq!(board.pile(6).exposed(), Some(c(10, Clubs)));
        assert_eq!(Spiderette.deal(42), Spiderette.deal(42));
    }

    #[test]
    fn undo_round_trips_a_played_game() {
        let start = Spiderette.deal(2024);
        let mut board = start.clone();
        let mut history = Vec::new();

        for step in 0..300 {
            let moves = Spiderette.suggestions(&board);
            let action = if moves.is_empty() {
                match Spiderette.activate(&board, STOCK) {
                    Some(action) => action,
                    None => break,
                }
            } else {
                Spiderette.action_for(&board, moves[(step * 7 + 3) % moves.len()])
            };
            board.apply(&action);
            history.push(action);
        }

        assert!(history.len() > 20, "played {} moves", history.len());
        while let Some(action) = history.pop() {
            board.undo(&action);
        }
        assert_eq!(board, start);
    }
}
