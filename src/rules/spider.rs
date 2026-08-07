//! Spider — two decks, ten columns, and a stock that is the whole problem.
//!
//! * 104 cards dealt into ten columns, four of six and six of five, with only
//!   the last card of each face up. The remaining 50 cards are the stock.
//! * Build the tableau **down regardless of suit**, but only a **same-suit
//!   descending run** may be lifted as a unit.
//! * **Any** card may take an empty column, which is what makes one worth
//!   having.
//! * The stock deals one card onto *every* column at once, five times over,
//!   and refuses while any column stands empty.
//! * A complete King-to-Ace run in one suit leaves the tableau for good. That
//!   happens on its own, as part of the move that finished it.
//! * You win when all eight runs are filed.
//!
//! Spiderette is this game on one deck and seven columns, so everything here
//! but the name and the blurb is shared with [`super::spiderette`] through a
//! [`Setup`]. Note that [`check`] takes no `Setup` at all: the build rule, the
//! lift rule and the empty-column rule are the same game in both.
//!
//! The one thing no other game here does is file a finished run. Those steps
//! are appended to the *same* [`Action`] as the move that completed the run —
//! see [`with_filing`] — so it costs one undo and `app.rs` never learns what a
//! run is. The default `suggestions` enumeration is left alone; nineteen piles
//! cost a few thousand comparisons per hint, which is nothing.

use crate::board::{
    Action, Board, BoardSpec, Fan, Move, MoveError, Pile, PileId, PileKind, Spot, Step,
};
use crate::card::{ACE, Card, KING, Suit, shuffled_decks};
use crate::rules::{OptionSpec, Rules, StatusItem, StatusTone};

/// Cards in a finished run, King down to Ace.
const RUN: usize = KING as usize;

// -- shared with `spiderette` ----------------------------------------------
//
// Only the deal, the board's shape, the stock and the scoreboard care which
// game this is; the rules themselves are identical, so they live here once.

/// Everything that separates Spiderette from Spider.
///
/// One `Copy` type passed as one argument, rather than three parameters
/// threaded through half the module — the same discipline `yukon::Build`
/// keeps for a game that differs by a single predicate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Setup {
    decks: usize,
    /// How many suits the decks are squashed into: 1, 2 or 4.
    suits: usize,
    /// How many cards each column is dealt. Its length is the column count.
    columns: &'static [usize],
}

impl Setup {
    pub(super) const SPIDERETTE: Self = Self {
        decks: 1,
        suits: 4,
        columns: &[1, 2, 3, 4, 5, 6, 7],
    };

    pub(super) const fn spider(suits: usize) -> Self {
        Self {
            decks: 2,
            suits,
            columns: &[6, 6, 6, 6, 5, 5, 5, 5, 5, 5],
        }
    }

    const fn column_count(self) -> usize {
        self.columns.len()
    }

    /// One foundation per run the decks can yield.
    const fn foundation_count(self) -> usize {
        self.decks * 4
    }

    /// The first foundation; the rest follow it, then the stock.
    const fn foundation(self) -> PileId {
        self.column_count()
    }

    const fn stock(self) -> PileId {
        self.column_count() + self.foundation_count()
    }
}

/// Whether `cards` run down in one suit, so they travel together.
fn is_suited_run(cards: &[Card]) -> bool {
    cards
        .windows(2)
        .all(|pair| pair[1].rank + 1 == pair[0].rank && pair[1].suit == pair[0].suit)
}

/// How many cards at the foot of `pile` run down in one suit, face up.
///
/// Spider's build rule read backwards, which is why it lives here rather than
/// on `Pile`: `board.rs` knows no game's rules. It returns a length because
/// both callers want one — filing needs to know the run is all thirteen, and
/// `rank` needs to know how close a move gets to that.
fn suited_tail(pile: &Pile) -> usize {
    let cards = pile.cards();
    let mut run = 0;
    while run < cards.len() {
        let at = cards.len() - 1 - run;
        if !pile.is_face_up(at) {
            break;
        }
        let joins = run == 0
            || (cards[at].suit == cards[at + 1].suit && cards[at].rank == cards[at + 1].rank + 1);
        if !joins {
            break;
        }
        run += 1;
    }
    run
}

/// A column holding a finished run, and the index that run starts at.
fn finished_run(setup: Setup, board: &Board) -> Option<(PileId, usize)> {
    (0..setup.column_count()).find_map(|column| {
        let pile = board.pile(column);
        // A thirteen-long suited run ending on an Ace starts on a King, so
        // these two tests together are the whole condition.
        let done = pile.exposed().is_some_and(|card| card.rank == ACE) && suited_tail(pile) == RUN;
        done.then(|| (column, pile.len() - RUN))
    })
}

fn free_foundation(setup: Setup, board: &Board) -> Option<PileId> {
    (setup.foundation()..setup.stock()).find(|&pile| board.pile(pile).is_empty())
}

/// `base`, plus the steps that file every finished run it leaves behind.
///
/// Detection needs the board *after* the move, so this walks a scratch copy:
/// each filing step's flags are worked out against the state that step will
/// actually meet, which is exactly what `Board::undo` assumes when it walks
/// the steps back in reverse. The scratch's move counter is thrown away, so
/// the whole thing still costs the player one move and one undo.
fn with_filing(setup: Setup, board: &Board, base: Action) -> Action {
    let mut scratch = board.clone();
    scratch.apply(&base);
    let mut steps = base.steps;

    // A loop rather than one pass over the columns: a stock deal drops a card
    // on every column at once and can finish several runs together. It cannot
    // cascade — nothing is ever played onto an Ace, so an Ace is only buried
    // by a deal, and filing is automatic enough that no finished run is ever
    // sitting there when one happens.
    while let Some((column, start)) = finished_run(setup, &scratch) {
        let Some(home) = free_foundation(setup, &scratch) else {
            break;
        };
        let mut step = Action::moving(&scratch, column, start, home).steps[0];
        // The run is turned over as it is set aside, so a filled foundation
        // shows its King the way Klondike's and Yukon's do rather than a bare
        // Ace — and `Pile::is_complete_suit` is then what certifies it.
        step.reverse = true;
        scratch.apply(&Action::single(step));
        steps.push(step);
    }
    Action::new(steps)
}

pub(super) fn deal(setup: Setup, seed: u64) -> Board {
    let mut deck = shuffled_decks(seed, setup.decks);
    if setup.suits < 4 {
        // Squashed after the shuffle, which keeps `card.rs` free of any game's
        // idea of a deck. The permutation is already uniform and repainting
        // the faces is position-independent, so every arrangement of the
        // resulting multiset stays equally likely — and one seed is one board
        // at every suit count, just dealt in fewer colours.
        for card in &mut deck {
            *card = Card::new(card.rank, Suit::ALL[card.suit.index() % setup.suits]);
        }
    }

    let mut deck = deck.into_iter();
    let mut piles: Vec<Pile> = setup
        .columns
        .iter()
        .map(|&dealt| {
            let cards: Vec<_> = deck.by_ref().take(dealt).collect();
            Pile::tableau(dealt - 1, &cards)
        })
        .collect();
    piles.extend((0..setup.foundation_count()).map(|_| Pile::new(PileKind::Foundation)));
    let stock: Vec<_> = deck.collect();
    piles.push(Pile::holding(PileKind::Stock, stock.len(), &stock));
    Board::new(seed, piles)
}

pub(super) fn spec(setup: Setup) -> BoardSpec {
    let columns = setup.column_count();
    // The foundations sit at the right-hand end of the top row, which puts
    // Spiderette's four exactly where Klondike and Yukon draw theirs and
    // leaves the stock alone on the left. Neither game's top row is wider
    // than its tableau, so this costs no width at all.
    let first = (columns - setup.foundation_count()) as u16;
    let mut spots = vec![Spot::new(setup.stock(), 0, 0, Fan::None)];
    spots.extend(
        (0..setup.foundation_count())
            .map(|i| Spot::new(setup.foundation() + i, 0, first + i as u16, Fan::None)),
    );
    spots.extend((0..columns).map(|i| Spot::new(i, 1, i as u16, Fan::Down)));
    BoardSpec::new(spots)
}

/// Deliberately takes no [`Setup`]: the build rule, the lift rule and the
/// empty-column rule are identical in Spiderette and Spider, which is the
/// clearest evidence the two really are one game's rules.
pub(super) fn check(board: &Board, mv: Move) -> Result<(), MoveError> {
    let Move { from, index, to } = mv;
    if from == to {
        return Err(MoveError::SamePile);
    }
    let source = board.pile(from);
    let target = board.pile(to);
    // Only the tableau is ever a source or a target: the stock deals itself,
    // and a finished run reaches a foundation on its own.
    if source.kind() != PileKind::Tableau || target.kind() != PileKind::Tableau {
        return Err(MoveError::WrongTarget);
    }
    if index >= source.len() {
        return Err(MoveError::NoSuchCard);
    }
    if !source.is_face_up(index) {
        return Err(MoveError::FaceDown);
    }

    let moving = &source.cards()[index..];
    if !is_suited_run(moving) {
        return Err(MoveError::NotASequence);
    }

    match target.exposed() {
        // Down regardless of suit. A King's rank + 1 is 14, so this arm also
        // keeps Kings off other cards, exactly as it does elsewhere.
        Some(card) if card.rank == moving[0].rank + 1 => Ok(()),
        Some(_) => Err(MoveError::Mismatch),
        // Any card takes an empty column — the one generosity in the game.
        None => Ok(()),
    }
}

pub(super) fn action_for(setup: Setup, board: &Board, mv: Move) -> Action {
    with_filing(
        setup,
        board,
        Action::moving(board, mv.from, mv.index, mv.to),
    )
}

/// The bare deal, before any run it finishes is filed.
///
/// Kept separate from `activate` so `has_progress_move` — which runs after
/// every move — can ask whether the stock will deal without cloning a board.
fn stock_deal(setup: Setup, board: &Board, pile: PileId) -> Option<Action> {
    if pile != setup.stock() {
        return None;
    }
    let left = board.pile(setup.stock()).len();
    if left == 0 {
        return None;
    }
    // The rule the whole game turns on. An empty column is the only room to
    // work in, and dealing is the only thing that fills one back up, so being
    // unable to deal until you have spent your space is the tension.
    if (0..setup.column_count()).any(|column| board.pile(column).is_empty()) {
        return None;
    }

    // The last deal can be short — Spiderette's 24 stock cards are three full
    // rounds and a round of three — and then it covers the leftmost columns.
    Some(Action::new(
        (0..setup.column_count().min(left))
            .map(|to| Step {
                from: setup.stock(),
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

pub(super) fn activate(setup: Setup, board: &Board, pile: PileId) -> Option<Action> {
    stock_deal(setup, board, pile).map(|deal| with_filing(setup, board, deal))
}

/// Whether the stock will deal, which is the only thing `activate` needs a
/// board for and the only sense in which a stuck board is not yet dead.
pub(super) fn stock_deals(setup: Setup, board: &Board) -> bool {
    stock_deal(setup, board, setup.stock()).is_some()
}

pub(super) fn stock_refusal(setup: Setup, board: &Board) -> &'static str {
    if board.pile(setup.stock()).is_empty() {
        "The stock has nothing left to give."
    } else {
        "Fill every empty column before dealing."
    }
}

pub(super) fn is_won(setup: Setup, board: &Board) -> bool {
    (setup.foundation()..setup.stock()).all(|pile| board.pile(pile).is_complete_suit())
}

/// Sliding a whole face-up column into an empty one just renames two piles.
///
/// Unlike Klondike's and Yukon's, this needs no `PileKind::Tableau` qualifier:
/// `check` has already refused every foundation and the stock, so an empty
/// target here is always an empty column.
pub(super) fn is_useful(board: &Board, mv: Move) -> bool {
    !(mv.index == 0 && board.pile(mv.to).is_empty())
}

/// Whether `mv` puts the last card on a run.
///
/// Short-circuits on the source's exposed card being an Ace, which is false
/// for very nearly every move `suggestions` offers this.
fn completes_a_run(board: &Board, mv: Move) -> bool {
    let source = board.pile(mv.from);
    if source.exposed().is_none_or(|card| card.rank != ACE) {
        return false;
    }
    // `check` has already established that the lifted cards run down in suit
    // and sit on the target's exposed card, so the joined tail is one run.
    let moved = source.len() - mv.index;
    moved <= RUN && suited_tail(board.pile(mv.to)) + moved == RUN
}

pub(super) fn rank(board: &Board, mv: Move) -> i32 {
    let Move { from, index, to } = mv;
    let (source, target) = (board.pile(from), board.pile(to));
    let head = source.cards()[index];
    let mut value = 0;

    if completes_a_run(board, mv) {
        value += 200; // the whole point of the game, and never the wrong move
    }
    if index > 0 && index == source.face_down() {
        value += 100; // turns a card over
    }
    if target.exposed().is_some_and(|card| card.suit == head.suit) {
        value += 50; // in suit, which is the only way a run ever gets built
    }
    if index == 0 && !target.is_empty() {
        value += 30; // empties a column, the scarcest thing on the board
    }
    // Splitting a suited run apart undoes work already done.
    if index > source.face_down()
        && source.cards()[index - 1].suit == head.suit
        && source.cards()[index - 1].rank == head.rank + 1
    {
        value -= 40;
    }
    if target.is_empty() {
        value -= 25; // and spends that scarce space
    }

    value - (source.len() - index) as i32
}

pub(super) fn status(setup: Setup, board: &Board) -> Vec<StatusItem> {
    let filed = (setup.foundation()..setup.stock())
        .filter(|&pile| !board.pile(pile).is_empty())
        .count();
    // `div_ceil`, not `/`: a short last deal is still a deal, and plain
    // division would tell a Spiderette player they had three left when the
    // stock holds four.
    let deals = board
        .pile(setup.stock())
        .len()
        .div_ceil(setup.column_count());

    vec![
        StatusItem::new(
            "runs",
            format!("{filed}/{}", setup.foundation_count()),
            StatusTone::Good,
        ),
        match deals {
            0 => StatusItem::new("deals", "spent", StatusTone::Muted),
            n => StatusItem::new("deals", n.to_string(), StatusTone::Good),
        },
    ]
}

pub(super) const HELP: &str = "  d             deal one card onto every column";

// -- Spider itself ---------------------------------------------------------

const OPTIONS: [OptionSpec; 1] = [OptionSpec {
    label: "Suits",
    values: &["1", "2", "4"],
}];

/// How many suits the 104 cards are squashed into, per `Suits` value.
const SUIT_COUNTS: [usize; 3] = [1, 2, 4];

/// The first value of every option is its default, so a fresh `Spider` is the
/// one-suit game — forgiving enough to be the first patience someone plays in
/// a terminal, and still the same deal as the four-suit one.
#[derive(Default)]
pub struct Spider {
    /// Which value each option in [`OPTIONS`] sits on.
    selected: [usize; OPTIONS.len()],
}

impl Spider {
    fn setup(&self) -> Setup {
        Setup::spider(SUIT_COUNTS[self.selected[0]])
    }
}

impl Rules for Spider {
    fn name(&self) -> &'static str {
        "Spider"
    }

    fn blurb(&self) -> &'static str {
        "Two decks in ten columns. Build down regardless of suit, but only a \
         run of one suit may be lifted as a unit. Any card takes an empty \
         column.\n\n\
         The stock deals a card onto every column at once and will not deal \
         while a column stands empty. A finished King-to-Ace run files itself. \
         Win by filing all eight.\n\n\
         Rules you can change: Suits (1, 2 or 4). The suit count repaints one \
         deal rather than dealing another, so a seed is one board at three \
         difficulties."
    }

    fn help(&self) -> &'static str {
        HELP
    }

    fn options(&self) -> &[OptionSpec] {
        &OPTIONS
    }

    fn selected(&self) -> Vec<usize> {
        self.selected.to_vec()
    }

    fn configure(&mut self, index: usize, delta: i32) {
        if let Some(slot) = self.selected.get_mut(index) {
            let count = OPTIONS[index].values.len() as i32;
            *slot = (*slot as i32 + delta).rem_euclid(count) as usize;
        }
    }

    fn deal(&self, seed: u64) -> Board {
        deal(self.setup(), seed)
    }

    fn spec(&self, _board: &Board) -> BoardSpec {
        spec(self.setup())
    }

    fn check(&self, board: &Board, mv: Move) -> Result<(), MoveError> {
        check(board, mv)
    }

    fn action_for(&self, board: &Board, mv: Move) -> Action {
        action_for(self.setup(), board, mv)
    }

    fn activate(&self, board: &Board, pile: PileId) -> Option<Action> {
        activate(self.setup(), board, pile)
    }

    fn stock_refusal(&self, board: &Board) -> &'static str {
        stock_refusal(self.setup(), board)
    }

    fn is_won(&self, board: &Board) -> bool {
        is_won(self.setup(), board)
    }

    fn is_useful(&self, board: &Board, mv: Move) -> bool {
        is_useful(board, mv)
    }

    fn rank(&self, board: &Board, mv: Move) -> i32 {
        rank(board, mv)
    }

    /// A stock that will not deal because a column is empty is not progress,
    /// which is why this asks the same question `activate` does rather than
    /// whether the stock has cards.
    fn has_progress_move(&self, board: &Board) -> bool {
        stock_deals(self.setup(), board) || !self.suggestions(board).is_empty()
    }

    fn status(&self, board: &Board) -> Vec<StatusItem> {
        status(self.setup(), board)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Suit;
    use crate::card::Suit::{Clubs, Diamonds, Hearts, Spades};

    /// Every hand-built position here uses Spider's shape. The suit count only
    /// touches the deal, so the rules tests are free to use all four.
    const S: Setup = Setup::spider(4);

    fn c(rank: u8, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    fn spider() -> Spider {
        Spider::default()
    }

    /// Spider with all four suits — `configure` rather than a private field,
    /// so the test drives the game the way the menu does.
    fn four_suits() -> Spider {
        let mut game = spider();
        game.configure(0, 2);
        assert_eq!(game.setup().suits, 4);
        game
    }

    /// A board built column by column: tableau, then empty foundations, then
    /// the stock. Columns left unnamed are dealt in empty.
    fn board(columns: &[Pile], stock: &[Card]) -> Board {
        let mut piles: Vec<Pile> = columns.to_vec();
        piles.resize(S.column_count(), Pile::tableau(0, &[]));
        piles.resize(S.stock(), Pile::new(PileKind::Foundation));
        piles.push(Pile::holding(PileKind::Stock, stock.len(), stock));
        Board::new(0, piles)
    }

    /// A finished run as it sits in the tableau: King down to Ace.
    fn run(suit: Suit) -> Vec<Card> {
        (ACE..=KING).rev().map(|rank| c(rank, suit)).collect()
    }

    /// A finished run as it sits on a foundation: Ace up to King.
    fn filed(suit: Suit) -> Pile {
        let cards: Vec<_> = (ACE..=KING).map(|rank| c(rank, suit)).collect();
        Pile::holding(PileKind::Foundation, 0, &cards)
    }

    /// Every card on the board, sorted.
    ///
    /// A `HashSet` would collapse the duplicate the second deck introduces and
    /// go on passing while half the cards went missing, which is exactly the
    /// trap two-deck games bring with them.
    fn census(board: &Board) -> Vec<(usize, u8)> {
        let mut cards: Vec<(usize, u8)> = board
            .piles()
            .iter()
            .flat_map(Pile::cards)
            .map(|card| (card.suit.index(), card.rank))
            .collect();
        cards.sort_unstable();
        cards
    }

    // -- the deal ----------------------------------------------------------

    #[test]
    fn deal_has_the_right_shape() {
        let board = four_suits().deal(42);
        assert_eq!(board.piles().len(), 19, "ten columns, eight homes, a stock");

        for column in 0..S.column_count() {
            let pile = board.pile(column);
            let dealt = if column < 4 { 6 } else { 5 };
            assert_eq!(pile.len(), dealt, "column {column}");
            assert_eq!(pile.face_down(), dealt - 1, "only the last card shows");
        }
        for home in S.foundation()..S.stock() {
            assert!(board.pile(home).is_empty(), "foundations start bare");
        }

        let stock = board.pile(S.stock());
        assert_eq!(stock.len(), 50, "five deals of ten");
        assert_eq!(stock.face_down(), 50, "the stock is entirely hidden");
        assert_eq!(census(&board).len(), 104, "two whole decks");
    }

    #[test]
    fn a_one_suit_deal_is_all_spades() {
        let board = spider().deal(42);
        assert!(
            board
                .piles()
                .iter()
                .flat_map(Pile::cards)
                .all(|card| card.suit == Spades)
        );
        // Eight cards of every rank, so eight runs are there to be built.
        let census = census(&board);
        for rank in ACE..=KING {
            assert_eq!(census.iter().filter(|&&c| c.1 == rank).count(), 8);
        }
    }

    #[test]
    fn a_two_suit_deal_is_half_spades_and_half_hearts() {
        let mut game = spider();
        game.configure(0, 1);
        let census = census(&game.deal(42));

        for suit in [Spades, Hearts] {
            assert_eq!(
                census.iter().filter(|&&c| c.0 == suit.index()).count(),
                52,
                "{suit:?}"
            );
        }
        for suit in [Diamonds, Clubs] {
            assert!(census.iter().all(|&c| c.0 != suit.index()), "{suit:?}");
        }
    }

    #[test]
    fn the_suit_count_only_repaints_the_deal() {
        // The suit option is a difficulty dial on one board, not a different
        // board, so a seed stays a seed across all three settings.
        let ranks = |game: &Spider| -> Vec<Vec<u8>> {
            game.deal(42)
                .piles()
                .iter()
                .map(|pile| pile.cards().iter().map(|card| card.rank).collect())
                .collect()
        };
        let mut two = spider();
        two.configure(0, 1);

        assert_eq!(ranks(&spider()), ranks(&four_suits()));
        assert_eq!(ranks(&two), ranks(&four_suits()));
    }

    // -- the build rule ----------------------------------------------------

    #[test]
    fn the_tableau_builds_down_regardless_of_suit() {
        let board = board(
            &[
                Pile::tableau(0, &[c(9, Spades)]),
                Pile::tableau(0, &[c(8, Hearts)]),
                Pile::tableau(0, &[c(8, Spades)]),
            ],
            &[],
        );

        assert_eq!(check(&board, Move::new(1, 0, 0)), Ok(()), "8♥ onto 9♠");
        assert_eq!(
            check(&board, Move::new(2, 0, 0)),
            Ok(()),
            "and 8♠ onto 9♠ just as happily"
        );
        assert_eq!(
            check(&board, Move::new(1, 0, 2)),
            Err(MoveError::Mismatch),
            "8♥ onto 8♠ is the wrong rank"
        );
        assert_eq!(
            check(&board, Move::new(0, 0, 1)),
            Err(MoveError::Mismatch),
            "and 9♠ onto 8♥ is backwards"
        );
    }

    #[test]
    fn only_a_same_suit_run_lifts_as_a_unit() {
        let mixed = board(
            &[
                Pile::tableau(0, &[c(10, Spades), c(9, Spades), c(8, Hearts)]),
                Pile::tableau(0, &[c(10, Hearts), c(9, Hearts), c(8, Spades)]),
                Pile::tableau(0, &[c(9, Diamonds)]),
                Pile::tableau(0, &[c(10, Clubs)]),
            ],
            &[],
        );

        assert_eq!(
            check(&mixed, Move::new(0, 1, 2)),
            Err(MoveError::NotASequence),
            "9♠8♥ is in order but not in suit",
        );
        assert_eq!(
            check(&mixed, Move::new(0, 2, 2)),
            Ok(()),
            "the 8♥ travels on its own",
        );
        assert_eq!(
            check(&mixed, Move::new(1, 0, 3)),
            Err(MoveError::NotASequence),
            "and 10♥9♥8♠ is broken by its last card",
        );

        let suited = board(
            &[
                Pile::tableau(0, &[c(10, Spades), c(9, Spades), c(8, Spades)]),
                Pile::tableau(0, &[c(10, Clubs)]),
            ],
            &[],
        );
        assert_eq!(
            check(&suited, Move::new(0, 1, 1)),
            Ok(()),
            "9♠8♠ is a run, so it moves together",
        );
    }

    #[test]
    fn any_card_may_take_an_empty_column() {
        // The one generosity in the game, and what makes an empty column the
        // thing worth spending moves on.
        let board = board(
            &[
                Pile::tableau(0, &[c(5, Diamonds)]),
                Pile::tableau(0, &[c(KING, Clubs)]),
                Pile::tableau(0, &[c(7, Hearts), c(6, Hearts)]),
            ],
            &[],
        );

        assert_eq!(check(&board, Move::new(0, 0, 9)), Ok(()), "a lone 5♦");
        assert_eq!(check(&board, Move::new(1, 0, 9)), Ok(()), "a King too");
        assert_eq!(check(&board, Move::new(2, 0, 9)), Ok(()), "and a whole run");
    }

    #[test]
    fn the_stock_and_the_foundations_are_never_a_source_or_a_target() {
        // Without this, `suggestions` would cheerfully offer to file runs by
        // hand and to play cards back off a foundation.
        let mut board = board(&[Pile::tableau(0, &[c(9, Spades)])], &[c(8, Hearts)]);
        let home = S.foundation();
        assert_eq!(
            check(&board, Move::new(0, 0, S.stock())),
            Err(MoveError::WrongTarget),
        );
        assert_eq!(
            check(&board, Move::new(0, 0, home)),
            Err(MoveError::WrongTarget),
        );
        assert_eq!(
            check(&board, Move::new(S.stock(), 0, 0)),
            Err(MoveError::WrongTarget),
        );

        board = board_with_home();
        assert_eq!(
            check(&board, Move::new(home, 12, 0)),
            Err(MoveError::WrongTarget),
            "a filed King stays filed",
        );
    }

    fn board_with_home() -> Board {
        let mut piles: Vec<Pile> = vec![Pile::tableau(0, &[c(KING, Hearts)])];
        piles.resize(S.column_count(), Pile::tableau(0, &[]));
        piles.push(filed(Spades));
        piles.resize(S.stock(), Pile::new(PileKind::Foundation));
        piles.push(Pile::new(PileKind::Stock));
        Board::new(0, piles)
    }

    #[test]
    fn illegal_drops_are_rejected() {
        let board = board(
            &[
                Pile::tableau(2, &[c(2, Clubs), c(3, Clubs), c(9, Spades)]),
                Pile::tableau(0, &[c(4, Hearts)]),
            ],
            &[],
        );

        assert_eq!(check(&board, Move::new(0, 0, 0)), Err(MoveError::SamePile));
        assert_eq!(check(&board, Move::new(0, 1, 1)), Err(MoveError::FaceDown));
        assert_eq!(
            check(&board, Move::new(0, 9, 1)),
            Err(MoveError::NoSuchCard)
        );
        assert_eq!(check(&board, Move::new(0, 2, 1)), Err(MoveError::Mismatch));
    }

    #[test]
    fn nothing_can_be_played_onto_an_ace() {
        // The invariant the filing loop leans on: an Ace is only ever buried
        // by a stock deal, so a finished run is always a column's tail.
        for suit in Suit::ALL {
            for rank in ACE..=KING {
                let board = board(
                    &[
                        Pile::tableau(0, &[c(ACE, Spades)]),
                        Pile::tableau(0, &[c(rank, suit)]),
                    ],
                    &[],
                );
                assert!(
                    check(&board, Move::new(1, 0, 0)).is_err(),
                    "{} landed on an Ace",
                    c(rank, suit),
                );
            }
        }
    }

    // -- filing a finished run ---------------------------------------------

    /// Column 0 one card short of a spade run, column 1 holding the Ace.
    fn one_card_short() -> Board {
        board(
            &[
                Pile::tableau(0, &run(Spades)[..12]),
                Pile::tableau(0, &[c(ACE, Spades)]),
            ],
            &[],
        )
    }

    #[test]
    fn completing_a_run_files_it_in_the_same_move() {
        let mut board = one_card_short();
        let action = four_suits().action_for(&board, Move::new(1, 0, 0));
        assert_eq!(action.steps.len(), 2, "the move, then the filing");

        board.apply(&action);
        assert_eq!(board.moves(), 1, "one move, however much it did");
        assert!(board.pile(0).is_empty(), "the run left the tableau");
        assert!(board.pile(1).is_empty());
    }

    #[test]
    fn a_filed_run_shows_its_king_on_the_foundation() {
        let mut board = one_card_short();
        board.apply(&four_suits().action_for(&board.clone(), Move::new(1, 0, 0)));

        let home = board.pile(S.foundation());
        assert_eq!(home.len(), 13);
        assert_eq!(
            home.exposed(),
            Some(c(KING, Spades)),
            "turned over as filed"
        );
        assert!(home.is_complete_suit(), "so the shared check certifies it");
    }

    #[test]
    fn filing_a_run_turns_the_card_it_uncovers() {
        let mut cards = vec![c(4, Clubs)];
        cards.extend_from_slice(&run(Spades)[..12]);
        let mut board = board(
            &[
                Pile::tableau(1, &cards),
                Pile::tableau(0, &[c(ACE, Spades)]),
            ],
            &[],
        );

        let action = four_suits().action_for(&board, Move::new(1, 0, 0));
        assert!(
            action.steps[1].flip_source,
            "the run was all that covered it"
        );
        board.apply(&action);

        assert_eq!(board.pile(0).cards(), &[c(4, Clubs)]);
        assert_eq!(board.pile(0).face_down(), 0, "and it turned face up");
    }

    #[test]
    fn undoing_a_filed_run_puts_it_back_where_it_was() {
        let start = one_card_short();
        let mut board = start.clone();
        let action = four_suits().action_for(&board, Move::new(1, 0, 0));

        board.apply(&action);
        board.undo(&action);
        assert_eq!(board, start, "one undo takes back the whole thing");
    }

    #[test]
    fn redo_replays_a_filed_run_exactly() {
        let mut board = one_card_short();
        let action = four_suits().action_for(&board, Move::new(1, 0, 0));

        board.apply(&action);
        let filed = board.clone();
        board.undo(&action);
        board.redo(&action);
        assert_eq!(board, filed);
    }

    #[test]
    fn a_stock_deal_files_every_run_it_completes() {
        // Two columns a card short, and a deal that finishes both at once.
        let mut columns = vec![
            Pile::tableau(0, &run(Spades)[..12]),
            Pile::tableau(0, &run(Hearts)[..12]),
        ];
        columns.resize(S.column_count(), Pile::tableau(0, &[c(7, Diamonds)]));

        // Cards come off the end of the stock, so the last one lands first.
        let mut stock = vec![c(7, Clubs); 8];
        stock.push(c(ACE, Hearts));
        stock.push(c(ACE, Spades));

        let start = board(&columns, &stock);
        let mut board = start.clone();
        let action = four_suits().activate(&board, S.stock()).expect("deals");
        assert_eq!(action.steps.len(), 12, "ten cards dealt, two runs filed");

        board.apply(&action);
        assert_eq!(board.moves(), 1);
        assert!(board.pile(0).is_empty() && board.pile(1).is_empty());
        assert!(board.pile(S.foundation()).is_complete_suit());
        assert!(board.pile(S.foundation() + 1).is_complete_suit());

        board.undo(&action);
        assert_eq!(board, start, "and it is still one undo");
    }

    #[test]
    fn runs_go_to_the_foundations_in_board_order() {
        let mut board = one_card_short();
        board.apply(&four_suits().action_for(&board.clone(), Move::new(1, 0, 0)));
        assert!(
            !board.pile(S.foundation()).is_empty(),
            "the first one first"
        );
        assert!(board.pile(S.foundation() + 1).is_empty());
    }

    // -- the stock ---------------------------------------------------------

    #[test]
    fn the_stock_deals_one_card_onto_every_column() {
        let start = four_suits().deal(42);
        let mut board = start.clone();
        let action = four_suits().activate(&board, S.stock()).expect("deals");

        board.apply(&action);
        assert_eq!(board.pile(S.stock()).len(), 40, "ten cards left the stock");
        for column in 0..S.column_count() {
            let pile = board.pile(column);
            assert!(pile.is_face_up(pile.len() - 1), "column {column} shows it");
        }

        board.undo(&action);
        assert_eq!(board, start, "and the stock keeps its order");
    }

    #[test]
    fn the_stock_will_not_deal_onto_an_empty_column() {
        let mut columns = vec![Pile::tableau(0, &[])];
        columns.resize(S.column_count(), Pile::tableau(0, &[c(7, Diamonds)]));
        let blocked = board(&columns, &[c(3, Clubs); 10]);

        assert!(four_suits().activate(&blocked, S.stock()).is_none());
        assert_eq!(
            four_suits().stock_refusal(&blocked),
            "Fill every empty column before dealing.",
            "and it says which rule stopped it",
        );

        columns[0] = Pile::tableau(0, &[c(7, Diamonds)]);
        let filled = board(&columns, &[c(3, Clubs); 10]);
        assert!(four_suits().activate(&filled, S.stock()).is_some());
    }

    #[test]
    fn an_empty_stock_deals_nothing() {
        let mut columns = Vec::new();
        columns.resize(S.column_count(), Pile::tableau(0, &[c(7, Diamonds)]));
        let spent = board(&columns, &[]);

        assert!(four_suits().activate(&spent, S.stock()).is_none());
        assert_eq!(
            four_suits().stock_refusal(&spent),
            "The stock has nothing left to give.",
        );
    }

    #[test]
    fn dead_positions_report_no_progress() {
        // Ten lone cards of one rank: nothing lands on anything.
        let mut columns = Vec::new();
        columns.resize(S.column_count(), Pile::tableau(0, &[c(5, Spades)]));

        let stuck = board(&columns, &[]);
        assert!(!four_suits().has_progress_move(&stuck));

        let waiting = board(&columns, &[c(3, Clubs); 10]);
        assert!(
            four_suits().has_progress_move(&waiting),
            "a stock still to come is always progress",
        );

        // An empty column looks like room, but the only move into it is the
        // pointless one — and it is what stops the stock dealing.
        columns[0] = Pile::tableau(0, &[]);
        let cornered = board(&columns, &[c(3, Clubs); 10]);
        assert!(
            !four_suits().has_progress_move(&cornered),
            "a stock that will not deal is not progress",
        );
    }

    // -- winning, and the scoreboard ---------------------------------------

    #[test]
    fn eight_finished_runs_are_a_win() {
        let mut piles: Vec<Pile> = Vec::new();
        piles.resize(S.column_count(), Pile::tableau(0, &[]));
        piles.extend(
            Suit::ALL
                .iter()
                .flat_map(|&suit| [filed(suit), filed(suit)]),
        );
        piles.push(Pile::new(PileKind::Stock));
        let board = Board::new(0, piles.clone());
        assert!(four_suits().is_won(&board));

        piles[S.foundation()] = Pile::new(PileKind::Foundation);
        assert!(
            !four_suits().is_won(&Board::new(0, piles)),
            "seven is not it"
        );
    }

    #[test]
    fn the_status_line_counts_the_runs_and_the_deals_left() {
        let mut board = four_suits().deal(42);
        let status = four_suits().status(&board);
        assert_eq!(status[0].value, "0/8");
        assert_eq!(status[1].value, "5");

        board.apply(&four_suits().activate(&board.clone(), S.stock()).unwrap());
        assert_eq!(four_suits().status(&board)[1].value, "4");
    }

    // -- hints -------------------------------------------------------------

    #[test]
    fn finishing_a_run_is_suggested_first() {
        let mut columns = vec![
            Pile::tableau(0, &run(Spades)[..12]),
            Pile::tableau(0, &[c(ACE, Spades)]),
            // A card-turner, which is the next best thing there is.
            Pile::tableau(1, &[c(4, Clubs), c(6, Hearts)]),
            Pile::tableau(0, &[c(7, Hearts)]),
        ];
        columns.resize(S.column_count(), Pile::tableau(0, &[c(2, Diamonds)]));
        let board = board(&columns, &[]);

        assert_eq!(
            four_suits().suggestions(&board).first(),
            Some(&Move::new(1, 0, 0)),
            "the Ace that finishes the run",
        );
    }

    #[test]
    fn a_move_that_turns_a_card_over_outranks_one_that_does_not() {
        let board = board(
            &[
                Pile::tableau(1, &[c(4, Clubs), c(6, Hearts)]),
                Pile::tableau(0, &[c(6, Spades)]),
                Pile::tableau(0, &[c(7, Hearts)]),
            ],
            &[],
        );
        // Both 6s can go home on the 7♥; only one of them uncovers anything.
        assert!(rank(&board, Move::new(0, 1, 2)) > rank(&board, Move::new(1, 0, 2)));
    }

    #[test]
    fn breaking_a_suited_run_is_ranked_below_leaving_it_alone() {
        let joined = board(
            &[
                Pile::tableau(0, &[c(10, Spades), c(9, Spades)]),
                Pile::tableau(0, &[c(10, Hearts)]),
            ],
            &[],
        );
        let loose = board(
            &[
                Pile::tableau(0, &[c(10, Spades), c(9, Hearts)]),
                Pile::tableau(0, &[c(10, Clubs)]),
            ],
            &[],
        );
        assert!(
            rank(&joined, Move::new(0, 1, 1)) < rank(&loose, Move::new(0, 1, 1)),
            "pulling the 9♠ off the 10♠ undoes work already done",
        );
    }

    #[test]
    fn sliding_a_whole_column_into_an_empty_one_is_not_suggested() {
        let board = board(&[Pile::tableau(0, &[c(9, Spades), c(8, Spades)])], &[]);
        assert_eq!(check(&board, Move::new(0, 0, 1)), Ok(()), "it is legal");
        assert!(
            !four_suits()
                .suggestions(&board)
                .contains(&Move::new(0, 0, 1)),
            "but it only renames two piles",
        );
    }

    // -- the long walks ----------------------------------------------------

    #[test]
    fn undo_round_trips_a_played_game() {
        // The one-suit game, deliberately: it finishes runs often, so the walk
        // covers filing steps and the flips they cause as well as plain moves.
        let game = spider();
        let start = game.deal(2024);
        let mut board = start.clone();
        let mut history = Vec::new();

        for step in 0..400 {
            let moves = game.suggestions(&board);
            let action = if moves.is_empty() {
                match game.activate(&board, S.stock()) {
                    Some(action) => action,
                    None => break,
                }
            } else {
                // Deterministic but varied, so the walk explores flips,
                // empties, suited runs and filings rather than one move.
                game.action_for(&board, moves[(step * 7 + 3) % moves.len()])
            };
            board.apply(&action);
            history.push(action);
        }

        assert!(history.len() > 30, "played {} moves", history.len());
        while let Some(action) = history.pop() {
            board.undo(&action);
        }
        assert_eq!(board, start);
    }

    #[test]
    fn a_played_game_never_gains_or_loses_a_card() {
        let game = spider();
        let mut board = game.deal(11);
        let opening = census(&board);
        assert_eq!(opening.len(), 104);

        for step in 0..200 {
            let moves = game.suggestions(&board);
            let action = if moves.is_empty() {
                match game.activate(&board, S.stock()) {
                    Some(action) => action,
                    None => break,
                }
            } else {
                game.action_for(&board, moves[(step * 5 + 1) % moves.len()])
            };
            board.apply(&action);
            assert_eq!(census(&board), opening, "after move {step}");
        }
    }

    #[test]
    fn the_deal_is_pinned_to_its_seed() {
        // Seeds are a product guarantee: `--seed 42` deals what it has always
        // dealt, at every suit count.
        let board = four_suits().deal(42);
        assert_eq!(
            board.pile(0).cards(),
            &[
                c(12, Hearts),
                c(6, Spades),
                c(9, Spades),
                c(5, Diamonds),
                c(ACE, Hearts),
                c(10, Hearts),
            ]
        );
        assert_eq!(board.pile(9).exposed(), Some(c(2, Diamonds)));
        // The one-suit game is the same deal, repainted.
        assert_eq!(spider().deal(42).pile(9).exposed(), Some(c(2, Spades)));
        assert_eq!(board.pile(S.stock()).len(), 50);
        assert_eq!(four_suits().deal(42), four_suits().deal(42));
    }
}
