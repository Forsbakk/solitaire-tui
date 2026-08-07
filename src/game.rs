//! Scorpion solitaire rules: the deal, move legality, and undo.
//!
//! Rules implemented here:
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

use crate::card::{Card, KING, shuffled_deck};

pub const COLUMNS: usize = 7;
const COLUMN_DEAL: usize = 7;
const FACE_DOWN_COLUMNS: usize = 4;
const FACE_DOWN_PER_COLUMN: usize = 3;
const RUN_LEN: usize = 13;

/// A tableau column, drawn top-to-bottom; the last card is the exposed one.
///
/// Face-down cards are always a prefix of `cards` (cards are only ever added
/// face up), so a single count describes them.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Column {
    cards: Vec<Card>,
    face_down: usize,
}

impl Column {
    pub fn cards(&self) -> &[Card] {
        &self.cards
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    pub fn is_face_up(&self, index: usize) -> bool {
        index >= self.face_down && index < self.cards.len()
    }

    /// The exposed (bottom) card, which is the only legal drop target.
    pub fn exposed(&self) -> Option<Card> {
        self.cards.last().copied()
    }

    /// Index of the first face-up card, or `None` if the column has none.
    pub fn first_face_up(&self) -> Option<usize> {
        (self.face_down < self.cards.len()).then_some(self.face_down)
    }

    /// A finished suit: 13 cards running K down to A, all face up.
    pub fn is_complete_run(&self) -> bool {
        if self.cards.len() != RUN_LEN || self.face_down != 0 {
            return false;
        }
        let suit = self.cards[0].suit;
        self.cards
            .iter()
            .enumerate()
            .all(|(i, card)| card.suit == suit && card.rank as usize == RUN_LEN - i)
    }
}

/// A completed change, kept so it can be undone or replayed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Move {
        from: usize,
        to: usize,
        count: usize,
        /// Whether the move flipped a face-down card in `from`.
        flipped: bool,
    },
    DealStock,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MoveError {
    SameColumn,
    NoSuchCard,
    FaceDown,
    NotKing,
    Mismatch,
}

impl MoveError {
    pub const fn message(self) -> &'static str {
        match self {
            Self::SameColumn => "A card cannot move onto its own column.",
            Self::NoSuchCard => "There is no card there.",
            Self::FaceDown => "That card is face down.",
            Self::NotKing => "Only a King can move to an empty column.",
            Self::Mismatch => "Cards must land on the same suit, one rank higher.",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Game {
    columns: [Column; COLUMNS],
    stock: Vec<Card>,
    moves: u32,
    seed: u64,
}

impl Game {
    /// Deal a new game from `seed`.
    pub fn new(seed: u64) -> Self {
        let mut deck = shuffled_deck(seed).into_iter();
        let mut columns: [Column; COLUMNS] = Default::default();

        for (i, column) in columns.iter_mut().enumerate() {
            column.cards.extend(deck.by_ref().take(COLUMN_DEAL));
            column.face_down = if i < FACE_DOWN_COLUMNS {
                FACE_DOWN_PER_COLUMN
            } else {
                0
            };
        }

        Self {
            columns,
            stock: deck.collect(),
            moves: 0,
            seed,
        }
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn column(&self, index: usize) -> &Column {
        &self.columns[index]
    }

    pub fn stock_len(&self) -> usize {
        self.stock.len()
    }

    pub fn moves(&self) -> u32 {
        self.moves
    }

    /// Whether the tail starting at `from[index]` may be dropped on `to`.
    pub fn check_move(&self, from: usize, index: usize, to: usize) -> Result<(), MoveError> {
        if from == to {
            return Err(MoveError::SameColumn);
        }
        let source = &self.columns[from];
        if index >= source.len() {
            return Err(MoveError::NoSuchCard);
        }
        if !source.is_face_up(index) {
            return Err(MoveError::FaceDown);
        }

        let head = source.cards[index];
        match self.columns[to].exposed() {
            // A King's rank + 1 is 14, so this arm also blocks Kings from
            // landing on cards, which is what the rules require.
            Some(target) => {
                if target.suit == head.suit && target.rank == head.rank + 1 {
                    Ok(())
                } else {
                    Err(MoveError::Mismatch)
                }
            }
            None => {
                if head.rank == KING {
                    Ok(())
                } else {
                    Err(MoveError::NotKing)
                }
            }
        }
    }

    /// Move the tail at `from[index]` onto `to`, returning the undoable action.
    pub fn apply_move(
        &mut self,
        from: usize,
        index: usize,
        to: usize,
    ) -> Result<Action, MoveError> {
        self.check_move(from, index, to)?;

        let tail = self.columns[from].cards.split_off(index);
        let count = tail.len();
        self.columns[to].cards.extend(tail);

        let source = &mut self.columns[from];
        let flipped = !source.cards.is_empty() && source.face_down == source.cards.len();
        if flipped {
            source.face_down -= 1;
        }

        self.moves += 1;
        Ok(Action::Move {
            from,
            to,
            count,
            flipped,
        })
    }

    /// Deal the three stock cards, one onto each of the first three columns.
    pub fn deal_stock(&mut self) -> Option<Action> {
        if self.stock.is_empty() {
            return None;
        }
        for column in self.columns.iter_mut().take(3) {
            if let Some(card) = self.stock.pop() {
                column.cards.push(card);
            }
        }
        self.moves += 1;
        Some(Action::DealStock)
    }

    /// Reverse `action`, which must be the most recent change to this game.
    pub fn undo(&mut self, action: Action) {
        match action {
            Action::Move {
                from,
                to,
                count,
                flipped,
            } => {
                if flipped {
                    self.columns[from].face_down += 1;
                }
                let split = self.columns[to].len() - count;
                let tail = self.columns[to].cards.split_off(split);
                self.columns[from].cards.extend(tail);
            }
            // Dealt onto columns 0, 1, 2 in order, so collect them back in
            // reverse to restore the stock's original order.
            Action::DealStock => {
                for column in self.columns.iter_mut().take(3).rev() {
                    if let Some(card) = column.cards.pop() {
                        self.stock.push(card);
                    }
                }
            }
        }
        self.moves = self.moves.saturating_sub(1);
    }

    /// Replay an undone action. The board must be in the state it was in
    /// before the action originally happened.
    pub fn redo(&mut self, action: Action) {
        match action {
            Action::Move {
                from, to, count, ..
            } => {
                let index = self.columns[from].len().saturating_sub(count);
                let _ = self.apply_move(from, index, to);
            }
            Action::DealStock => {
                self.deal_stock();
            }
        }
    }

    pub fn is_won(&self) -> bool {
        self.stock.is_empty()
            && self
                .columns
                .iter()
                .all(|column| column.is_empty() || column.is_complete_run())
    }

    /// Whether any move that changes the position remains.
    pub fn has_progress_move(&self) -> bool {
        !self.stock.is_empty() || !self.suggestions().is_empty()
    }

    /// Every legal move that changes the position, most useful first.
    ///
    /// The ranking is a heuristic, not a solver: Scorpion cannot be played
    /// perfectly without searching, so these are reasonable moves rather than
    /// guaranteed-winning ones. Sliding a whole column into an empty one is
    /// legal but accomplishes nothing, so it is left out entirely.
    pub fn suggestions(&self) -> Vec<Suggestion> {
        let mut found = Vec::new();
        for from in 0..COLUMNS {
            let source = &self.columns[from];
            let Some(first) = source.first_face_up() else {
                continue;
            };
            for index in first..source.len() {
                for to in 0..COLUMNS {
                    if index == 0 && self.columns[to].is_empty() {
                        continue;
                    }
                    if self.check_move(from, index, to).is_ok() {
                        found.push(Suggestion { from, index, to });
                    }
                }
            }
        }
        // Stable sort over an already deterministic list, so repeated calls in
        // the same position always offer hints in the same order.
        found.sort_by_key(|suggestion| -self.value_of(*suggestion));
        found
    }

    /// How much a move is worth, in the rough order a player would think of.
    fn value_of(&self, suggestion: Suggestion) -> i32 {
        let Suggestion { from, index, to } = suggestion;
        let source = &self.columns[from];
        let mut value = 0;

        // Turning a card face up is the main way a Scorpion deal opens up.
        if index > 0 && index == source.face_down {
            value += 100;
        }
        // Emptying a column creates the only place a King can go.
        if index == 0 {
            value += 50;
        }
        // Filling an empty column spends that scarce space.
        if self.columns[to].is_empty() {
            value -= 25;
        }
        // All else equal, prefer disturbing fewer cards.
        value - (source.len() - index) as i32
    }
}

/// A legal move worth considering, as `from[index]` onto column `to`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Suggestion {
    pub from: usize,
    pub index: usize,
    pub to: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Suit::{Clubs, Diamonds, Hearts, Spades};
    use crate::card::{ACE, Suit};
    use std::collections::HashSet;

    fn c(rank: u8, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    fn col(face_down: usize, cards: &[Card]) -> Column {
        Column {
            cards: cards.to_vec(),
            face_down,
        }
    }

    /// A game with a hand-built board, for testing specific positions.
    fn board(columns: [Column; COLUMNS], stock: &[Card]) -> Game {
        Game {
            columns,
            stock: stock.to_vec(),
            moves: 0,
            seed: 0,
        }
    }

    fn empty_board() -> [Column; COLUMNS] {
        Default::default()
    }

    /// Every legal `(from, index, to)` in the current position.
    fn legal_moves(game: &Game) -> Vec<(usize, usize, usize)> {
        let mut moves = Vec::new();
        for from in 0..COLUMNS {
            let source = game.column(from);
            let Some(first) = source.first_face_up() else {
                continue;
            };
            for index in first..source.len() {
                for to in 0..COLUMNS {
                    if game.check_move(from, index, to).is_ok() {
                        moves.push((from, index, to));
                    }
                }
            }
        }
        moves
    }

    #[test]
    fn deal_has_the_right_shape() {
        let game = Game::new(42);

        assert_eq!(game.stock_len(), 3);
        let mut seen = HashSet::new();
        for i in 0..COLUMNS {
            let column = game.column(i);
            assert_eq!(column.len(), 7);
            assert_eq!(
                column.first_face_up(),
                Some(if i < 4 { 3 } else { 0 }),
                "columns 1-4 start with three face-down cards"
            );
            seen.extend(column.cards().iter().copied());
        }
        seen.extend(game.stock.iter().copied());
        assert_eq!(seen.len(), 52, "all 52 cards dealt exactly once");
    }

    #[test]
    fn deal_is_seed_stable() {
        let cards = |game: &Game| -> Vec<Card> {
            (0..COLUMNS)
                .flat_map(|i| game.column(i).cards().to_vec())
                .collect()
        };
        assert_eq!(Game::new(99), Game::new(99));
        assert_ne!(cards(&Game::new(99)), cards(&Game::new(100)));
    }

    #[test]
    fn tail_lands_on_same_suit_one_rank_higher() {
        let mut columns = empty_board();
        columns[0] = col(0, &[c(9, Hearts)]);
        columns[1] = col(0, &[c(10, Hearts)]);
        let mut game = board(columns, &[]);

        assert_eq!(game.check_move(0, 0, 1), Ok(()));
        game.apply_move(0, 0, 1).unwrap();
        assert!(game.column(0).is_empty());
        assert_eq!(game.column(1).cards(), &[c(10, Hearts), c(9, Hearts)]);
    }

    #[test]
    fn tail_moves_regardless_of_its_own_order() {
        let mut columns = empty_board();
        // 9♥ with junk below it: the whole tail travels together.
        columns[0] = col(
            0,
            &[c(2, Clubs), c(9, Hearts), c(4, Spades), c(ACE, Diamonds)],
        );
        columns[1] = col(0, &[c(10, Hearts)]);
        let mut game = board(columns, &[]);

        game.apply_move(0, 1, 1).unwrap();
        assert_eq!(game.column(0).cards(), &[c(2, Clubs)]);
        assert_eq!(
            game.column(1).cards(),
            &[c(10, Hearts), c(9, Hearts), c(4, Spades), c(ACE, Diamonds)]
        );
    }

    #[test]
    fn illegal_drops_are_rejected() {
        let mut columns = empty_board();
        columns[0] = col(2, &[c(3, Clubs), c(4, Clubs), c(9, Hearts)]);
        columns[1] = col(0, &[c(10, Spades)]);
        columns[2] = col(0, &[c(8, Hearts)]);
        columns[3] = col(0, &[c(KING, Hearts)]);
        let game = board(columns, &[]);

        assert_eq!(game.check_move(0, 2, 0), Err(MoveError::SameColumn));
        assert_eq!(game.check_move(0, 1, 1), Err(MoveError::FaceDown));
        assert_eq!(game.check_move(0, 9, 1), Err(MoveError::NoSuchCard));
        assert_eq!(
            game.check_move(0, 2, 1),
            Err(MoveError::Mismatch),
            "wrong suit"
        );
        assert_eq!(
            game.check_move(0, 2, 2),
            Err(MoveError::Mismatch),
            "wrong rank"
        );
        assert_eq!(
            game.check_move(3, 0, 2),
            Err(MoveError::Mismatch),
            "a King never lands on a card"
        );
    }

    #[test]
    fn only_a_king_may_take_an_empty_column() {
        let mut columns = empty_board();
        columns[0] = col(0, &[c(KING, Spades), c(4, Clubs)]);
        columns[1] = col(0, &[c(2, Diamonds)]);
        let game = board(columns, &[]);

        assert_eq!(game.check_move(0, 0, 6), Ok(()), "King-headed tail");
        assert_eq!(game.check_move(0, 1, 6), Err(MoveError::NotKing));
        assert_eq!(game.check_move(1, 0, 6), Err(MoveError::NotKing));
    }

    #[test]
    fn move_flips_the_newly_exposed_card() {
        let mut columns = empty_board();
        columns[0] = col(2, &[c(3, Clubs), c(4, Clubs), c(9, Hearts)]);
        columns[1] = col(0, &[c(10, Hearts)]);
        let mut game = board(columns, &[]);

        let action = game.apply_move(0, 2, 1).unwrap();
        assert_eq!(game.column(0).face_down, 1, "4♣ turned face up");
        assert!(matches!(action, Action::Move { flipped: true, .. }));

        game.undo(action);
        assert_eq!(game.column(0).face_down, 2, "undo hides it again");
        assert_eq!(game.column(0).len(), 3);
    }

    #[test]
    fn emptying_a_column_flips_nothing() {
        let mut columns = empty_board();
        columns[0] = col(0, &[c(9, Hearts)]);
        columns[1] = col(0, &[c(10, Hearts)]);
        let mut game = board(columns, &[]);

        let action = game.apply_move(0, 0, 1).unwrap();
        assert!(matches!(action, Action::Move { flipped: false, .. }));
        assert_eq!(game.column(0).face_down, 0);
    }

    #[test]
    fn stock_deals_three_cards_and_undoes_cleanly() {
        let start = Game::new(5);
        let mut game = start.clone();

        let action = game.deal_stock().expect("stock is not empty");
        assert_eq!(game.stock_len(), 0);
        for i in 0..3 {
            assert_eq!(game.column(i).len(), 8);
        }
        assert!(game.deal_stock().is_none(), "the stock deals only once");

        game.undo(action);
        assert_eq!(game, start, "stock order and columns fully restored");
    }

    #[test]
    fn undo_round_trips_a_played_game() {
        let start = Game::new(2024);
        let mut game = start.clone();
        let mut history = Vec::new();

        for step in 0..300 {
            let moves = legal_moves(&game);
            if moves.is_empty() {
                match game.deal_stock() {
                    Some(action) => history.push(action),
                    None => break,
                }
                continue;
            }
            // Deterministic but varied choice, so the walk explores flips,
            // empties and multi-card tails.
            let (from, index, to) = moves[(step * 7 + 3) % moves.len()];
            history.push(game.apply_move(from, index, to).unwrap());
        }

        assert!(history.len() > 20, "played {} moves", history.len());
        while let Some(action) = history.pop() {
            game.undo(action);
        }
        assert_eq!(game, start);
    }

    #[test]
    fn redo_replays_a_move_exactly() {
        let mut game = Game::new(11);
        let moves = legal_moves(&game);
        let (from, index, to) = moves[0];

        let action = game.apply_move(from, index, to).unwrap();
        let after = game.clone();

        game.undo(action);
        game.redo(action);
        assert_eq!(game, after);
    }

    #[test]
    fn a_board_of_four_runs_is_a_win() {
        let run = |suit| col(0, &(0..13).map(|i| c(13 - i, suit)).collect::<Vec<_>>());
        let mut columns = empty_board();
        for (i, suit) in Suit::ALL.into_iter().enumerate() {
            columns[i] = run(suit);
        }
        let game = board(columns.clone(), &[]);
        assert!(game.is_won());

        // Same cards, but two runs spliced out of order.
        let mut broken = columns;
        broken[0].cards.swap(3, 4);
        assert!(!board(broken, &[]).is_won(), "order matters");

        // A complete board with cards still in the stock is not a win.
        let mut columns = empty_board();
        for (i, suit) in Suit::ALL.into_iter().enumerate() {
            columns[i] = run(suit);
        }
        assert!(!board(columns, &[c(ACE, Spades)]).is_won());
    }

    #[test]
    fn a_run_with_a_face_down_card_is_not_a_win() {
        let mut columns = empty_board();
        for (i, suit) in Suit::ALL.into_iter().enumerate() {
            columns[i] = col(
                usize::from(i == 0),
                &(0..13).map(|r| c(13 - r, suit)).collect::<Vec<_>>(),
            );
        }
        assert!(!board(columns, &[]).is_won());
    }

    #[test]
    fn dead_positions_report_no_progress() {
        let mut columns = empty_board();
        columns[0] = col(0, &[c(2, Spades)]);
        columns[1] = col(0, &[c(7, Hearts)]);
        assert!(!board(columns, &[]).has_progress_move());

        let mut columns = empty_board();
        columns[0] = col(0, &[c(2, Spades)]);
        columns[1] = col(0, &[c(3, Spades)]);
        assert!(board(columns, &[]).has_progress_move());

        // A lone King with an empty column: legal, but it changes nothing.
        let mut columns = empty_board();
        columns[0] = col(0, &[c(KING, Spades)]);
        assert!(!board(columns, &[]).has_progress_move());

        // Stock still to come always counts as progress.
        let mut columns = empty_board();
        columns[0] = col(0, &[c(2, Spades)]);
        assert!(board(columns, &[c(ACE, Hearts)]).has_progress_move());
    }

    #[test]
    fn every_suggestion_is_a_legal_move_that_changes_something() {
        let game = Game::new(42);
        let suggestions = game.suggestions();
        assert!(!suggestions.is_empty(), "the opening deal has moves");

        for suggestion in &suggestions {
            let Suggestion { from, index, to } = *suggestion;
            assert_eq!(game.check_move(from, index, to), Ok(()));
            assert!(
                !(index == 0 && game.column(to).is_empty()),
                "shuffling a whole column into an empty one is not a move"
            );
        }
    }

    #[test]
    fn turning_a_card_over_is_suggested_ahead_of_a_move_that_does_not() {
        let mut columns = empty_board();
        // Lifting the 9♥ turns the 4♣ underneath it face up.
        columns[0] = col(2, &[c(3, Clubs), c(4, Clubs), c(9, Hearts)]);
        columns[1] = col(0, &[c(10, Hearts)]);
        // Whereas this one just shuffles two face-up cards around.
        columns[2] = col(0, &[c(5, Spades), c(2, Diamonds)]);
        columns[3] = col(0, &[c(3, Diamonds)]);
        let game = board(columns, &[]);

        let suggestions = game.suggestions();
        assert_eq!(
            suggestions.first(),
            Some(&Suggestion {
                from: 0,
                index: 2,
                to: 1
            }),
            "got {suggestions:?}"
        );
        assert_eq!(suggestions.len(), 2, "the other move is still offered");
    }

    #[test]
    fn suggestions_are_stable_for_the_same_position() {
        let game = Game::new(7);
        assert_eq!(game.suggestions(), game.suggestions());
    }

    #[test]
    fn a_dead_position_suggests_nothing() {
        let mut columns = empty_board();
        columns[0] = col(0, &[c(2, Spades)]);
        columns[1] = col(0, &[c(7, Hearts)]);
        assert!(board(columns, &[]).suggestions().is_empty());
    }
}
