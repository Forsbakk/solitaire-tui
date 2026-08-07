//! Piles, the board they sit on, and the undoable actions that change it.
//!
//! Nothing here knows any game's rules. A board is a flat `Vec<Pile>`, an index
//! into it is a [`PileId`], and every move is addressed as a source pile, a card
//! inside it, and a destination pile. What is *legal* lives in `crate::rules`.
//!
//! An [`Action`] is a list of [`Step`]s applied in order and undone in reverse.
//! One player action can be several steps — Scorpion's stock deal puts a card on
//! each of three piles — which is what lets a single undo put the board back
//! exactly as it was.

use crate::card::{Card, KING};

pub type PileId = usize;

/// Cards in a finished suit, King down to Ace.
const RUN_LEN: usize = 13;

/// What a pile is for. Rules read this to decide what may be done with it, and
/// the layout reads it to decide how to draw it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PileKind {
    /// The main playing area, built downwards.
    Tableau,
    /// Where finished suits are collected.
    Foundation,
    /// Undealt cards.
    Stock,
    /// Cards turned from the stock and available to play.
    Waste,
}

/// A stack of cards, drawn top-to-bottom; the last card is the exposed one.
///
/// Face-down cards are always a prefix of `cards`, so a single count describes
/// them. Every move that adds cards either appends them face up or appends them
/// to a pile that is already entirely face down, which is what keeps the prefix
/// contiguous.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Pile {
    kind: PileKind,
    cards: Vec<Card>,
    face_down: usize,
}

impl Pile {
    pub const fn new(kind: PileKind) -> Self {
        Self {
            kind,
            cards: Vec::new(),
            face_down: 0,
        }
    }

    /// A pile holding `cards`, the first `face_down` of them face down.
    pub fn holding(kind: PileKind, face_down: usize, cards: &[Card]) -> Self {
        debug_assert!(face_down <= cards.len());
        Self {
            kind,
            cards: cards.to_vec(),
            face_down,
        }
    }

    /// A tableau pile. The common case, and what most tests build.
    pub fn tableau(face_down: usize, cards: &[Card]) -> Self {
        Self::holding(PileKind::Tableau, face_down, cards)
    }

    pub const fn kind(&self) -> PileKind {
        self.kind
    }

    pub fn cards(&self) -> &[Card] {
        &self.cards
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    pub const fn face_down(&self) -> usize {
        self.face_down
    }

    pub fn is_face_up(&self, index: usize) -> bool {
        index >= self.face_down && index < self.cards.len()
    }

    /// The exposed (bottom) card, which is what other cards land on.
    pub fn exposed(&self) -> Option<Card> {
        self.cards.last().copied()
    }

    /// Index of the first face-up card, or `None` if the pile has none.
    pub fn first_face_up(&self) -> Option<usize> {
        (self.face_down < self.cards.len()).then_some(self.face_down)
    }

    /// A finished suit: 13 cards running King down to Ace, all face up.
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

    /// A finished foundation: 13 cards running Ace up to King in one suit.
    pub fn is_complete_suit(&self) -> bool {
        self.cards.len() == RUN_LEN
            && self.cards.last().is_some_and(|card| card.rank == KING)
            && self
                .cards
                .iter()
                .enumerate()
                .all(|(i, card)| card.suit == self.cards[0].suit && card.rank as usize == i + 1)
    }
}

/// A card being moved: the card at `from[index]`, and everything below it, onto
/// `to`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Move {
    pub from: PileId,
    pub index: usize,
    pub to: PileId,
}

impl Move {
    pub const fn new(from: PileId, index: usize, to: PileId) -> Self {
        Self { from, index, to }
    }
}

/// One pile-to-pile transfer of cards.
///
/// The flags record what the transfer did to the cards' orientation and order,
/// so that undoing it needs no knowledge of the rules that produced it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Step {
    pub from: PileId,
    pub to: PileId,
    pub count: usize,
    /// The cards arrive in the opposite order — the waste recycled into the
    /// stock, which is what makes the next pass through it repeat.
    pub reverse: bool,
    /// Whether the moved cards were face up where they came from.
    pub was_face_up: bool,
    /// Whether they are face up where they land.
    pub face_up: bool,
    /// Whether taking them turned the source's new last card face up.
    pub flip_source: bool,
    /// Whether this counts against a game's redeal allowance. Kept on the step
    /// so that undoing a recycle hands the redeal back.
    pub recycle: bool,
}

/// A completed change to the board, kept so it can be undone or replayed.
///
/// Steps are applied in order and undone in reverse, so an action is only
/// reversible against the board state it originally left behind.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Action {
    pub steps: Vec<Step>,
}

impl Action {
    pub const fn new(steps: Vec<Step>) -> Self {
        Self { steps }
    }

    pub fn single(step: Step) -> Self {
        Self { steps: vec![step] }
    }

    /// Lift `from[index]` and everything below it onto `to`, working out from
    /// the board whether the source will flip a card face up.
    ///
    /// This is the move nearly every game makes, so the flip rule lives here
    /// rather than being restated in each rules module.
    pub fn moving(board: &Board, from: PileId, index: usize, to: PileId) -> Self {
        let source = board.pile(from);
        Self::single(Step {
            from,
            to,
            count: source.len() - index,
            reverse: false,
            was_face_up: source.is_face_up(index),
            face_up: true,
            // The cards left behind are `0..index`. If every one of them is
            // face down, the new last card is exposed and turns over.
            flip_source: index > 0 && source.face_down() == index,
            recycle: false,
        })
    }
}

/// Every pile in play, plus the bookkeeping shown in the status bar.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Board {
    piles: Vec<Pile>,
    moves: u32,
    recycles: u32,
    seed: u64,
}

impl Board {
    pub const fn new(seed: u64, piles: Vec<Pile>) -> Self {
        Self {
            piles,
            moves: 0,
            recycles: 0,
            seed,
        }
    }

    pub fn pile(&self, id: PileId) -> &Pile {
        &self.piles[id]
    }

    pub fn piles(&self) -> &[Pile] {
        &self.piles
    }

    pub const fn seed(&self) -> u64 {
        self.seed
    }

    pub const fn moves(&self) -> u32 {
        self.moves
    }

    /// How many times a pile has been recycled — how many redeals are spent.
    pub const fn recycles(&self) -> u32 {
        self.recycles
    }

    /// Every pile of `kind`, in board order.
    pub fn of_kind(&self, kind: PileKind) -> impl Iterator<Item = PileId> + '_ {
        self.piles
            .iter()
            .enumerate()
            .filter(move |(_, pile)| pile.kind == kind)
            .map(|(id, _)| id)
    }

    pub fn first_of_kind(&self, kind: PileKind) -> Option<PileId> {
        self.of_kind(kind).next()
    }

    /// How many cards are still face down anywhere on the board.
    pub fn face_down_count(&self) -> usize {
        self.piles.iter().map(Pile::face_down).sum()
    }

    pub fn apply(&mut self, action: &Action) {
        for step in &action.steps {
            self.apply_step(step);
        }
        self.moves += 1;
    }

    /// Reverse `action`, which must be the most recent change to this board.
    pub fn undo(&mut self, action: &Action) {
        for step in action.steps.iter().rev() {
            self.undo_step(step);
        }
        self.moves = self.moves.saturating_sub(1);
    }

    /// Replay an undone action. The board must be in the state it was in
    /// before the action originally happened.
    pub fn redo(&mut self, action: &Action) {
        self.apply(action);
    }

    fn apply_step(&mut self, step: &Step) {
        let source = &mut self.piles[step.from];
        let split = source.cards.len() - step.count;
        let mut moved = source.cards.split_off(split);
        // Taking cards off the end can leave the count pointing past the end,
        // which happens whenever the source was entirely face down.
        source.face_down = source.face_down.min(source.cards.len());
        if step.flip_source {
            source.face_down -= 1;
        }

        if step.reverse {
            moved.reverse();
        }
        if step.recycle {
            self.recycles += 1;
        }

        let target = &mut self.piles[step.to];
        if !step.face_up {
            debug_assert_eq!(
                target.face_down,
                target.cards.len(),
                "face-down cards may only land on an entirely face-down pile",
            );
            target.face_down += step.count;
        }
        target.cards.extend(moved);
    }

    fn undo_step(&mut self, step: &Step) {
        let target = &mut self.piles[step.to];
        let split = target.cards.len() - step.count;
        let mut moved = target.cards.split_off(split);
        if !step.face_up {
            target.face_down -= step.count;
        }

        if step.reverse {
            moved.reverse();
        }
        if step.recycle {
            self.recycles -= 1;
        }

        let source = &mut self.piles[step.from];
        if step.flip_source {
            source.face_down += 1;
        }
        source.cards.extend(moved);
        if !step.was_face_up {
            source.face_down += step.count;
        }
    }
}

/// How a pile shows the cards it holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fan {
    /// Only the exposed card is drawn — stock, foundations.
    None,
    /// Cards overlap downwards, so every one of them can be seen and clicked.
    Down,
    /// The last `n` cards overlap rightwards. Only the exposed one is in play;
    /// the rest are shown so the player can see what a draw turned up.
    Right(usize),
}

/// Where a pile sits: which row of the board, and which slot along it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Spot {
    pub pile: PileId,
    pub row: u8,
    pub slot: u16,
    pub fan: Fan,
}

impl Spot {
    pub const fn new(pile: PileId, row: u8, slot: u16, fan: Fan) -> Self {
        Self {
            pile,
            row,
            slot,
            fan,
        }
    }
}

/// A game's board shape, declared once and read by the layout.
///
/// Piles with no spot are not drawn — Scorpion's stock is only ever a count in
/// the status bar, so it has cards but no place on the board.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BoardSpec {
    pub spots: Vec<Spot>,
}

impl BoardSpec {
    pub const fn new(spots: Vec<Spot>) -> Self {
        Self { spots }
    }

    pub fn rows(&self) -> u8 {
        self.spots
            .iter()
            .map(|spot| spot.row)
            .max()
            .map_or(0, |r| r + 1)
    }

    /// The spots in `row`, left to right.
    pub fn row(&self, row: u8) -> impl Iterator<Item = &Spot> + '_ {
        self.spots.iter().filter(move |spot| spot.row == row)
    }

    pub fn spot(&self, pile: PileId) -> Option<&Spot> {
        self.spots.iter().find(|spot| spot.pile == pile)
    }
}

/// Why a move was refused. The wording is deliberately game-neutral, since
/// every variant raises the same handful of complaints.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MoveError {
    SamePile,
    NoSuchCard,
    FaceDown,
    NotKing,
    Mismatch,
    /// The lifted cards are not an ordered run, so they cannot travel together.
    NotASequence,
    /// The destination does not accept cards this way at all.
    WrongTarget,
    /// Only the exposed card of that pile may be taken.
    NotExposed,
}

impl MoveError {
    pub const fn message(self) -> &'static str {
        match self {
            Self::SamePile => "A card cannot move onto its own pile.",
            Self::NoSuchCard => "There is no card there.",
            Self::FaceDown => "That card is face down.",
            Self::NotKing => "Only a King can move to an empty pile.",
            Self::Mismatch => "Those cards do not go together.",
            Self::NotASequence => "Only an ordered run can move as a unit.",
            Self::WrongTarget => "Cards cannot be moved there.",
            Self::NotExposed => "Only the top card of that pile can be taken.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Suit::{Clubs, Diamonds, Hearts, Spades};
    use crate::card::{ACE, Suit};

    fn c(rank: u8, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    fn board(piles: Vec<Pile>) -> Board {
        Board::new(0, piles)
    }

    #[test]
    fn a_move_flips_the_card_it_uncovers_and_undo_hides_it_again() {
        let mut board = board(vec![
            Pile::tableau(2, &[c(3, Clubs), c(4, Clubs), c(9, Hearts)]),
            Pile::tableau(0, &[c(10, Hearts)]),
        ]);
        let start = board.clone();

        let action = Action::moving(&board, 0, 2, 1);
        board.apply(&action);
        assert_eq!(board.pile(0).face_down(), 1, "4♣ turned face up");
        assert_eq!(board.pile(1).len(), 2);

        board.undo(&action);
        assert_eq!(board, start);
    }

    #[test]
    fn emptying_a_pile_flips_nothing() {
        let board = board(vec![
            Pile::tableau(0, &[c(9, Hearts)]),
            Pile::tableau(0, &[c(10, Hearts)]),
        ]);
        let action = Action::moving(&board, 0, 0, 1);
        assert!(!action.steps[0].flip_source);
    }

    #[test]
    fn face_down_cards_turn_over_on_the_way_to_the_waste_and_back() {
        let stock = Pile::holding(PileKind::Stock, 3, &[c(2, Clubs), c(3, Clubs), c(4, Clubs)]);
        let mut board = board(vec![stock, Pile::new(PileKind::Waste)]);
        let start = board.clone();

        let draw = Action::single(Step {
            from: 0,
            to: 1,
            count: 2,
            reverse: false,
            was_face_up: false,
            face_up: true,
            flip_source: false,
            recycle: false,
        });
        board.apply(&draw);

        assert_eq!(board.pile(0).len(), 1);
        assert_eq!(board.pile(0).face_down(), 1, "what is left stays hidden");
        assert_eq!(board.pile(1).face_down(), 0, "the waste is face up");
        assert_eq!(board.pile(1).exposed(), Some(c(4, Clubs)));

        board.undo(&draw);
        assert_eq!(board, start);
    }

    #[test]
    fn recycling_the_waste_reverses_it_so_the_next_pass_repeats() {
        let cards = [c(2, Clubs), c(3, Clubs), c(4, Clubs)];
        let mut board = board(vec![
            Pile::new(PileKind::Stock),
            Pile::holding(PileKind::Waste, 0, &cards),
        ]);
        let start = board.clone();

        let recycle = Action::single(Step {
            from: 1,
            to: 0,
            count: 3,
            reverse: true,
            was_face_up: true,
            face_up: false,
            flip_source: false,
            recycle: false,
        });
        board.apply(&recycle);

        assert!(board.pile(1).is_empty());
        assert_eq!(board.pile(0).face_down(), 3, "back to face down");
        assert_eq!(
            board.pile(0).cards(),
            &[c(4, Clubs), c(3, Clubs), c(2, Clubs)],
            "reversed, so drawing from the end deals 2♣ 3♣ 4♣ again"
        );

        board.undo(&recycle);
        assert_eq!(board, start);
    }

    #[test]
    fn a_multi_step_action_undoes_as_one() {
        let mut board = board(vec![
            Pile::holding(PileKind::Stock, 3, &[c(2, Clubs), c(3, Clubs), c(4, Clubs)]),
            Pile::tableau(0, &[c(9, Hearts)]),
            Pile::tableau(0, &[c(10, Hearts)]),
            Pile::tableau(0, &[c(ACE, Spades)]),
        ]);
        let start = board.clone();

        let deal = Action::new(
            (1..=3)
                .map(|to| Step {
                    from: 0,
                    to,
                    count: 1,
                    reverse: false,
                    was_face_up: false,
                    face_up: true,
                    flip_source: false,
                    recycle: false,
                })
                .collect(),
        );
        board.apply(&deal);
        for to in 1..=3 {
            assert_eq!(board.pile(to).len(), 2);
        }

        board.undo(&deal);
        assert_eq!(board, start, "including the stock's original order");
    }

    #[test]
    fn a_complete_run_needs_the_right_order_and_no_hidden_cards() {
        let run: Vec<Card> = (0..13).map(|i| c(13 - i, Spades)).collect();
        assert!(Pile::tableau(0, &run).is_complete_run());
        assert!(!Pile::tableau(1, &run).is_complete_run());

        let mut scrambled = run.clone();
        scrambled.swap(3, 4);
        assert!(!Pile::tableau(0, &scrambled).is_complete_run());
    }

    #[test]
    fn a_complete_suit_runs_ace_up_in_one_suit() {
        let suit: Vec<Card> = (ACE..=KING).map(|rank| c(rank, Diamonds)).collect();
        assert!(Pile::holding(PileKind::Foundation, 0, &suit).is_complete_suit());
        assert!(!Pile::holding(PileKind::Foundation, 0, &suit[..12]).is_complete_suit());

        let mut wrong = suit.clone();
        wrong[5] = c(6, Hearts);
        assert!(!Pile::holding(PileKind::Foundation, 0, &wrong).is_complete_suit());
    }

    #[test]
    fn piles_can_be_found_by_kind() {
        let board = board(vec![
            Pile::new(PileKind::Stock),
            Pile::new(PileKind::Tableau),
            Pile::new(PileKind::Foundation),
            Pile::new(PileKind::Tableau),
        ]);
        assert_eq!(board.of_kind(PileKind::Tableau).collect::<Vec<_>>(), [1, 3]);
        assert_eq!(board.first_of_kind(PileKind::Stock), Some(0));
        assert_eq!(board.first_of_kind(PileKind::Waste), None);
    }
}
