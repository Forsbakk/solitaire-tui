//! Klondike — the game most people mean when they say "solitaire".
//!
//! * Seven tableau piles dealt 1 to 7 cards, only the last of each face up.
//!   The remaining 24 cards are the stock.
//! * Build the tableau **down in alternating colour**. A run may move as a unit
//!   only if it is already in order; a King-headed run may take an empty pile.
//! * Four foundations build **up in suit from the Ace**. Cards may be played
//!   back off them.
//! * Turning the stock moves cards to the waste, where only the top one plays.
//!   An empty stock can be refilled from the waste, subject to the redeal rule.
//! * You win when all four foundations run Ace to King.

use crate::board::{
    Action, Board, BoardSpec, Fan, Move, MoveError, Pile, PileId, PileKind, Spot, Step,
};
use crate::card::{ACE, Card, KING, shuffled_deck};
use crate::rules::{OptionSpec, Rules, StatusItem, StatusTone};

const TABLEAU: usize = 7;
const FOUNDATIONS: usize = 4;
/// The first foundation; the rest follow it.
const FOUNDATION: PileId = TABLEAU;
const STOCK: PileId = TABLEAU + FOUNDATIONS;
const WASTE: PileId = STOCK + 1;

const OPTIONS: [OptionSpec; 2] = [
    OptionSpec {
        label: "Draw",
        values: &["3", "1"],
    },
    OptionSpec {
        label: "Redeals",
        values: &["unlimited", "3", "none"],
    },
];

/// How many cards each `Draw` value turns.
const DRAW_COUNTS: [usize; 2] = [3, 1];
/// How many redeals each `Redeals` value allows; `None` is unlimited.
const REDEAL_LIMITS: [Option<u32>; 3] = [None, Some(3), Some(0)];

/// The first value of every option is its default, so a fresh `Klondike` is the
/// traditional game: three cards at a time, going round as often as you like.
#[derive(Default)]
pub struct Klondike {
    /// Which value each option in [`OPTIONS`] sits on.
    selected: [usize; OPTIONS.len()],
}

impl Klondike {
    fn draw_count(&self) -> usize {
        DRAW_COUNTS[self.selected[0]]
    }

    fn redeal_limit(&self) -> Option<u32> {
        REDEAL_LIMITS[self.selected[1]]
    }

    fn foundations() -> impl Iterator<Item = PileId> {
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
        Self::foundations().find(|&pile| Self::foundation_takes(board, pile, card))
    }

    /// Whether `cards` run down in alternating colour, so they travel together.
    fn is_sequence(cards: &[Card]) -> bool {
        cards.windows(2).all(|pair| {
            pair[1].rank + 1 == pair[0].rank && pair[1].suit.is_red() != pair[0].suit.is_red()
        })
    }

    /// Whether the waste can be turned back into the stock.
    fn can_recycle(&self, board: &Board) -> bool {
        board.pile(STOCK).is_empty()
            && !board.pile(WASTE).is_empty()
            && self
                .redeal_limit()
                .is_none_or(|limit| board.recycles() < limit)
    }
}

impl Rules for Klondike {
    fn name(&self) -> &'static str {
        "Klondike"
    }

    fn blurb(&self) -> &'static str {
        "Build the tableau down in alternating colour, moving ordered runs as a \
         unit. Only a King may take an empty pile.\n\n\
         Turn the stock for more cards, and build the four foundations up in \
         suit from the Ace. Win by sending all 52 cards home."
    }

    fn help(&self) -> &'static str {
        "  d             turn the stock (space on the stock does it too)\n\
         \x20 f             send a card home, held or under the cursor\n\
         \x20 A             finish the game once every card is free"
    }

    fn deal(&self, seed: u64) -> Board {
        let mut deck = shuffled_deck(seed).into_iter();
        let mut piles: Vec<Pile> = (0..TABLEAU)
            .map(|i| {
                let cards: Vec<_> = deck.by_ref().take(i + 1).collect();
                // Every card but the last starts face down.
                Pile::tableau(i, &cards)
            })
            .collect();

        piles.extend((0..FOUNDATIONS).map(|_| Pile::new(PileKind::Foundation)));
        let stock: Vec<_> = deck.collect();
        piles.push(Pile::holding(PileKind::Stock, stock.len(), &stock));
        piles.push(Pile::new(PileKind::Waste));
        Board::new(seed, piles)
    }

    fn spec(&self, _board: &Board) -> BoardSpec {
        // The waste fans rightwards so a draw of three can be read at a glance,
        // even though only the card on top of it is in play. It spreads into the
        // empty slot beside it, so it costs the board no extra width.
        let mut spots = vec![
            Spot::new(STOCK, 0, 0, Fan::None),
            Spot::new(WASTE, 0, 1, Fan::Right(self.draw_count())),
        ];
        // A gap at slot 2 keeps the stock and the foundations apart, which is
        // how every physical layout of this game reads.
        spots.extend(
            Self::foundations()
                .enumerate()
                .map(|(i, pile)| Spot::new(pile, 0, 3 + i as u16, Fan::None)),
        );
        spots.extend((0..TABLEAU).map(|i| Spot::new(i, 1, i as u16, Fan::Down)));
        BoardSpec::new(spots)
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

    fn check(&self, board: &Board, mv: Move) -> Result<(), MoveError> {
        let Move { from, index, to } = mv;
        if from == to {
            return Err(MoveError::SamePile);
        }
        let source = board.pile(from);
        let target = board.pile(to);

        // The stock is turned, never picked from, and nothing is ever put on
        // the stock or the waste by hand.
        if source.kind() == PileKind::Stock
            || matches!(target.kind(), PileKind::Stock | PileKind::Waste)
        {
            return Err(MoveError::WrongTarget);
        }
        if index >= source.len() {
            return Err(MoveError::NoSuchCard);
        }
        if !source.is_face_up(index) {
            return Err(MoveError::FaceDown);
        }

        let last = source.len() - 1;
        // Only the top card of the waste or a foundation is in play.
        if matches!(source.kind(), PileKind::Waste | PileKind::Foundation) && index != last {
            return Err(MoveError::NotExposed);
        }

        let head = source.cards()[index];
        if target.kind() == PileKind::Foundation {
            if source.kind() == PileKind::Foundation {
                return Err(MoveError::WrongTarget);
            }
            if index != last {
                // Foundations take one card at a time, never a run.
                return Err(MoveError::NotExposed);
            }
            return if Self::foundation_takes(board, to, head) {
                Ok(())
            } else {
                Err(MoveError::Mismatch)
            };
        }

        // Landing on the tableau: the cards must already be in order.
        if !Self::is_sequence(&source.cards()[index..]) {
            return Err(MoveError::NotASequence);
        }
        match target.exposed() {
            Some(card)
                if card.rank == head.rank + 1 && card.suit.is_red() != head.suit.is_red() =>
            {
                Ok(())
            }
            Some(_) => Err(MoveError::Mismatch),
            None if head.rank == KING => Ok(()),
            None => Err(MoveError::NotKing),
        }
    }

    /// Turning the stock, or refilling it from the waste when it runs out.
    fn activate(&self, board: &Board, pile: PileId) -> Option<Action> {
        if pile != STOCK {
            return None;
        }
        let stock = board.pile(STOCK);
        if !stock.is_empty() {
            // Turned as a packet, so the last card off the stock ends up on
            // top of the waste — and so a recycled pass repeats exactly.
            return Some(Action::single(Step {
                from: STOCK,
                to: WASTE,
                count: self.draw_count().min(stock.len()),
                reverse: true,
                was_face_up: false,
                face_up: true,
                flip_source: false,
                recycle: false,
            }));
        }
        self.can_recycle(board).then(|| {
            Action::single(Step {
                from: WASTE,
                to: STOCK,
                count: board.pile(WASTE).len(),
                reverse: true,
                was_face_up: true,
                face_up: false,
                flip_source: false,
                recycle: true,
            })
        })
    }

    fn send_home(&self, board: &Board, from: PileId) -> Option<Action> {
        let source = board.pile(from);
        if matches!(source.kind(), PileKind::Stock | PileKind::Foundation) {
            return None;
        }
        let card = source.exposed()?;
        let home = Self::home_for(board, card)?;
        Some(Action::moving(board, from, source.len() - 1, home))
    }

    /// Once nothing is hidden and the stock is spent, the rest is bookkeeping.
    fn autoplay(&self, board: &Board) -> Option<Action> {
        if board.face_down_count() > 0
            || !board.pile(STOCK).is_empty()
            || !board.pile(WASTE).is_empty()
            || self.is_won(board)
        {
            return None;
        }
        // The lowest card that will go home, so the foundations climb evenly.
        (0..TABLEAU)
            .filter_map(|pile| {
                let card = board.pile(pile).exposed()?;
                let home = Self::home_for(board, card)?;
                Some((card.rank, pile, home))
            })
            .min()
            .map(|(_, pile, home)| Action::moving(board, pile, board.pile(pile).len() - 1, home))
    }

    fn is_won(&self, board: &Board) -> bool {
        Self::foundations().all(|pile| board.pile(pile).is_complete_suit())
    }

    fn is_useful(&self, board: &Board, mv: Move) -> bool {
        // Taking a card back off a foundation is legal but never the move to
        // suggest, and sliding a whole pile into an empty one gains nothing.
        // An empty *foundation* is a different matter: a lone Ace on the first
        // tableau pile belongs on it, and that move is also index 0 into an
        // empty pile.
        board.pile(mv.from).kind() != PileKind::Foundation
            && !(mv.index == 0
                && board.pile(mv.to).kind() == PileKind::Tableau
                && board.pile(mv.to).is_empty()
                && board.pile(mv.from).kind() == PileKind::Tableau)
    }

    fn rank(&self, board: &Board, mv: Move) -> i32 {
        let Move { from, index, to } = mv;
        let source = board.pile(from);
        let mut value = 0;

        // Turning a card over is what opens a Klondike deal up.
        if index > 0 && index == source.face_down() {
            value += 100;
        }
        if board.pile(to).kind() == PileKind::Foundation {
            value += 60;
        }
        // Clearing the waste keeps the cards beneath it reachable.
        if source.kind() == PileKind::Waste {
            value += 40;
        }
        // Emptying a tableau pile makes somewhere for a King.
        if index == 0 && source.kind() == PileKind::Tableau {
            value += 30;
        }
        if board.pile(to).is_empty() && board.pile(to).kind() == PileKind::Tableau {
            value -= 25;
        }
        value - (source.len() - index) as i32
    }

    fn has_progress_move(&self, board: &Board) -> bool {
        !board.pile(STOCK).is_empty()
            || self.can_recycle(board)
            || !self.suggestions(board).is_empty()
    }

    fn status(&self, board: &Board) -> Vec<StatusItem> {
        let mut items = vec![StatusItem::new(
            "home",
            format!(
                "{}/52",
                Self::foundations()
                    .map(|pile| board.pile(pile).len())
                    .sum::<usize>()
            ),
            StatusTone::Good,
        )];
        if let Some(limit) = self.redeal_limit() {
            let left = limit.saturating_sub(board.recycles());
            items.push(StatusItem::new(
                "redeals",
                left.to_string(),
                if left > 0 {
                    StatusTone::Good
                } else {
                    StatusTone::Muted
                },
            ));
        }
        items
    }
}

/// Positions other modules' tests need to build.
#[cfg(test)]
pub mod tests_support {
    use super::*;
    use crate::card::Suit;

    /// A board that is already decided: four suits stacked King-down in the
    /// tableau with nothing hidden and no stock, so autoplay can file all 52.
    pub fn decided_board() -> Board {
        let mut piles: Vec<Pile> = Suit::ALL
            .into_iter()
            .map(|suit| {
                let cards: Vec<Card> = (0..13).map(|i| Card::new(13 - i, suit)).collect();
                Pile::tableau(0, &cards)
            })
            .collect();
        piles.resize(TABLEAU, Pile::tableau(0, &[]));
        piles.extend((0..FOUNDATIONS).map(|_| Pile::new(PileKind::Foundation)));
        piles.push(Pile::new(PileKind::Stock));
        piles.push(Pile::new(PileKind::Waste));
        Board::new(0, piles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Suit;
    use crate::card::Suit::{Clubs, Diamonds, Hearts, Spades};
    use std::collections::HashSet;

    fn c(rank: u8, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    fn klondike() -> Klondike {
        Klondike::default()
    }

    fn draw_one() -> Klondike {
        let mut game = klondike();
        game.configure(0, 1);
        assert_eq!(game.draw_count(), 1);
        game
    }

    /// A board built pile by pile: tableau piles, then foundations, then the
    /// stock and waste.
    fn board(tableau: &[Pile], foundations: &[Pile], stock: &[Card], waste: &[Card]) -> Board {
        let mut piles: Vec<Pile> = tableau.to_vec();
        piles.resize(TABLEAU, Pile::tableau(0, &[]));
        piles.extend_from_slice(foundations);
        piles.resize(TABLEAU + FOUNDATIONS, Pile::new(PileKind::Foundation));
        piles.push(Pile::holding(PileKind::Stock, stock.len(), stock));
        piles.push(Pile::holding(PileKind::Waste, 0, waste));
        Board::new(0, piles)
    }

    fn foundation(cards: &[Card]) -> Pile {
        Pile::holding(PileKind::Foundation, 0, cards)
    }

    fn suit_up_to(suit: Suit, top: u8) -> Vec<Card> {
        (ACE..=top).map(|rank| c(rank, suit)).collect()
    }

    #[test]
    fn deal_has_the_right_shape() {
        let board = klondike().deal(42);

        let mut seen = HashSet::new();
        for i in 0..TABLEAU {
            let pile = board.pile(i);
            assert_eq!(pile.len(), i + 1, "pile {i} is dealt {} cards", i + 1);
            assert_eq!(
                pile.first_face_up(),
                Some(i),
                "only the last card of pile {i} is face up"
            );
            seen.extend(pile.cards().iter().copied());
        }
        for pile in Klondike::foundations() {
            assert!(board.pile(pile).is_empty());
        }
        assert_eq!(board.pile(STOCK).len(), 24);
        assert_eq!(board.pile(STOCK).face_down(), 24, "the stock is hidden");
        assert!(board.pile(WASTE).is_empty());

        seen.extend(board.pile(STOCK).cards().iter().copied());
        assert_eq!(seen.len(), 52, "all 52 cards dealt exactly once");
    }

    #[test]
    fn the_tableau_builds_down_in_alternating_colour() {
        let board = board(
            &[
                Pile::tableau(0, &[c(10, Spades)]),
                Pile::tableau(0, &[c(9, Hearts)]),
                Pile::tableau(0, &[c(9, Clubs)]),
                Pile::tableau(0, &[c(8, Hearts)]),
            ],
            &[],
            &[],
            &[],
        );
        let check = |from, index, to| klondike().check(&board, Move::new(from, index, to));

        assert_eq!(check(1, 0, 0), Ok(()), "red nine onto a black ten");
        assert_eq!(check(2, 0, 0), Err(MoveError::Mismatch), "same colour");
        assert_eq!(check(3, 0, 0), Err(MoveError::Mismatch), "wrong rank");
    }

    #[test]
    fn only_a_king_may_take_an_empty_pile() {
        let board = board(
            &[
                Pile::tableau(0, &[c(KING, Spades), c(12, Hearts)]),
                Pile::tableau(0, &[c(5, Diamonds)]),
            ],
            &[],
            &[],
            &[],
        );
        let check = |from, index, to| klondike().check(&board, Move::new(from, index, to));

        assert_eq!(check(0, 0, 6), Ok(()), "a King-headed run");
        assert_eq!(check(0, 1, 6), Err(MoveError::NotKing));
        assert_eq!(check(1, 0, 6), Err(MoveError::NotKing));
    }

    #[test]
    fn a_run_moves_as_a_unit_only_when_it_is_already_in_order() {
        let ordered = board(
            &[
                Pile::tableau(0, &[c(9, Hearts), c(8, Spades), c(7, Diamonds)]),
                Pile::tableau(0, &[c(10, Spades)]),
            ],
            &[],
            &[],
            &[],
        );
        assert_eq!(klondike().check(&ordered, Move::new(0, 0, 1)), Ok(()));

        // The same cards, but the middle one breaks the alternation.
        let jumbled = board(
            &[
                Pile::tableau(0, &[c(9, Hearts), c(8, Hearts), c(7, Diamonds)]),
                Pile::tableau(0, &[c(10, Spades)]),
                Pile::tableau(0, &[c(8, Spades)]),
            ],
            &[],
            &[],
            &[],
        );
        assert_eq!(
            klondike().check(&jumbled, Move::new(0, 0, 1)),
            Err(MoveError::NotASequence),
            "unlike Scorpion, a jumbled tail cannot travel"
        );
        // Its tip alone is a run of one, so that still moves.
        assert_eq!(klondike().check(&jumbled, Move::new(0, 2, 2)), Ok(()));
    }

    #[test]
    fn foundations_build_up_in_suit_from_the_ace() {
        let board = board(
            &[
                Pile::tableau(0, &[c(ACE, Spades)]),
                Pile::tableau(0, &[c(2, Spades)]),
                Pile::tableau(0, &[c(2, Hearts)]),
                Pile::tableau(0, &[c(4, Spades)]),
            ],
            &[foundation(&[])],
            &[],
            &[],
        );
        let check = |from, to| klondike().check(&board, Move::new(from, 0, to));

        assert_eq!(check(0, FOUNDATION), Ok(()), "an Ace starts one");
        assert_eq!(check(1, FOUNDATION), Err(MoveError::Mismatch), "not an Ace");

        let started = board_with_spade_ace();
        let check = |from, to| klondike().check(&started, Move::new(from, 0, to));
        assert_eq!(check(1, FOUNDATION), Ok(()), "the two follows the ace");
        assert_eq!(check(2, FOUNDATION), Err(MoveError::Mismatch), "wrong suit");
        assert_eq!(
            check(3, FOUNDATION),
            Err(MoveError::Mismatch),
            "skips a rank"
        );
    }

    fn board_with_spade_ace() -> Board {
        board(
            &[
                Pile::tableau(0, &[c(ACE, Spades)]),
                Pile::tableau(0, &[c(2, Spades)]),
                Pile::tableau(0, &[c(2, Hearts)]),
                Pile::tableau(0, &[c(4, Spades)]),
            ],
            &[foundation(&[c(ACE, Spades)])],
            &[],
            &[],
        )
    }

    #[test]
    fn a_foundation_takes_one_card_at_a_time_and_gives_them_back() {
        let board = board(
            &[Pile::tableau(0, &[c(3, Hearts), c(2, Spades)])],
            &[foundation(&[c(ACE, Spades)]), foundation(&[])],
            &[],
            &[],
        );
        // The 2♠ alone goes home; the pair does not.
        assert_eq!(
            klondike().check(&board, Move::new(0, 1, FOUNDATION)),
            Ok(())
        );
        assert_eq!(
            klondike().check(&board, Move::new(0, 0, FOUNDATION)),
            Err(MoveError::NotExposed)
        );
        // And a card may come back off onto the tableau.
        assert_eq!(
            klondike().check(&board, Move::new(FOUNDATION, 0, 0)),
            Err(MoveError::Mismatch),
            "the ace does not fit on the 2♠"
        );
        assert_eq!(
            klondike().check(&board, Move::new(FOUNDATION, 0, FOUNDATION + 1)),
            Err(MoveError::WrongTarget),
            "and never shuffles between foundations"
        );
    }

    #[test]
    fn nothing_is_taken_from_the_stock_or_put_back_on_it_by_hand() {
        let board = klondike().deal(42);
        let check = |from, index, to| klondike().check(&board, Move::new(from, index, to));

        assert_eq!(check(STOCK, 23, 0), Err(MoveError::WrongTarget));
        assert_eq!(check(0, 0, STOCK), Err(MoveError::WrongTarget));
        assert_eq!(check(0, 0, WASTE), Err(MoveError::WrongTarget));
    }

    #[test]
    fn only_the_top_of_the_waste_is_in_play() {
        let board = board(
            &[Pile::tableau(0, &[c(5, Spades)])],
            &[],
            &[],
            &[c(9, Hearts), c(4, Hearts)],
        );
        assert_eq!(klondike().check(&board, Move::new(WASTE, 1, 0)), Ok(()));
        assert_eq!(
            klondike().check(&board, Move::new(WASTE, 0, 0)),
            Err(MoveError::NotExposed)
        );
    }

    #[test]
    fn turning_the_stock_moves_cards_to_the_waste_face_up() {
        let game = klondike();
        let start = game.deal(42);
        let mut board = start.clone();

        let action = game.activate(&board, STOCK).expect("the stock is full");
        board.apply(&action);
        assert_eq!(board.pile(STOCK).len(), 21);
        assert_eq!(board.pile(WASTE).len(), 3, "draw three");
        assert_eq!(board.pile(WASTE).face_down(), 0, "and face up");
        assert_eq!(board.pile(STOCK).face_down(), 21, "the rest stays hidden");

        board.undo(&action);
        assert_eq!(board, start);
    }

    #[test]
    fn draw_one_turns_a_single_card() {
        let game = draw_one();
        let mut board = game.deal(42);
        board.apply(&game.activate(&board, STOCK).unwrap());
        assert_eq!(board.pile(WASTE).len(), 1);
    }

    #[test]
    fn a_recycled_stock_deals_exactly_the_same_cards_again() {
        let game = klondike();
        let mut board = game.deal(42);
        let opening = board.pile(STOCK).cards().to_vec();

        // Empty the stock into the waste, then put it back.
        let mut first_pass = Vec::new();
        while !board.pile(STOCK).is_empty() {
            let action = game.activate(&board, STOCK).unwrap();
            board.apply(&action);
            first_pass.push(board.pile(WASTE).exposed().unwrap());
        }
        let recycle = game.activate(&board, STOCK).expect("the waste goes back");
        board.apply(&recycle);

        assert!(board.pile(WASTE).is_empty());
        assert_eq!(board.pile(STOCK).cards(), opening, "the original order");
        assert_eq!(board.pile(STOCK).face_down(), 24, "face down again");
        assert_eq!(board.recycles(), 1, "and it cost a redeal");

        let mut second_pass = Vec::new();
        while !board.pile(STOCK).is_empty() {
            let action = game.activate(&board, STOCK).unwrap();
            board.apply(&action);
            second_pass.push(board.pile(WASTE).exposed().unwrap());
        }
        assert_eq!(first_pass, second_pass, "every turn repeats");
    }

    #[test]
    fn undoing_a_recycle_hands_the_redeal_back() {
        let game = klondike();
        let mut board = game.deal(42);
        while !board.pile(STOCK).is_empty() {
            let action = game.activate(&board, STOCK).unwrap();
            board.apply(&action);
        }
        let before = board.clone();

        let recycle = game.activate(&board, STOCK).unwrap();
        board.apply(&recycle);
        assert_eq!(board.recycles(), 1);

        board.undo(&recycle);
        assert_eq!(board, before, "including the redeal count");
        assert_eq!(board.recycles(), 0);
    }

    #[test]
    fn the_redeal_rule_is_enforced() {
        let mut game = klondike();
        game.configure(1, 2); // Redeals: none
        assert_eq!(game.redeal_limit(), Some(0));

        let empty_stock = board(&[], &[], &[], &[c(9, Hearts)]);
        assert!(
            game.activate(&empty_stock, STOCK).is_none(),
            "with no redeals the waste stays put"
        );

        let mut game = klondike();
        game.configure(1, 1); // Redeals: 3
        assert_eq!(game.redeal_limit(), Some(3));
        assert!(game.activate(&empty_stock, STOCK).is_some());

        assert!(
            klondike().activate(&empty_stock, STOCK).is_some(),
            "unlimited by default"
        );
    }

    #[test]
    fn an_empty_stock_with_an_empty_waste_does_nothing() {
        let board = board(&[], &[], &[], &[]);
        assert!(klondike().activate(&board, STOCK).is_none());
    }

    #[test]
    fn send_home_finds_the_right_foundation() {
        let board = board(
            &[Pile::tableau(0, &[c(2, Spades)])],
            &[foundation(&[c(ACE, Spades)]), foundation(&[c(ACE, Hearts)])],
            &[],
            &[c(2, Hearts)],
        );
        let game = klondike();

        let mut after = board.clone();
        after.apply(&game.send_home(&board, 0).expect("the 2♠ has a home"));
        assert_eq!(after.pile(FOUNDATION).len(), 2);

        let mut after = board.clone();
        after.apply(&game.send_home(&board, WASTE).expect("the 2♥ has a home"));
        assert_eq!(after.pile(FOUNDATION + 1).len(), 2);

        // A card with nowhere to go, and a pile that is never a source.
        let stuck = board_with_no_home();
        assert!(game.send_home(&stuck, 0).is_none());
        assert!(game.send_home(&board, STOCK).is_none());
        assert!(game.send_home(&board, FOUNDATION).is_none());
    }

    fn board_with_no_home() -> Board {
        board(&[Pile::tableau(0, &[c(9, Clubs)])], &[], &[], &[])
    }

    #[test]
    fn autoplay_only_runs_once_the_game_is_decided() {
        let game = klondike();
        assert!(
            game.autoplay(&game.deal(42)).is_none(),
            "not while cards are still hidden"
        );

        // Face-up tableau, empty stock and waste: everything can go home.
        let waiting = board(
            &[
                Pile::tableau(0, &[c(2, Spades)]),
                Pile::tableau(0, &[c(ACE, Spades)]),
            ],
            &[],
            &[],
            &[],
        );
        let action = game.autoplay(&waiting).expect("the ace can go");
        let mut after = waiting.clone();
        after.apply(&action);
        assert_eq!(
            after.pile(FOUNDATION).exposed(),
            Some(c(ACE, Spades)),
            "and it takes the lowest card first"
        );

        // Still blocked while the stock holds anything.
        let with_stock = board(
            &[Pile::tableau(0, &[c(ACE, Spades)])],
            &[],
            &[c(9, Hearts)],
            &[],
        );
        assert!(game.autoplay(&with_stock).is_none());
    }

    #[test]
    fn autoplay_drains_a_decided_board_to_a_win() {
        // Four piles, each a suit stacked King down to Ace so the Ace is on
        // top: the tableau equivalent of a solved game waiting to be filed.
        let tableau: Vec<Pile> = Suit::ALL
            .into_iter()
            .map(|suit| Pile::tableau(0, &(0..13).map(|i| c(13 - i, suit)).collect::<Vec<_>>()))
            .collect();
        let mut board = board(&tableau, &[], &[], &[]);
        let game = klondike();

        let mut steps = 0;
        while let Some(action) = game.autoplay(&board) {
            board.apply(&action);
            steps += 1;
            assert!(steps <= 52, "autoplay should terminate");
        }
        assert_eq!(steps, 52, "every card was filed");
        assert!(game.is_won(&board));
    }

    #[test]
    fn a_full_set_of_foundations_is_a_win() {
        let done: Vec<Pile> = Suit::ALL
            .into_iter()
            .map(|suit| foundation(&suit_up_to(suit, KING)))
            .collect();
        assert!(klondike().is_won(&board(&[], &done, &[], &[])));

        let mut nearly = done.clone();
        nearly[3] = foundation(&suit_up_to(Suit::ALL[3], 12));
        assert!(!klondike().is_won(&board(&[], &nearly, &[], &[])));
    }

    #[test]
    fn the_deal_is_pinned_to_its_seed() {
        let board = klondike().deal(42);
        assert_eq!(board.pile(0).exposed(), Some(c(7, Spades)));
        assert_eq!(board.pile(6).len(), 7);
        assert_eq!(board.pile(STOCK).len(), 24);
        assert_eq!(klondike().deal(42), klondike().deal(42));
    }

    #[test]
    fn hints_never_suggest_taking_a_card_back_off_a_foundation() {
        let board = board(
            &[Pile::tableau(0, &[c(2, Hearts)])],
            &[foundation(&[c(ACE, Spades)])],
            &[],
            &[],
        );
        // Legal, but backwards.
        assert_eq!(
            klondike().check(&board, Move::new(FOUNDATION, 0, 0)),
            Ok(())
        );
        assert!(
            !klondike()
                .suggestions(&board)
                .iter()
                .any(|mv| mv.from == FOUNDATION)
        );
    }

    #[test]
    fn a_move_that_turns_a_card_over_outranks_one_that_does_not() {
        let board = board(
            &[
                // Moving the 9♥ turns the card beneath it face up.
                Pile::tableau(1, &[c(4, Clubs), c(9, Hearts)]),
                Pile::tableau(0, &[c(10, Spades)]),
                // Whereas this just shuffles two face-up cards.
                Pile::tableau(0, &[c(5, Clubs), c(7, Hearts)]),
                Pile::tableau(0, &[c(8, Spades)]),
            ],
            &[],
            &[],
            &[],
        );
        assert_eq!(
            klondike().suggestions(&board).first(),
            Some(&Move::new(0, 1, 1))
        );
    }

    #[test]
    fn undo_round_trips_a_played_game() {
        let game = klondike();
        let start = game.deal(2024);
        let mut board = start.clone();
        let mut history = Vec::new();

        for step in 0..400 {
            let moves = game.suggestions(&board);
            if moves.is_empty() {
                match game.activate(&board, STOCK) {
                    Some(action) => {
                        board.apply(&action);
                        history.push(action);
                    }
                    None => break,
                }
                continue;
            }
            // Deterministic but varied, so the walk explores flips, runs,
            // foundation sends and the waste.
            let mv = moves[(step * 7 + 3) % moves.len()];
            let action = game.action_for(&board, mv);
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
    fn a_played_game_never_loses_or_duplicates_a_card() {
        let game = klondike();
        let mut board = game.deal(7);
        for step in 0..400 {
            let moves = game.suggestions(&board);
            let action = if moves.is_empty() {
                match game.activate(&board, STOCK) {
                    Some(action) => action,
                    None => break,
                }
            } else {
                game.action_for(&board, moves[(step * 5 + 1) % moves.len()])
            };
            board.apply(&action);

            let all: HashSet<Card> = board
                .piles()
                .iter()
                .flat_map(|pile| pile.cards().iter().copied())
                .collect();
            assert_eq!(all.len(), 52, "after {} moves", board.moves());
        }
    }

    #[test]
    fn progress_counts_the_stock_and_the_redeal() {
        let game = klondike();
        // Nothing to play, but cards still to turn.
        let with_stock = board(
            &[Pile::tableau(0, &[c(9, Clubs)])],
            &[],
            &[c(2, Hearts)],
            &[],
        );
        assert!(game.has_progress_move(&with_stock));

        // Nothing to play and nothing to turn, but the waste can go back.
        let recyclable = board(
            &[Pile::tableau(0, &[c(9, Clubs)])],
            &[],
            &[],
            &[c(4, Clubs)],
        );
        assert!(game.has_progress_move(&recyclable));

        let mut no_redeals = klondike();
        no_redeals.configure(1, 2);
        assert!(!no_redeals.has_progress_move(&recyclable), "truly stuck");
    }
}
