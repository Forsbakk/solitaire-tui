//! Scorpion solitaire.
//!
//! * 52 cards dealt into 7 columns of 7. Columns 1-4 start with three face-down
//!   cards; columns 5-7 are entirely face up. The remaining 3 cards are stock.
//! * A move lifts a face-up card *and everything below it* (a "tail", in any
//!   order) onto the exposed card of another column, which must be the same
//!   suit and exactly one rank higher.
//! * Only a King-headed tail may move to an empty column.
//! * When a tail leaves, a newly exposed face-down card flips up.
//! * The stock is dealt once, one card onto each of the first three columns.
//! * You win when every non-empty column is a complete K-to-A same-suit run.

use crate::board::{
    Action, Board, BoardSpec, Fan, Move, MoveError, Pile, PileId, PileKind, Spot, Step,
};
use crate::card::{KING, shuffled_deck};
use crate::rules::{Rules, StatusItem, StatusTone};

const COLUMNS: usize = 7;
const COLUMN_DEAL: usize = 7;
const FACE_DOWN_COLUMNS: usize = 4;
const FACE_DOWN_PER_COLUMN: usize = 3;
/// Columns the stock is dealt onto, one card each.
const STOCK_TARGETS: usize = 3;

/// The stock sits after the columns and is never drawn — it shows as a count in
/// the status bar, since its three cards arrive all at once.
const STOCK: PileId = COLUMNS;

#[derive(Default)]
pub struct Scorpion;

impl Rules for Scorpion {
    fn name(&self) -> &'static str {
        "Scorpion"
    }

    fn blurb(&self) -> &'static str {
        "Lift any face-up card together with every card below it — they travel \
         as a unit and need not be in order. Drop them on the same suit, one \
         rank higher. Only a King may take an empty column.\n\n\
         Win by building all four suits in the tableau, King down to Ace."
    }

    fn help(&self) -> &'static str {
        "  d             deal the three stock cards (once per game)"
    }

    fn deal(&self, seed: u64) -> Board {
        let mut deck = shuffled_deck(seed).into_iter();
        let mut piles: Vec<Pile> = (0..COLUMNS)
            .map(|i| {
                let cards: Vec<_> = deck.by_ref().take(COLUMN_DEAL).collect();
                let face_down = if i < FACE_DOWN_COLUMNS {
                    FACE_DOWN_PER_COLUMN
                } else {
                    0
                };
                Pile::tableau(face_down, &cards)
            })
            .collect();

        let stock: Vec<_> = deck.collect();
        piles.push(Pile::holding(PileKind::Stock, stock.len(), &stock));
        Board::new(seed, piles)
    }

    fn spec(&self, _board: &Board) -> BoardSpec {
        BoardSpec::new(
            (0..COLUMNS)
                .map(|i| Spot::new(i, 0, i as u16, Fan::Down))
                .collect(),
        )
    }

    fn check(&self, board: &Board, mv: Move) -> Result<(), MoveError> {
        let Move { from, index, to } = mv;
        if from == to {
            return Err(MoveError::SamePile);
        }
        let source = board.pile(from);
        let target = board.pile(to);
        if source.kind() != PileKind::Tableau || target.kind() != PileKind::Tableau {
            return Err(MoveError::WrongTarget);
        }
        if index >= source.len() {
            return Err(MoveError::NoSuchCard);
        }
        if !source.is_face_up(index) {
            return Err(MoveError::FaceDown);
        }

        let head = source.cards()[index];
        match target.exposed() {
            // A King's rank + 1 is 14, so this arm also blocks Kings from
            // landing on cards, which is what the rules require.
            Some(card) if card.suit == head.suit && card.rank == head.rank + 1 => Ok(()),
            Some(_) => Err(MoveError::Mismatch),
            None if head.rank == KING => Ok(()),
            None => Err(MoveError::NotKing),
        }
    }

    /// Deal the three stock cards, one onto each of the first three columns.
    fn activate(&self, board: &Board, pile: PileId) -> Option<Action> {
        if pile != STOCK || board.pile(STOCK).is_empty() {
            return None;
        }
        Some(Action::new(
            (0..STOCK_TARGETS)
                .map(|to| Step {
                    from: STOCK,
                    to,
                    count: 1,
                    reverse: false,
                    was_face_up: false,
                    face_up: true,
                    flip_source: false,
                    recycle: false,
                })
                .collect(),
        ))
    }

    fn is_won(&self, board: &Board) -> bool {
        board.pile(STOCK).is_empty()
            && (0..COLUMNS)
                .map(|i| board.pile(i))
                .all(|pile| pile.is_empty() || pile.is_complete_run())
    }

    /// Sliding a whole column into an empty one is legal but accomplishes
    /// nothing, so it is never offered.
    fn is_useful(&self, board: &Board, mv: Move) -> bool {
        !(mv.index == 0 && board.pile(mv.to).is_empty())
    }

    fn rank(&self, board: &Board, mv: Move) -> i32 {
        let Move { from, index, to } = mv;
        let source = board.pile(from);
        let mut value = 0;

        // Turning a card face up is the main way a Scorpion deal opens up.
        if index > 0 && index == source.face_down() {
            value += 100;
        }
        // Emptying a column creates the only place a King can go.
        if index == 0 {
            value += 50;
        }
        // Filling an empty column spends that scarce space.
        if board.pile(to).is_empty() {
            value -= 25;
        }
        // All else equal, prefer disturbing fewer cards.
        value - (source.len() - index) as i32
    }

    /// The stock still being there always counts as progress, even when the
    /// board itself is stuck.
    fn has_progress_move(&self, board: &Board) -> bool {
        !board.pile(STOCK).is_empty() || !self.suggestions(board).is_empty()
    }

    fn status(&self, board: &Board) -> Vec<StatusItem> {
        vec![match board.pile(STOCK).len() {
            0 => StatusItem::new("stock", "dealt", StatusTone::Muted),
            n => StatusItem::new("stock", "●".repeat(n), StatusTone::Good),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Suit::{Clubs, Diamonds, Hearts, Spades};
    use crate::card::{ACE, Card, Suit};
    use std::collections::HashSet;

    fn c(rank: u8, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    /// A board with hand-built columns, for testing specific positions.
    fn board(columns: &[Pile], stock: &[Card]) -> Board {
        let mut piles: Vec<Pile> = columns.to_vec();
        piles.resize(COLUMNS, Pile::tableau(0, &[]));
        piles.push(Pile::holding(PileKind::Stock, stock.len(), stock));
        Board::new(0, piles)
    }

    /// Every legal move in the current position, useful or not.
    fn legal_moves(board: &Board) -> Vec<Move> {
        let rules = Scorpion;
        let mut moves = Vec::new();
        for from in 0..COLUMNS {
            let source = board.pile(from);
            let Some(first) = source.first_face_up() else {
                continue;
            };
            for index in first..source.len() {
                for to in 0..COLUMNS {
                    let mv = Move::new(from, index, to);
                    if rules.check(board, mv).is_ok() {
                        moves.push(mv);
                    }
                }
            }
        }
        moves
    }

    #[test]
    fn deal_has_the_right_shape() {
        let board = Scorpion.deal(42);

        assert_eq!(board.pile(STOCK).len(), 3);
        let mut seen = HashSet::new();
        for i in 0..COLUMNS {
            let pile = board.pile(i);
            assert_eq!(pile.len(), 7);
            assert_eq!(
                pile.first_face_up(),
                Some(if i < 4 { 3 } else { 0 }),
                "columns 1-4 start with three face-down cards"
            );
            seen.extend(pile.cards().iter().copied());
        }
        seen.extend(board.pile(STOCK).cards().iter().copied());
        assert_eq!(seen.len(), 52, "all 52 cards dealt exactly once");
    }

    #[test]
    fn the_deal_is_pinned_to_its_seed() {
        // Guards the whole seed -> board mapping: the app's own tests pick
        // specific cards out of seed 42, the README's sample board is this
        // deal, and players expect a seed to keep dealing what it always dealt.
        let board = Scorpion.deal(42);
        assert_eq!(
            board.pile(4).cards()[..4],
            [c(4, Diamonds), c(3, Hearts), c(10, Diamonds), c(11, Clubs)]
        );
        assert_eq!(board.pile(4).exposed(), Some(c(7, Clubs)));
        assert_eq!(board.pile(6).exposed(), Some(c(12, Diamonds)));
        assert_eq!(
            board.pile(0).cards()[3],
            c(ACE, Clubs),
            "the first face-up card"
        );
        assert_eq!(
            board.pile(STOCK).cards(),
            &[c(9, Spades), c(11, Spades), c(10, Spades)]
        );
    }

    #[test]
    fn tail_lands_on_same_suit_one_rank_higher() {
        let mut board = board(
            &[
                Pile::tableau(0, &[c(9, Hearts)]),
                Pile::tableau(0, &[c(10, Hearts)]),
            ],
            &[],
        );

        let mv = Move::new(0, 0, 1);
        assert_eq!(Scorpion.check(&board, mv), Ok(()));
        board.apply(&Scorpion.action_for(&board, mv));
        assert!(board.pile(0).is_empty());
        assert_eq!(board.pile(1).cards(), &[c(10, Hearts), c(9, Hearts)]);
    }

    #[test]
    fn tail_moves_regardless_of_its_own_order() {
        // 9♥ with junk below it: the whole tail travels together.
        let mut board = board(
            &[
                Pile::tableau(
                    0,
                    &[c(2, Clubs), c(9, Hearts), c(4, Spades), c(ACE, Diamonds)],
                ),
                Pile::tableau(0, &[c(10, Hearts)]),
            ],
            &[],
        );

        let mv = Move::new(0, 1, 1);
        board.apply(&Scorpion.action_for(&board, mv));
        assert_eq!(board.pile(0).cards(), &[c(2, Clubs)]);
        assert_eq!(
            board.pile(1).cards(),
            &[c(10, Hearts), c(9, Hearts), c(4, Spades), c(ACE, Diamonds)]
        );
    }

    #[test]
    fn illegal_drops_are_rejected() {
        let board = board(
            &[
                Pile::tableau(2, &[c(3, Clubs), c(4, Clubs), c(9, Hearts)]),
                Pile::tableau(0, &[c(10, Spades)]),
                Pile::tableau(0, &[c(8, Hearts)]),
                Pile::tableau(0, &[c(KING, Hearts)]),
            ],
            &[],
        );
        let check = |from, index, to| Scorpion.check(&board, Move::new(from, index, to));

        assert_eq!(check(0, 2, 0), Err(MoveError::SamePile));
        assert_eq!(check(0, 1, 1), Err(MoveError::FaceDown));
        assert_eq!(check(0, 9, 1), Err(MoveError::NoSuchCard));
        assert_eq!(check(0, 2, 1), Err(MoveError::Mismatch), "wrong suit");
        assert_eq!(check(0, 2, 2), Err(MoveError::Mismatch), "wrong rank");
        assert_eq!(
            check(3, 0, 2),
            Err(MoveError::Mismatch),
            "a King never lands on a card"
        );
    }

    #[test]
    fn the_stock_is_not_a_place_to_put_cards() {
        let board = Scorpion.deal(42);
        assert_eq!(
            Scorpion.check(&board, Move::new(4, 6, STOCK)),
            Err(MoveError::WrongTarget)
        );
        assert_eq!(
            Scorpion.check(&board, Move::new(STOCK, 0, 4)),
            Err(MoveError::WrongTarget)
        );
        assert!(
            !Scorpion
                .suggestions(&board)
                .iter()
                .any(|mv| mv.from == STOCK || mv.to == STOCK)
        );
    }

    #[test]
    fn only_a_king_may_take_an_empty_column() {
        let board = board(
            &[
                Pile::tableau(0, &[c(KING, Spades), c(4, Clubs)]),
                Pile::tableau(0, &[c(2, Diamonds)]),
            ],
            &[],
        );
        let check = |from, index, to| Scorpion.check(&board, Move::new(from, index, to));

        assert_eq!(check(0, 0, 6), Ok(()), "King-headed tail");
        assert_eq!(check(0, 1, 6), Err(MoveError::NotKing));
        assert_eq!(check(1, 0, 6), Err(MoveError::NotKing));
    }

    #[test]
    fn move_flips_the_newly_exposed_card() {
        let mut board = board(
            &[
                Pile::tableau(2, &[c(3, Clubs), c(4, Clubs), c(9, Hearts)]),
                Pile::tableau(0, &[c(10, Hearts)]),
            ],
            &[],
        );
        let start = board.clone();

        let action = Scorpion.action_for(&board, Move::new(0, 2, 1));
        board.apply(&action);
        assert_eq!(board.pile(0).face_down(), 1, "4♣ turned face up");
        assert!(action.steps[0].flip_source);

        board.undo(&action);
        assert_eq!(board, start, "undo hides it again");
    }

    #[test]
    fn emptying_a_column_flips_nothing() {
        let board = board(
            &[
                Pile::tableau(0, &[c(9, Hearts)]),
                Pile::tableau(0, &[c(10, Hearts)]),
            ],
            &[],
        );
        let action = Scorpion.action_for(&board, Move::new(0, 0, 1));
        assert!(!action.steps[0].flip_source);
    }

    #[test]
    fn stock_deals_three_cards_and_undoes_cleanly() {
        let start = Scorpion.deal(5);
        let mut board = start.clone();

        let action = Scorpion
            .activate(&board, STOCK)
            .expect("stock is not empty");
        board.apply(&action);
        assert!(board.pile(STOCK).is_empty());
        for i in 0..STOCK_TARGETS {
            assert_eq!(board.pile(i).len(), 8);
            assert!(board.pile(i).is_face_up(7), "a dealt card arrives face up");
        }
        assert!(
            Scorpion.activate(&board, STOCK).is_none(),
            "the stock deals only once"
        );

        board.undo(&action);
        assert_eq!(board, start, "stock order and columns fully restored");
    }

    #[test]
    fn undo_round_trips_a_played_game() {
        let start = Scorpion.deal(2024);
        let mut board = start.clone();
        let mut history = Vec::new();

        for step in 0..300 {
            let moves = legal_moves(&board);
            if moves.is_empty() {
                match Scorpion.activate(&board, STOCK) {
                    Some(action) => {
                        board.apply(&action);
                        history.push(action);
                    }
                    None => break,
                }
                continue;
            }
            // Deterministic but varied choice, so the walk explores flips,
            // empties and multi-card tails.
            let mv = moves[(step * 7 + 3) % moves.len()];
            let action = Scorpion.action_for(&board, mv);
            board.apply(&action);
            history.push(action);
        }

        assert!(history.len() > 20, "played {} moves", history.len());
        while let Some(action) = history.pop() {
            board.undo(&action);
        }
        assert_eq!(board, start);
    }

    #[test]
    fn redo_replays_a_move_exactly() {
        let mut board = Scorpion.deal(11);
        let mv = legal_moves(&board)[0];

        let action = Scorpion.action_for(&board, mv);
        board.apply(&action);
        let after = board.clone();

        board.undo(&action);
        board.redo(&action);
        assert_eq!(board, after);
    }

    #[test]
    fn a_board_of_four_runs_is_a_win() {
        let run = |suit| Pile::tableau(0, &(0..13).map(|i| c(13 - i, suit)).collect::<Vec<_>>());
        let columns: Vec<Pile> = Suit::ALL.into_iter().map(run).collect();
        assert!(Scorpion.is_won(&board(&columns, &[])));

        // Same cards, but one run spliced out of order.
        let mut broken = columns.clone();
        let mut cards = broken[0].cards().to_vec();
        cards.swap(3, 4);
        broken[0] = Pile::tableau(0, &cards);
        assert!(!Scorpion.is_won(&board(&broken, &[])), "order matters");

        // A complete board with cards still in the stock is not a win.
        assert!(!Scorpion.is_won(&board(&columns, &[c(ACE, Spades)])));
    }

    #[test]
    fn a_run_with_a_face_down_card_is_not_a_win() {
        let columns: Vec<Pile> = Suit::ALL
            .into_iter()
            .enumerate()
            .map(|(i, suit)| {
                Pile::tableau(
                    usize::from(i == 0),
                    &(0..13).map(|r| c(13 - r, suit)).collect::<Vec<_>>(),
                )
            })
            .collect();
        assert!(!Scorpion.is_won(&board(&columns, &[])));
    }

    #[test]
    fn dead_positions_report_no_progress() {
        let stuck = [
            Pile::tableau(0, &[c(2, Spades)]),
            Pile::tableau(0, &[c(7, Hearts)]),
        ];
        assert!(!Scorpion.has_progress_move(&board(&stuck, &[])));

        let alive = [
            Pile::tableau(0, &[c(2, Spades)]),
            Pile::tableau(0, &[c(3, Spades)]),
        ];
        assert!(Scorpion.has_progress_move(&board(&alive, &[])));

        // A lone King with an empty column: legal, but it changes nothing.
        let pointless = [Pile::tableau(0, &[c(KING, Spades)])];
        assert!(!Scorpion.has_progress_move(&board(&pointless, &[])));

        // Stock still to come always counts as progress.
        let waiting = [Pile::tableau(0, &[c(2, Spades)])];
        assert!(Scorpion.has_progress_move(&board(&waiting, &[c(ACE, Hearts)])));
    }

    #[test]
    fn every_suggestion_is_a_legal_move_that_changes_something() {
        let board = Scorpion.deal(42);
        let suggestions = Scorpion.suggestions(&board);
        assert!(!suggestions.is_empty(), "the opening deal has moves");

        for mv in &suggestions {
            assert_eq!(Scorpion.check(&board, *mv), Ok(()));
            assert!(
                !(mv.index == 0 && board.pile(mv.to).is_empty()),
                "shuffling a whole column into an empty one is not a move"
            );
        }
    }

    #[test]
    fn turning_a_card_over_is_suggested_ahead_of_a_move_that_does_not() {
        let board = board(
            &[
                // Lifting the 9♥ turns the 4♣ underneath it face up.
                Pile::tableau(2, &[c(3, Clubs), c(4, Clubs), c(9, Hearts)]),
                Pile::tableau(0, &[c(10, Hearts)]),
                // Whereas this one just shuffles two face-up cards around.
                Pile::tableau(0, &[c(5, Spades), c(2, Diamonds)]),
                Pile::tableau(0, &[c(3, Diamonds)]),
            ],
            &[],
        );

        let suggestions = Scorpion.suggestions(&board);
        assert_eq!(
            suggestions.first(),
            Some(&Move::new(0, 2, 1)),
            "got {suggestions:?}"
        );
        assert_eq!(suggestions.len(), 2, "the other move is still offered");
    }

    #[test]
    fn a_dead_position_suggests_nothing() {
        let stuck = [
            Pile::tableau(0, &[c(2, Spades)]),
            Pile::tableau(0, &[c(7, Hearts)]),
        ];
        assert!(Scorpion.suggestions(&board(&stuck, &[])).is_empty());
    }
}
