//! FreeCell — eight columns, four free cells, and nothing hidden.
//!
//! * 52 cards dealt face up into 8 columns: four of seven, four of six. There
//!   is no stock and no card is ever face down, so a seed names a puzzle rather
//!   than a gamble.
//! * The tableau builds **down in alternating colour**, and **any** card may
//!   take an empty column — the one place this game is kinder than Klondike.
//! * Four **free cells** hold one card each, always in play.
//! * A run travels as a unit only if it is already in order *and* there is room
//!   to shuffle it across: `(free cells + 1) × (empty columns + 1)` cards, not
//!   counting the column it is landing on. Anything the limit refuses can still
//!   be played a card at a time.
//! * Four foundations build up in suit from the Ace. A card may be taken back
//!   off one.
//! * You win when all 52 cards are home.
//!
//! Baker's Game is this game built in suit, so everything here but the
//! [`Build`](super::Build) predicate is `pub(super)`.

use crate::board::{Action, Board, BoardSpec, Fan, Move, MoveError, Pile, PileId, PileKind, Spot};
use crate::card::{ACE, Card, shuffled_deck};
use crate::rules::{Build, Rules, StatusItem, StatusTone};

const TABLEAU: usize = 8;
const CELLS: usize = 4;
const FOUNDATIONS: usize = 4;
/// The first free cell; the rest follow it.
const CELL: PileId = TABLEAU;
/// The first foundation; the rest follow it.
const FOUNDATION: PileId = CELL + CELLS;
/// Cards in a deck, and so what a full set of foundations holds.
const DECK: usize = 52;
/// 52 over eight columns is six each, with four left over.
const PER_COLUMN: usize = DECK / TABLEAU;
const LONG_COLUMNS: usize = DECK % TABLEAU;

// -- shared with a future Baker's Game -------------------------------------
//
// Only `check` cares which game it is; the deal, the board, the cells, the
// foundations and the hint heuristic are the same either way.

pub(super) fn cells() -> impl Iterator<Item = PileId> {
    CELL..CELL + CELLS
}

pub(super) fn foundations() -> impl Iterator<Item = PileId> {
    FOUNDATION..FOUNDATION + FOUNDATIONS
}

/// Whether `card` may be added to foundation `pile`.
fn foundation_takes(board: &Board, pile: PileId, card: Card) -> bool {
    match board.pile(pile).exposed() {
        Some(top) => top.suit == card.suit && card.rank == top.rank + 1,
        None => card.rank == ACE,
    }
}

/// The foundation `card` belongs on, if any will have it.
fn home_for(board: &Board, card: Card) -> Option<PileId> {
    foundations().find(|&pile| foundation_takes(board, pile, card))
}

/// Cells are interchangeable, and so are empty columns, so a hint offers only
/// the first of each rather than four spellings of one move.
fn first_empty_cell(board: &Board) -> Option<PileId> {
    cells().find(|&pile| board.pile(pile).is_empty())
}

fn first_empty_column(board: &Board) -> Option<PileId> {
    (0..TABLEAU).find(|&pile| board.pile(pile).is_empty())
}

/// Whether `cards` are already an ordered run, and so travel as a unit.
fn is_run(build: Build, cards: &[Card]) -> bool {
    cards.windows(2).all(|pair| build.accepts(pair[0], pair[1]))
}

/// How many cards may travel together onto `to`.
///
/// A supermove is a shuttle: every card but one has to be parked somewhere
/// while the rest cross, so the limit is one more than the room to park them.
/// The target is left out of the count because a column being landed on is not
/// room — nothing can be parked where the run is about to go. That one
/// exclusion covers both cases: landing on a card, and landing on an empty
/// column, which spends one of its own multipliers.
///
/// The other convention, `(free + 1) × 2^empty`, permits more. This one never
/// permits a move the player could not also make by hand, and anything it
/// refuses can still be played a card at a time.
fn capacity(board: &Board, to: PileId) -> usize {
    let free = cells().filter(|&pile| board.pile(pile).is_empty()).count();
    let empty = (0..TABLEAU)
        .filter(|&pile| pile != to && board.pile(pile).is_empty())
        .count();
    (free + 1) * (empty + 1)
}

pub(super) fn deal(seed: u64) -> Board {
    let mut deck = shuffled_deck(seed).into_iter();
    let mut piles: Vec<Pile> = (0..TABLEAU)
        .map(|i| {
            // Four columns of seven, then four of six. Nothing is buried: that
            // is the whole point of the game.
            let total = PER_COLUMN + usize::from(i < LONG_COLUMNS);
            let cards: Vec<_> = deck.by_ref().take(total).collect();
            Pile::tableau(0, &cards)
        })
        .collect();

    piles.extend((0..CELLS).map(|_| Pile::new(PileKind::FreeCell)));
    piles.extend((0..FOUNDATIONS).map(|_| Pile::new(PileKind::Foundation)));
    Board::new(seed, piles)
}

pub(super) fn spec() -> BoardSpec {
    // Cells to the left, foundations to the right, flush over the eight
    // columns they serve. Eight piles into eight slots leaves none spare for a
    // gap between the halves, and taking a ninth slot would make the top row
    // wider than the tableau and hang the last foundation past the columns.
    // They are told apart by what is drawn in them instead: an empty cell says
    // `free`, an empty foundation says nothing.
    let mut spots: Vec<Spot> = cells()
        .enumerate()
        .map(|(i, pile)| Spot::new(pile, 0, i as u16, Fan::None))
        .collect();
    spots.extend(
        foundations()
            .enumerate()
            .map(|(i, pile)| Spot::new(pile, 0, (CELLS + i) as u16, Fan::None)),
    );
    spots.extend((0..TABLEAU).map(|i| Spot::new(i, 1, i as u16, Fan::Down)));
    BoardSpec::new(spots)
}

pub(super) fn check(build: Build, board: &Board, mv: Move) -> Result<(), MoveError> {
    let Move { from, index, to } = mv;
    if from == to {
        return Err(MoveError::SamePile);
    }
    let source = board.pile(from);
    let target = board.pile(to);
    if index >= source.len() {
        return Err(MoveError::NoSuchCard);
    }
    // Nothing here is ever face down, but asking keeps the order of complaints
    // the same as every other game's.
    if !source.is_face_up(index) {
        return Err(MoveError::FaceDown);
    }

    let last = source.len() - 1;
    // Only the card on top of a foundation is in play. A cell holds one card,
    // so `index` can only be `last` there in any case.
    if source.kind() == PileKind::Foundation && index != last {
        return Err(MoveError::NotExposed);
    }

    let head = source.cards()[index];
    match target.kind() {
        PileKind::FreeCell => {
            if index != last {
                // A cell takes one card, never a run.
                return Err(MoveError::NotExposed);
            }
            if target.is_empty() {
                Ok(())
            } else {
                Err(MoveError::Occupied)
            }
        }
        PileKind::Foundation => {
            if source.kind() == PileKind::Foundation {
                return Err(MoveError::WrongTarget);
            }
            if index != last {
                // Foundations take one card at a time, never a run.
                return Err(MoveError::NotExposed);
            }
            if foundation_takes(board, to, head) {
                Ok(())
            } else {
                Err(MoveError::Mismatch)
            }
        }
        PileKind::Tableau => {
            // Unlike Yukon, the lifted cards must already be in order.
            if !is_run(build, &source.cards()[index..]) {
                return Err(MoveError::NotASequence);
            }
            // Unlike every other game here, an empty column takes any card, so
            // there is no `NotKing` arm to write.
            if let Some(card) = target.exposed()
                && !build.accepts(card, head)
            {
                return Err(MoveError::Mismatch);
            }
            // Whether the run *fits* is asked last, so a run that could never
            // land there is refused for the reason it could never land there.
            if source.len() - index > capacity(board, to) {
                return Err(MoveError::TooMany);
            }
            Ok(())
        }
        // No FreeCell board has either, but `suggestions` tries every pair of
        // piles, and a rules module rejects pile *kinds* rather than trusting
        // which game it is in.
        PileKind::Stock | PileKind::Waste => Err(MoveError::WrongTarget),
    }
}

pub(super) fn is_won(board: &Board) -> bool {
    foundations().all(|pile| board.pile(pile).is_complete_suit())
}

pub(super) fn send_home(board: &Board, from: PileId) -> Option<Action> {
    let source = board.pile(from);
    // A cell's card is as much in play as a column's, so `f` has to work on
    // both — the app passes whichever pile the player is holding from.
    if !matches!(source.kind(), PileKind::Tableau | PileKind::FreeCell) {
        return None;
    }
    let card = source.exposed()?;
    let home = home_for(board, card)?;
    Some(Action::moving(board, from, source.len() - 1, home))
}

/// Whether the board can only be played out from here.
///
/// Every other game asks whether anything is still face down, which here is
/// never true and would call the opening deal decided. The question that
/// matters instead is whether every column runs down in rank: if it does, the
/// lowest card still out is on top of its column or sitting in a cell, and it
/// is playable, because every lower card of its suit must already be home.
/// Filing it leaves the columns descending, so the argument holds until the
/// board is empty.
fn is_decided(board: &Board) -> bool {
    (0..TABLEAU).all(|pile| {
        board
            .pile(pile)
            .cards()
            .windows(2)
            .all(|pair| pair[0].rank > pair[1].rank)
    })
}

pub(super) fn autoplay(board: &Board) -> Option<Action> {
    if is_won(board) || !is_decided(board) {
        return None;
    }
    // The cells are sources too: a finish that drained only the columns would
    // stop with cards stranded in them and the game not won.
    (0..TABLEAU)
        .chain(cells())
        .filter_map(|pile| {
            let card = board.pile(pile).exposed()?;
            Some((card.rank, pile, home_for(board, card)?))
        })
        .min()
        .map(|(_, pile, home)| Action::moving(board, pile, board.pile(pile).len() - 1, home))
}

pub(super) fn is_useful(board: &Board, mv: Move) -> bool {
    let Move { from, index, to } = mv;
    let source = board.pile(from);
    let target = board.pile(to);
    match target.kind() {
        // Always worth doing, and this arm has to come first: a lone Ace onto a
        // bare foundation is also "index 0 onto an empty pile", the very shape
        // the empty-column clause below throws away.
        PileKind::Foundation => true,
        // Parking a card is worth offering once, and only out of the tableau:
        // shuffling cards between cells changes nothing about the position.
        PileKind::FreeCell => {
            source.kind() == PileKind::Tableau && Some(to) == first_empty_cell(board)
        }
        PileKind::Tableau => {
            // Taking a card back off a foundation is legal but backwards.
            if source.kind() == PileKind::Foundation {
                return false;
            }
            if target.is_empty() {
                // Sliding a whole column into an empty one gains nothing, and
                // one empty column is as good as another.
                return !(index == 0 && source.kind() == PileKind::Tableau)
                    && Some(to) == first_empty_column(board);
            }
            true
        }
        PileKind::Stock | PileKind::Waste => false,
    }
}

pub(super) fn rank(board: &Board, mv: Move) -> i32 {
    let Move { from, index, to } = mv;
    let source = board.pile(from);
    let target = board.pile(to);
    let mut value = 0;

    if target.kind() == PileKind::Foundation {
        value += 60;
    }
    // Emptying a cell is what buys the next supermove its room.
    if source.kind() == PileKind::FreeCell {
        value += 40;
    }
    // Emptying a column is the biggest prize on a FreeCell board.
    if index == 0 && source.kind() == PileKind::Tableau {
        value += 30;
    }
    // Both of these spend the room the next supermove will want.
    if target.kind() == PileKind::FreeCell {
        value -= 20;
    }
    if target.kind() == PileKind::Tableau && target.is_empty() {
        value -= 25;
    }
    // All else equal, prefer disturbing fewer cards. There is deliberately no
    // clause for turning a card over: nothing here is face down.
    value - (source.len() - index) as i32
}

pub(super) fn status(board: &Board) -> Vec<StatusItem> {
    let home: usize = foundations().map(|pile| board.pile(pile).len()).sum();
    let free = cells().filter(|&pile| board.pile(pile).is_empty()).count();
    vec![
        StatusItem::new("home", format!("{home}/{DECK}"), StatusTone::Good),
        StatusItem::new(
            "free",
            format!("{free}/{CELLS}"),
            if free > 0 {
                StatusTone::Good
            } else {
                StatusTone::Muted
            },
        ),
    ]
}

/// The key FreeCell adds. There is no stock, so `d` is not among them, and `f`
/// is shared with every other game. How many cards may travel together is a
/// rule rather than a key, so it lives in the blurb.
pub(super) const HELP: &str = "  A             finish a board that is already in order";

#[derive(Default)]
pub struct FreeCell;

impl Rules for FreeCell {
    fn name(&self) -> &'static str {
        "FreeCell"
    }

    fn blurb(&self) -> &'static str {
        "Every card is face up from the start, so a deal is a puzzle rather \
         than a gamble and nearly all of them can be won. Build the eight \
         columns down in alternating colour, and put any card at all into an \
         empty column.\n\n\
         Four free cells hold one card each. A run in order travels as a unit \
         if there is room to shuffle it across — one card, doubled for every \
         free cell and every empty column — and whatever that refuses can \
         still be moved a card at a time.\n\n\
         The four foundations build up in suit from the Ace, and a card may be \
         taken back off one. Win by sending all 52 home."
    }

    fn help(&self) -> &'static str {
        HELP
    }

    fn deal(&self, seed: u64) -> Board {
        deal(seed)
    }

    fn spec(&self, _board: &Board) -> BoardSpec {
        spec()
    }

    fn check(&self, board: &Board, mv: Move) -> Result<(), MoveError> {
        check(Build::AlternatingColour, board, mv)
    }

    fn is_won(&self, board: &Board) -> bool {
        is_won(board)
    }

    fn send_home(&self, board: &Board, from: PileId) -> Option<Action> {
        send_home(board, from)
    }

    fn autoplay(&self, board: &Board) -> Option<Action> {
        autoplay(board)
    }

    fn is_useful(&self, board: &Board, mv: Move) -> bool {
        is_useful(board, mv)
    }

    fn rank(&self, board: &Board, mv: Move) -> i32 {
        rank(board, mv)
    }

    fn status(&self, board: &Board) -> Vec<StatusItem> {
        status(board)
    }
}

/// Positions other modules' tests need to build.
#[cfg(test)]
pub mod tests_support {
    use super::*;
    use crate::card::KING;
    use crate::card::Suit::{Hearts, Spades};

    /// One column holding a King with a 9 on top of it, and nothing else on the
    /// board: the 9 has exactly two useful moves, and parking it in a cell
    /// outranks filling an empty column with it.
    pub fn a_card_worth_parking() -> Board {
        let mut piles = vec![Pile::tableau(
            0,
            &[Card::new(KING, Spades), Card::new(9, Hearts)],
        )];
        piles.resize(TABLEAU, Pile::tableau(0, &[]));
        piles.extend((0..CELLS).map(|_| Pile::new(PileKind::FreeCell)));
        piles.extend((0..FOUNDATIONS).map(|_| Pile::new(PileKind::Foundation)));
        Board::new(0, piles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Suit::{Clubs, Diamonds, Hearts, Spades};
    use crate::card::{KING, Suit};
    use std::collections::HashSet;

    fn c(rank: u8, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    fn column(cards: &[Card]) -> Pile {
        Pile::tableau(0, cards)
    }

    fn cell(card: Card) -> Pile {
        Pile::holding(PileKind::FreeCell, 0, &[card])
    }

    fn foundation(cards: &[Card]) -> Pile {
        Pile::holding(PileKind::Foundation, 0, cards)
    }

    /// A board of eight columns, then the four cells, then the four
    /// foundations. Anything left short is filled in empty.
    fn board(columns: &[Pile], cells: &[Pile], foundations: &[Pile]) -> Board {
        let mut piles: Vec<Pile> = columns.to_vec();
        piles.resize(TABLEAU, Pile::tableau(0, &[]));
        piles.extend_from_slice(cells);
        piles.resize(TABLEAU + CELLS, Pile::new(PileKind::FreeCell));
        piles.extend_from_slice(foundations);
        piles.resize(
            TABLEAU + CELLS + FOUNDATIONS,
            Pile::new(PileKind::Foundation),
        );
        Board::new(0, piles)
    }

    /// Eight columns with a run of `run` in the first, a matching landing place
    /// in the second and a mismatched one in the third, so a supermove has
    /// somewhere to go and somewhere it cannot. Every other column is occupied,
    /// so the only empty columns are the ones a test asks for.
    fn packed(columns: &[Pile]) -> Vec<Pile> {
        // High cards, so no filler ever takes the run under test.
        let filler = [
            c(KING, Clubs),
            c(KING, Diamonds),
            c(12, Spades),
            c(12, Hearts),
            c(12, Clubs),
            c(11, Spades),
            c(11, Clubs),
        ];
        let mut piles: Vec<Pile> = columns.to_vec();
        piles.extend(
            filler
                .into_iter()
                .take(TABLEAU - columns.len())
                .map(|card| column(&[card])),
        );
        assert_eq!(piles.len(), TABLEAU, "every column accounted for");
        piles
    }

    #[test]
    fn deal_has_the_right_shape() {
        let board = FreeCell.deal(7);
        assert_eq!(board.piles().len(), TABLEAU + CELLS + FOUNDATIONS);

        for pile in 0..TABLEAU {
            let expected = if pile < LONG_COLUMNS { 7 } else { 6 };
            assert_eq!(board.pile(pile).len(), expected, "column {pile}");
        }
        // Nothing is hidden, and there is no stock to hide anything in.
        assert_eq!(board.face_down_count(), 0);
        assert!(board.first_of_kind(PileKind::Stock).is_none());

        for pile in cells().chain(foundations()) {
            assert!(board.pile(pile).is_empty(), "pile {pile} starts empty");
        }

        let dealt: HashSet<Card> = (0..TABLEAU)
            .flat_map(|pile| board.pile(pile).cards().iter().copied())
            .collect();
        assert_eq!(dealt.len(), DECK);
    }

    #[test]
    fn the_deal_is_pinned_to_its_seed() {
        let board = FreeCell.deal(42);
        assert_eq!(
            board.pile(0).cards(),
            &[
                c(7, Spades),
                c(3, Diamonds),
                c(KING, Spades),
                c(ACE, Clubs),
                c(2, Clubs),
                c(ACE, Spades),
                c(ACE, Diamonds),
            ]
        );
        assert_eq!(
            board.pile(4).cards(),
            &[
                c(4, Diamonds),
                c(3, Hearts),
                c(10, Diamonds),
                c(11, Clubs),
                c(4, Spades),
                c(6, Clubs),
            ]
        );
        assert_eq!(board.pile(7).exposed(), Some(c(10, Spades)));
    }

    #[test]
    fn the_tableau_builds_down_in_alternating_colour() {
        let board = board(
            &[
                column(&[c(10, Spades)]),
                column(&[c(9, Hearts)]),
                column(&[c(9, Spades)]),
                column(&[c(8, Hearts)]),
            ],
            &[],
            &[],
        );
        let check = |from, index, to| FreeCell.check(&board, Move::new(from, index, to));

        assert_eq!(check(1, 0, 0), Ok(()), "9♥ onto 10♠");
        assert_eq!(check(2, 0, 0), Err(MoveError::Mismatch), "9♠ onto 10♠");
        assert_eq!(check(3, 0, 0), Err(MoveError::Mismatch), "8♥ onto 10♠");
        assert_eq!(check(0, 0, 0), Err(MoveError::SamePile));
        assert_eq!(check(0, 5, 1), Err(MoveError::NoSuchCard));
    }

    #[test]
    fn any_card_may_take_an_empty_column() {
        // Every other game in this tree answers `NotKing` here. FreeCell has no
        // such arm: an empty column is simply space.
        let board = board(
            &[column(&[c(KING, Spades)]), column(&[c(4, Clubs)])],
            &[],
            &[],
        );

        assert_eq!(FreeCell.check(&board, Move::new(0, 0, 2)), Ok(()), "a King");
        assert_eq!(FreeCell.check(&board, Move::new(1, 0, 2)), Ok(()), "a 4");
    }

    #[test]
    fn a_free_cell_takes_one_card_and_only_when_it_is_empty() {
        let board = board(
            &[column(&[c(KING, Spades), c(4, Clubs)])],
            &[cell(c(2, Diamonds))],
            &[],
        );
        let check = |from, index, to| FreeCell.check(&board, Move::new(from, index, to));

        assert_eq!(check(0, 1, 9), Ok(()), "4♣ into an empty cell");
        assert_eq!(check(0, 1, 8), Err(MoveError::Occupied), "a cell holds one");
        assert_eq!(
            check(0, 0, 9),
            Err(MoveError::NotExposed),
            "a cell never takes a run"
        );
        // A card comes back out of a cell like any other.
        assert_eq!(check(8, 0, 1), Ok(()), "2♦ into an empty column");
    }

    #[test]
    fn a_run_moves_only_when_it_is_already_in_order() {
        let board = board(
            &[
                column(&[c(9, Hearts), c(7, Diamonds)]),
                column(&[c(10, Spades)]),
                column(&[c(8, Clubs)]),
            ],
            &[],
            &[],
        );
        let check = |from, index, to| FreeCell.check(&board, Move::new(from, index, to));

        // Yukon would lift this pair as it stands; here they are not a run.
        assert_eq!(check(0, 0, 1), Err(MoveError::NotASequence));
        assert_eq!(check(0, 1, 2), Ok(()), "its tip alone still moves");
    }

    #[test]
    fn a_supermove_is_limited_by_the_cells_and_the_empty_columns() {
        let run = column(&[
            c(8, Spades),
            c(7, Hearts),
            c(6, Clubs),
            c(5, Diamonds),
            c(4, Spades),
            c(3, Hearts),
        ]);
        let columns = packed(&[run, column(&[c(9, Hearts)]), column(&[c(8, Clubs)])]);
        let packed_in = board(&columns, &[], &[]);
        let check = |from, index, to| FreeCell.check(&packed_in, Move::new(from, index, to));

        // Four cells free and no empty column: five cards may travel.
        assert_eq!(check(0, 1, 2), Ok(()), "five onto 8♣");
        assert_eq!(check(0, 0, 1), Err(MoveError::TooMany), "six onto 9♥");

        // Two cells free and one empty column: six.
        let longer = column(&[
            c(9, Diamonds),
            c(8, Spades),
            c(7, Hearts),
            c(6, Clubs),
            c(5, Diamonds),
            c(4, Spades),
            c(3, Hearts),
        ]);
        let columns = packed(&[
            longer,
            column(&[c(10, Spades)]),
            column(&[c(9, Hearts)]),
            column(&[]),
        ]);
        let roomier = board(
            &columns,
            &[cell(c(ACE, Clubs)), cell(c(ACE, Diamonds))],
            &[],
        );
        let check = |from, index, to| FreeCell.check(&roomier, Move::new(from, index, to));

        assert_eq!(check(0, 1, 2), Ok(()), "six onto 9♥");
        assert_eq!(check(0, 0, 1), Err(MoveError::TooMany), "seven onto 10♠");
    }

    #[test]
    fn an_empty_target_column_does_not_count_towards_its_own_limit() {
        // Two cells free and exactly one empty column — the one being landed
        // on, which is no room at all: three cards, not six.
        let run = column(&[c(6, Clubs), c(5, Diamonds), c(4, Spades), c(3, Hearts)]);
        let columns = packed(&[run, column(&[])]);
        let board = board(
            &columns,
            &[cell(c(ACE, Clubs)), cell(c(ACE, Diamonds))],
            &[],
        );
        let check = |from, index, to| FreeCell.check(&board, Move::new(from, index, to));

        assert_eq!(check(0, 1, 1), Ok(()), "three into the empty column");
        assert_eq!(check(0, 0, 1), Err(MoveError::TooMany), "four");
    }

    #[test]
    fn a_run_that_could_never_land_there_is_refused_for_that_reason() {
        let run = column(&[
            c(8, Spades),
            c(7, Hearts),
            c(6, Clubs),
            c(5, Diamonds),
            c(4, Spades),
            c(3, Hearts),
        ]);
        let columns = packed(&[run, column(&[c(9, Spades)])]);
        let board = board(&columns, &[], &[]);

        // Six cards is over the limit *and* 8♠ does not go on 9♠. The player
        // hears the reason that will still be true next turn.
        assert_eq!(
            FreeCell.check(&board, Move::new(0, 0, 1)),
            Err(MoveError::Mismatch)
        );
    }

    #[test]
    fn a_supermove_is_one_action_and_one_undo() {
        let run = column(&[c(8, Spades), c(7, Hearts), c(6, Clubs)]);
        let columns = packed(&[run, column(&[c(9, Hearts)])]);
        let start = board(&columns, &[], &[]);
        let mut board = start.clone();

        let mv = Move::new(0, 0, 1);
        assert_eq!(FreeCell.check(&board, mv), Ok(()));
        let action = FreeCell.action_for(&board, mv);
        assert_eq!(action.steps.len(), 1, "a supermove is one step");

        board.apply(&action);
        assert_eq!(board.pile(1).cards().len(), 4);
        board.undo(&action);
        assert_eq!(board, start);
    }

    #[test]
    fn foundations_build_up_in_suit_from_the_ace() {
        let board = board(
            &[
                column(&[c(ACE, Spades)]),
                column(&[c(2, Spades)]),
                column(&[c(2, Hearts)]),
                column(&[c(KING, Spades), c(2, Clubs)]),
            ],
            &[],
            &[foundation(&[c(ACE, Clubs)])],
        );
        let check = |from, index, to| FreeCell.check(&board, Move::new(from, index, to));

        assert_eq!(check(0, 0, 13), Ok(()), "an Ace onto a bare foundation");
        assert_eq!(check(3, 1, 12), Ok(()), "2♣ onto A♣");
        assert_eq!(check(2, 0, 12), Err(MoveError::Mismatch), "2♥ onto A♣");
        assert_eq!(check(1, 0, 12), Err(MoveError::Mismatch), "2♠ onto A♣");
        assert_eq!(
            check(3, 0, 13),
            Err(MoveError::NotExposed),
            "foundations take one card at a time"
        );
        assert_eq!(
            check(12, 0, 13),
            Err(MoveError::WrongTarget),
            "a foundation never feeds another"
        );
    }

    #[test]
    fn a_card_may_be_taken_back_off_a_foundation() {
        let start = board(
            &[column(&[]), column(&[c(3, Hearts)])],
            &[],
            &[foundation(&[c(ACE, Spades), c(2, Spades)])],
        );
        let check = |from, index, to| FreeCell.check(&start, Move::new(from, index, to));

        assert_eq!(check(12, 1, 1), Ok(()), "2♠ back onto 3♥");
        assert_eq!(check(12, 1, 8), Ok(()), "2♠ into a cell");
        assert_eq!(
            check(12, 0, 1),
            Err(MoveError::NotExposed),
            "and only the top one"
        );

        let mut board = start.clone();
        let action = FreeCell.action_for(&board, Move::new(12, 1, 1));
        board.apply(&action);
        board.undo(&action);
        assert_eq!(board, start);
    }

    #[test]
    fn send_home_works_from_a_column_or_a_cell() {
        let board = board(
            &[column(&[c(ACE, Spades)]), column(&[c(5, Clubs)])],
            &[cell(c(ACE, Hearts))],
            &[],
        );

        assert!(send_home(&board, 0).is_some(), "an Ace in a column");
        assert!(send_home(&board, 8).is_some(), "an Ace in a cell");
        assert!(send_home(&board, 1).is_none(), "nowhere to go yet");
        assert!(send_home(&board, 12).is_none(), "not back off a foundation");
    }

    #[test]
    fn autoplay_waits_until_every_column_runs_down_in_rank() {
        // The trap this game exists to trip: nothing is ever face down, so the
        // gate every other game uses would call the opening deal decided.
        assert!(FreeCell.autoplay(&FreeCell.deal(42)).is_none());

        let buried = board(&[column(&[c(ACE, Spades), c(5, Spades)])], &[], &[]);
        assert!(FreeCell.autoplay(&buried).is_none(), "A♠ is covered");

        let clear = board(&[column(&[c(5, Spades), c(ACE, Spades)])], &[], &[]);
        assert!(FreeCell.autoplay(&clear).is_some());
    }

    #[test]
    fn autoplay_drains_the_cells_as_well_as_the_columns() {
        // Four suits stacked King down to 2, with the Aces parked in the cells.
        // A finish that read only the columns would strand them there.
        let columns: Vec<Pile> = Suit::ALL
            .into_iter()
            .map(|suit| {
                column(
                    &(2..=KING)
                        .rev()
                        .map(|rank| c(rank, suit))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        let cells: Vec<Pile> = Suit::ALL
            .into_iter()
            .map(|suit| cell(c(ACE, suit)))
            .collect();
        let mut board = board(&columns, &cells, &[]);

        let mut played = 0;
        while let Some(action) = FreeCell.autoplay(&board) {
            board.apply(&action);
            played += 1;
            assert!(played <= DECK, "autoplay is not finishing");
        }
        assert_eq!(played, DECK);
        assert!(FreeCell.is_won(&board));
    }

    #[test]
    fn hints_offer_only_the_first_empty_cell_and_the_first_empty_column() {
        // Four empty cells and seven empty columns are two moves, not eleven.
        let board = tests_support::a_card_worth_parking();
        let suggestions = FreeCell.suggestions(&board);

        assert_eq!(suggestions, vec![Move::new(0, 1, 8), Move::new(0, 1, 1)]);
    }

    #[test]
    fn hints_never_take_a_card_back_off_a_foundation() {
        let board = board(
            &[column(&[c(2, Hearts)])],
            &[],
            &[foundation(&[c(ACE, Spades)])],
        );

        assert_eq!(FreeCell.check(&board, Move::new(12, 0, 0)), Ok(()));
        assert!(
            FreeCell.suggestions(&board).iter().all(|mv| mv.from != 12),
            "legal, but never the move to suggest"
        );
    }

    #[test]
    fn sliding_a_whole_column_into_an_empty_one_is_not_suggested() {
        let board = board(&[column(&[c(KING, Spades)])], &[], &[]);

        assert_eq!(FreeCell.check(&board, Move::new(0, 0, 1)), Ok(()));
        assert!(!FreeCell.suggestions(&board).contains(&Move::new(0, 0, 1)));
    }

    #[test]
    fn the_status_line_counts_the_cards_home_and_the_cells_free() {
        let started = board(
            &[],
            &[cell(c(4, Clubs))],
            &[foundation(&[c(ACE, Spades), c(2, Spades)])],
        );
        let status = FreeCell.status(&started);
        assert_eq!(status[0].value, "2/52");
        assert_eq!(status[1].value, "3/4");
        assert_eq!(status[1].tone, StatusTone::Good);

        let full = board(
            &[],
            &[
                cell(c(4, Clubs)),
                cell(c(5, Clubs)),
                cell(c(6, Clubs)),
                cell(c(7, Clubs)),
            ],
            &[],
        );
        let status = FreeCell.status(&full);
        assert_eq!(status[1].value, "0/4");
        assert_eq!(status[1].tone, StatusTone::Muted);
    }

    #[test]
    fn a_full_set_of_foundations_is_a_win() {
        let suited: Vec<Pile> = Suit::ALL
            .into_iter()
            .map(|suit| foundation(&(ACE..=KING).map(|rank| c(rank, suit)).collect::<Vec<_>>()))
            .collect();

        assert!(FreeCell.is_won(&board(&[], &[], &suited)));
        assert!(!FreeCell.is_won(&FreeCell.deal(42)));
    }

    #[test]
    fn undo_round_trips_a_played_game() {
        let start = FreeCell.deal(2024);
        let mut board = start.clone();
        let mut history = Vec::new();

        // A stride of 5 rather than the 7 the other games walk with: FreeCell
        // is a game you can lose, and 7 paints this deal into a corner after a
        // dozen moves, which is too short a walk to exercise anything.
        for step in 0..400 {
            let moves = FreeCell.suggestions(&board);
            if moves.is_empty() {
                break;
            }
            let action = FreeCell.action_for(&board, moves[(step * 5 + 3) % moves.len()]);
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
    fn a_played_game_never_gains_or_loses_a_card() {
        let mut board = FreeCell.deal(99);

        for step in 0..200 {
            let moves = FreeCell.suggestions(&board);
            if moves.is_empty() {
                break;
            }
            board.apply(&FreeCell.action_for(&board, moves[(step * 5 + 1) % moves.len()]));

            let seen: HashSet<Card> = board
                .piles()
                .iter()
                .flat_map(|pile| pile.cards().iter().copied())
                .collect();
            assert_eq!(seen.len(), DECK, "after {} moves", step + 1);
        }
    }
}
