//! Russian Solitaire — Yukon built in suit.
//!
//! Every rule is [`super::yukon`]'s: the same deal of 1, 6, 7, 8, 9, 10 and 11
//! cards, the same free-tail lift, the same four foundations built up in suit
//! from the Ace, and only a King into an empty pile. The single difference is
//! what a card may be dropped on — its **own suit**, one rank higher, rather
//! than the opposite colour.
//!
//! That one word costs the player most of their legal moves, which is why the
//! game is so much harder than Yukon and so rarely goes home.

use crate::board::{Action, Board, BoardSpec, Move, MoveError, PileId};
use crate::rules::yukon::{self, Build};
use crate::rules::{Rules, StatusItem};

#[derive(Default)]
pub struct Russian;

impl Rules for Russian {
    fn name(&self) -> &'static str {
        "Russian"
    }

    fn blurb(&self) -> &'static str {
        "Russian Solitaire: Yukon built in suit. Lift any face-up card together \
         with every card below it — they travel as a unit and need not be in \
         order — and drop them on the same suit, one rank higher. Only a King \
         may take an empty pile.\n\n\
         There is no stock, and the four foundations build up in suit from the \
         Ace. Building in suit rather than alternating colour leaves far fewer \
         places to put anything, so expect to lose most deals."
    }

    fn help(&self) -> &'static str {
        yukon::HELP
    }

    fn deal(&self, seed: u64) -> Board {
        yukon::deal(seed)
    }

    fn spec(&self, _board: &Board) -> BoardSpec {
        yukon::spec()
    }

    fn check(&self, board: &Board, mv: Move) -> Result<(), MoveError> {
        yukon::check(Build::SameSuit, board, mv)
    }

    fn is_won(&self, board: &Board) -> bool {
        yukon::is_won(board)
    }

    fn send_home(&self, board: &Board, from: PileId) -> Option<Action> {
        yukon::send_home(board, from)
    }

    fn autoplay(&self, board: &Board) -> Option<Action> {
        yukon::autoplay(board)
    }

    fn is_useful(&self, board: &Board, mv: Move) -> bool {
        yukon::is_useful(board, mv)
    }

    fn rank(&self, board: &Board, mv: Move) -> i32 {
        yukon::rank(board, mv)
    }

    fn status(&self, board: &Board) -> Vec<StatusItem> {
        yukon::status(board)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{Pile, PileKind};
    use crate::card::Suit::{Clubs, Diamonds, Hearts, Spades};
    use crate::card::{ACE, Card, KING, Suit};
    use crate::rules::yukon::Yukon;

    fn c(rank: u8, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    /// Russian Solitaire's board is Yukon's, so the tableau is seven piles and
    /// the foundations follow them.
    fn board(tableau: &[Pile]) -> Board {
        let mut piles: Vec<Pile> = tableau.to_vec();
        piles.resize(7, Pile::tableau(0, &[]));
        piles.resize(11, Pile::new(PileKind::Foundation));
        Board::new(0, piles)
    }

    #[test]
    fn the_deal_is_yukons() {
        assert_eq!(Russian.deal(42), Yukon.deal(42));
        assert_eq!(Russian.deal(2024), Yukon.deal(2024));
    }

    #[test]
    fn the_deal_is_pinned_to_its_seed() {
        let board = Russian.deal(42);
        assert_eq!(board.pile(0).cards(), &[c(7, Spades)]);
        assert_eq!(board.pile(6).exposed(), Some(c(10, Spades)));
    }

    #[test]
    fn a_tail_lands_on_its_own_suit_one_rank_higher() {
        let board = board(&[
            Pile::tableau(0, &[c(9, Hearts)]),
            Pile::tableau(0, &[c(10, Hearts)]),
            Pile::tableau(0, &[c(10, Spades)]),
        ]);
        let check = |from, index, to| Russian.check(&board, Move::new(from, index, to));

        assert_eq!(check(0, 0, 1), Ok(()), "9♥ onto 10♥");
        assert_eq!(
            check(0, 0, 2),
            Err(MoveError::Mismatch),
            "the alternating-colour drop Yukon allows"
        );
    }

    #[test]
    fn it_differs_from_yukon_in_exactly_the_build_rule() {
        let board = board(&[
            Pile::tableau(0, &[c(9, Hearts)]),
            Pile::tableau(0, &[c(10, Spades)]),
        ]);
        let mv = Move::new(0, 0, 1);
        assert_eq!(Yukon.check(&board, mv), Ok(()));
        assert_eq!(Russian.check(&board, mv), Err(MoveError::Mismatch));
    }

    #[test]
    fn only_a_king_may_take_an_empty_pile() {
        let board = board(&[
            Pile::tableau(0, &[c(KING, Spades), c(4, Clubs)]),
            Pile::tableau(0, &[c(2, Diamonds)]),
        ]);
        let check = |from, index, to| Russian.check(&board, Move::new(from, index, to));

        assert_eq!(check(0, 0, 6), Ok(()));
        assert_eq!(check(0, 1, 6), Err(MoveError::NotKing));
    }

    #[test]
    fn the_foundations_are_yukons() {
        let board = board(&[Pile::tableau(0, &[c(ACE, Spades)])]);
        assert_eq!(Russian.check(&board, Move::new(0, 0, 7)), Ok(()));
        assert!(Russian.send_home(&board, 0).is_some());

        let suited = |suit| {
            Pile::holding(
                PileKind::Foundation,
                0,
                &(ACE..=KING).map(|rank| c(rank, suit)).collect::<Vec<_>>(),
            )
        };
        let mut piles: Vec<Pile> = vec![Pile::tableau(0, &[]); 7];
        piles.extend(Suit::ALL.into_iter().map(suited));
        assert!(Russian.is_won(&Board::new(0, piles)));
    }

    #[test]
    fn undo_round_trips_a_played_game() {
        let start = Russian.deal(2024);
        let mut board = start.clone();
        let mut history = Vec::new();

        for step in 0..400 {
            let moves = Russian.suggestions(&board);
            if moves.is_empty() {
                break;
            }
            let action = Russian.action_for(&board, moves[(step * 7 + 3) % moves.len()]);
            board.apply(&action);
            history.push(action);
        }

        assert!(history.len() > 10, "played {} moves", history.len());
        while let Some(action) = history.pop() {
            board.undo(&action);
        }
        assert_eq!(board, start);
    }
}
