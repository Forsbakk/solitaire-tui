//! Yukon — Scorpion's free-tail lift played onto Klondike's foundations.
//!
//! * 52 cards dealt into 7 piles: one lone face-up card, then piles of 6 to 11
//!   cards with five face up and the rest buried. There is no stock — every
//!   card is on the table from the start.
//! * A move lifts a face-up card *and everything below it* (a "tail", in any
//!   order) onto the exposed card of another pile, which must be the opposite
//!   colour and exactly one rank higher.
//! * Only a King-headed tail may move to an empty pile.
//! * When a tail leaves, a newly exposed face-down card flips up.
//! * Four foundations build up in suit from the Ace. A card may be taken back
//!   off one.
//! * You win when all 52 cards are home.
//!
//! Russian Solitaire is this game built in suit instead of alternating colour,
//! so everything here but the [`Build`](super::Build) predicate is shared with
//! [`super::russian`].

use crate::board::{Action, Board, BoardSpec, Fan, Move, MoveError, Pile, PileId, PileKind, Spot};
use crate::card::{ACE, Card, KING, shuffled_deck};
use crate::rules::{Build, Rules, StatusItem, StatusTone};

const TABLEAU: usize = 7;
const FOUNDATIONS: usize = 4;
/// How many cards sit face up on every pile but the first.
const FACE_UP_PER_PILE: usize = 5;
/// The first foundation; the rest follow it.
const FOUNDATION: PileId = TABLEAU;
/// Cards in a deck, and so what a full set of foundations holds.
const DECK: usize = 52;

// -- shared with `russian` -------------------------------------------------
//
// Only `check` cares which game it is; the deal, the board, the foundations and
// the hint heuristic are identical, so they live here once.

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

pub(super) fn deal(seed: u64) -> Board {
    let mut deck = shuffled_deck(seed).into_iter();
    let mut piles: Vec<Pile> = (0..TABLEAU)
        .map(|i| {
            // The first pile is a lone face-up card; the rest bury `i` cards
            // under five face-up ones.
            let total = if i == 0 { 1 } else { i + FACE_UP_PER_PILE };
            let cards: Vec<_> = deck.by_ref().take(total).collect();
            Pile::tableau(i, &cards)
        })
        .collect();

    piles.extend((0..FOUNDATIONS).map(|_| Pile::new(PileKind::Foundation)));
    Board::new(seed, piles)
}

pub(super) fn spec() -> BoardSpec {
    // Slots 0-2 of the top row stay empty, so the foundations land exactly
    // where Klondike draws them and the two games' boards read alike.
    let mut spots: Vec<Spot> = foundations()
        .enumerate()
        .map(|(i, pile)| Spot::new(pile, 0, 3 + i as u16, Fan::None))
        .collect();
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
    if !source.is_face_up(index) {
        return Err(MoveError::FaceDown);
    }

    let last = source.len() - 1;
    // Only the card on top of a foundation is in play.
    if source.kind() == PileKind::Foundation && index != last {
        return Err(MoveError::NotExposed);
    }

    let head = source.cards()[index];
    if target.kind() == PileKind::Foundation {
        if source.kind() == PileKind::Foundation {
            return Err(MoveError::WrongTarget);
        }
        if index != last {
            // Foundations take one card at a time, never a tail.
            return Err(MoveError::NotExposed);
        }
        return if foundation_takes(board, to, head) {
            Ok(())
        } else {
            Err(MoveError::Mismatch)
        };
    }

    // Landing on the tableau. Unlike Klondike the tail need not be in order —
    // that free lift is what makes this Yukon.
    match target.exposed() {
        Some(card) if build.accepts(card, head) => Ok(()),
        Some(_) => Err(MoveError::Mismatch),
        None if head.rank == KING => Ok(()),
        None => Err(MoveError::NotKing),
    }
}

pub(super) fn is_won(board: &Board) -> bool {
    foundations().all(|pile| board.pile(pile).is_complete_suit())
}

pub(super) fn send_home(board: &Board, from: PileId) -> Option<Action> {
    let source = board.pile(from);
    if source.kind() != PileKind::Tableau {
        return None;
    }
    let card = source.exposed()?;
    let home = home_for(board, card)?;
    Some(Action::moving(board, from, source.len() - 1, home))
}

/// Once nothing is face down the rest is bookkeeping. With no stock to spend,
/// that is the whole condition.
pub(super) fn autoplay(board: &Board) -> Option<Action> {
    if board.face_down_count() > 0 || is_won(board) {
        return None;
    }
    // The lowest card that will go home, so the foundations climb evenly.
    (0..TABLEAU)
        .filter_map(|pile| {
            let card = board.pile(pile).exposed()?;
            Some((card.rank, pile, home_for(board, card)?))
        })
        .min()
        .map(|(_, pile, home)| Action::moving(board, pile, board.pile(pile).len() - 1, home))
}

pub(super) fn is_useful(board: &Board, mv: Move) -> bool {
    let to = board.pile(mv.to);
    // Taking a card back off a foundation is legal but never the move to
    // suggest, and sliding a whole pile into an empty one gains nothing. An
    // empty *foundation* is a different matter: a lone Ace belongs on it, and
    // the first pile of a Yukon deal is exactly one card.
    board.pile(mv.from).kind() != PileKind::Foundation
        && !(mv.index == 0 && to.kind() == PileKind::Tableau && to.is_empty())
}

pub(super) fn rank(board: &Board, mv: Move) -> i32 {
    let Move { from, index, to } = mv;
    let source = board.pile(from);
    let target = board.pile(to);
    let mut value = 0;

    // Turning a card over is what opens a Yukon deal up.
    if index > 0 && index == source.face_down() {
        value += 100;
    }
    if target.kind() == PileKind::Foundation {
        value += 60;
    }
    // Emptying a pile makes somewhere for a King.
    if index == 0 && source.kind() == PileKind::Tableau {
        value += 30;
    }
    // Filling an empty pile spends that scarce space.
    if target.kind() == PileKind::Tableau && target.is_empty() {
        value -= 25;
    }
    // All else equal, prefer disturbing fewer cards.
    value - (source.len() - index) as i32
}

pub(super) fn status(board: &Board) -> Vec<StatusItem> {
    let home: usize = foundations().map(|pile| board.pile(pile).len()).sum();
    vec![StatusItem::new(
        "home",
        format!("{home}/{DECK}"),
        StatusTone::Good,
    )]
}

/// The keys Yukon and Russian Solitaire add. Neither has a stock, so `d` is not
/// among them.
pub(super) const HELP: &str = "  f             send a card home, held or under the cursor\n\
                               \x20 A             finish the game once every card is free";

#[derive(Default)]
pub struct Yukon;

impl Rules for Yukon {
    fn name(&self) -> &'static str {
        "Yukon"
    }

    fn blurb(&self) -> &'static str {
        "Lift any face-up card together with every card below it — they travel \
         as a unit and need not be in order. Drop them on the opposite colour, \
         one rank higher. Only a King may take an empty pile.\n\n\
         There is no stock: all 52 cards are on the table from the start. Build \
         the four foundations up in suit from the Ace, and win by sending every \
         card home."
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::Suit;
    use crate::card::Suit::{Clubs, Diamonds, Hearts, Spades};
    use std::collections::HashSet;

    fn c(rank: u8, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    /// A board with hand-built piles, for testing specific positions.
    fn board(tableau: &[Pile], foundations: &[Pile]) -> Board {
        let mut piles: Vec<Pile> = tableau.to_vec();
        piles.resize(TABLEAU, Pile::tableau(0, &[]));
        piles.extend_from_slice(foundations);
        piles.resize(TABLEAU + FOUNDATIONS, Pile::new(PileKind::Foundation));
        Board::new(0, piles)
    }

    #[test]
    fn deal_has_the_right_shape() {
        let board = Yukon.deal(42);
        assert_eq!(board.piles().len(), TABLEAU + FOUNDATIONS);

        let mut seen = HashSet::new();
        for i in 0..TABLEAU {
            let pile = board.pile(i);
            assert_eq!(pile.len(), if i == 0 { 1 } else { i + FACE_UP_PER_PILE });
            assert_eq!(pile.face_down(), i, "pile {i} buries {i} cards");
            seen.extend(pile.cards().iter().copied());
        }
        assert_eq!(seen.len(), DECK, "all 52 cards dealt exactly once");

        for pile in foundations() {
            assert!(board.pile(pile).is_empty(), "foundations start empty");
        }
        assert!(
            board.first_of_kind(PileKind::Stock).is_none(),
            "Yukon has no stock"
        );
    }

    #[test]
    fn the_deal_is_pinned_to_its_seed() {
        // Seeds are a product guarantee: `--seed 42` deals what it has always
        // dealt, and this is what catches a refactor changing that quietly.
        let board = Yukon.deal(42);
        assert_eq!(board.pile(0).cards(), &[c(7, Spades)]);
        assert_eq!(
            board.pile(1).cards(),
            &[
                c(3, Diamonds),
                c(KING, Spades),
                c(ACE, Clubs),
                c(2, Clubs),
                c(ACE, Spades),
                c(ACE, Diamonds),
            ]
        );
        assert_eq!(
            board.pile(3).cards()[3],
            c(5, Clubs),
            "the first face-up card of pile 4"
        );
        assert_eq!(board.pile(6).exposed(), Some(c(10, Spades)));
    }

    #[test]
    fn a_tail_moves_regardless_of_its_own_order() {
        // 9♥ with junk below it: the whole tail travels together. This is the
        // move that makes Yukon Yukon rather than Klondike.
        let mut board = board(
            &[
                Pile::tableau(
                    0,
                    &[c(2, Clubs), c(9, Hearts), c(4, Spades), c(ACE, Diamonds)],
                ),
                Pile::tableau(0, &[c(10, Spades)]),
            ],
            &[],
        );

        let mv = Move::new(0, 1, 1);
        assert_eq!(Yukon.check(&board, mv), Ok(()));
        board.apply(&Yukon.action_for(&board, mv));
        assert_eq!(board.pile(0).cards(), &[c(2, Clubs)]);
        assert_eq!(
            board.pile(1).cards(),
            &[c(10, Spades), c(9, Hearts), c(4, Spades), c(ACE, Diamonds)]
        );
    }

    #[test]
    fn a_tail_lands_on_the_opposite_colour_one_rank_higher() {
        let board = board(
            &[
                Pile::tableau(2, &[c(3, Clubs), c(4, Clubs), c(9, Hearts)]),
                Pile::tableau(0, &[c(10, Spades)]),
                Pile::tableau(0, &[c(10, Diamonds)]),
                Pile::tableau(0, &[c(8, Spades)]),
                Pile::tableau(0, &[c(KING, Hearts)]),
            ],
            &[],
        );
        let check = |from, index, to| Yukon.check(&board, Move::new(from, index, to));

        assert_eq!(check(0, 2, 1), Ok(()), "red 9 onto a black 10");
        assert_eq!(check(0, 2, 2), Err(MoveError::Mismatch), "same colour");
        assert_eq!(check(0, 2, 3), Err(MoveError::Mismatch), "wrong rank");
        assert_eq!(check(0, 2, 0), Err(MoveError::SamePile));
        assert_eq!(check(0, 1, 1), Err(MoveError::FaceDown));
        assert_eq!(check(0, 9, 1), Err(MoveError::NoSuchCard));
        assert_eq!(
            check(4, 0, 3),
            Err(MoveError::Mismatch),
            "a King never lands on a card"
        );
    }

    #[test]
    fn only_a_king_may_take_an_empty_pile() {
        let board = board(
            &[
                Pile::tableau(0, &[c(KING, Spades), c(4, Clubs)]),
                Pile::tableau(0, &[c(2, Diamonds)]),
            ],
            &[],
        );
        let check = |from, index, to| Yukon.check(&board, Move::new(from, index, to));

        assert_eq!(check(0, 0, 6), Ok(()), "King-headed tail");
        assert_eq!(check(0, 1, 6), Err(MoveError::NotKing));
        assert_eq!(check(1, 0, 6), Err(MoveError::NotKing));
    }

    #[test]
    fn foundations_build_up_in_suit_from_the_ace() {
        let board = board(
            &[
                Pile::tableau(0, &[c(ACE, Spades)]),
                Pile::tableau(0, &[c(2, Spades)]),
                Pile::tableau(0, &[c(2, Hearts)]),
                Pile::tableau(0, &[c(3, Spades), c(2, Spades)]),
            ],
            &[Pile::holding(PileKind::Foundation, 0, &[c(ACE, Hearts)])],
        );
        let check = |from, index, to| Yukon.check(&board, Move::new(from, index, to));

        assert_eq!(check(0, 0, FOUNDATION + 1), Ok(()), "an Ace takes an empty");
        assert_eq!(check(2, 0, FOUNDATION), Ok(()), "2♥ onto A♥");
        assert_eq!(
            check(1, 0, FOUNDATION),
            Err(MoveError::Mismatch),
            "wrong suit"
        );
        assert_eq!(
            check(0, 0, FOUNDATION),
            Err(MoveError::Mismatch),
            "an Ace does not sit on an Ace"
        );
        assert_eq!(
            check(3, 0, FOUNDATION + 1),
            Err(MoveError::NotExposed),
            "a foundation takes one card, never a tail"
        );
    }

    #[test]
    fn a_card_may_be_taken_back_off_a_foundation() {
        let start = board(
            &[Pile::tableau(0, &[c(3, Spades)])],
            &[Pile::holding(
                PileKind::Foundation,
                0,
                &[c(ACE, Hearts), c(2, Hearts)],
            )],
        );
        let mut board = start.clone();

        assert_eq!(
            Yukon.check(&board, Move::new(FOUNDATION, 0, 0)),
            Err(MoveError::NotExposed),
            "only the card on top of a foundation is in play"
        );
        assert_eq!(
            Yukon.check(&board, Move::new(FOUNDATION, 1, FOUNDATION + 1)),
            Err(MoveError::WrongTarget),
            "and it never moves to another foundation"
        );

        let mv = Move::new(FOUNDATION, 1, 0);
        assert_eq!(Yukon.check(&board, mv), Ok(()), "2♥ back onto the 3♠");
        let action = Yukon.action_for(&board, mv);
        board.apply(&action);
        assert_eq!(board.pile(0).cards(), &[c(3, Spades), c(2, Hearts)]);

        board.undo(&action);
        assert_eq!(board, start);
    }

    #[test]
    fn move_flips_the_newly_exposed_card() {
        let start = board(
            &[
                Pile::tableau(2, &[c(3, Clubs), c(4, Clubs), c(9, Hearts)]),
                Pile::tableau(0, &[c(10, Spades)]),
            ],
            &[],
        );
        let mut board = start.clone();

        let action = Yukon.action_for(&board, Move::new(0, 2, 1));
        board.apply(&action);
        assert_eq!(board.pile(0).face_down(), 1, "4♣ turned face up");
        assert!(action.steps[0].flip_source);

        board.undo(&action);
        assert_eq!(board, start, "undo hides it again");
    }

    #[test]
    fn a_lone_ace_is_suggested_home() {
        // The first pile of a Yukon deal is a single card, so the move that
        // sends it home is index 0 into an empty pile — the exact shape
        // `is_useful` filters out when the target is a tableau pile.
        let board = board(&[Pile::tableau(0, &[c(ACE, Spades)])], &[]);
        assert_eq!(
            Yukon.suggestions(&board).first(),
            Some(&Move::new(0, 0, FOUNDATION))
        );
    }

    #[test]
    fn sliding_a_whole_pile_into_an_empty_one_is_not_suggested() {
        let board = board(&[Pile::tableau(0, &[c(KING, Spades), c(4, Hearts)])], &[]);
        assert!(
            Yukon.suggestions(&board).is_empty(),
            "the King has nowhere worth going"
        );
        assert_eq!(
            Yukon.check(&board, Move::new(0, 0, 1)),
            Ok(()),
            "still legal, just pointless"
        );
        assert!(!Yukon.has_progress_move(&board), "and so a dead position");
    }

    #[test]
    fn send_home_and_autoplay_finish_a_decided_board() {
        // Four piles, each a suit already in descending order: nothing hidden,
        // so the whole thing plays itself out.
        let piles: Vec<Pile> = Suit::ALL
            .into_iter()
            .map(|suit| Pile::tableau(0, &(0..13).map(|i| c(13 - i, suit)).collect::<Vec<_>>()))
            .collect();
        let mut board = board(&piles, &[]);

        assert!(send_home(&board, 0).is_some(), "the Ace at the foot goes");
        assert!(
            send_home(&board, TABLEAU).is_none(),
            "but a foundation sends nothing"
        );

        let mut played = 0;
        while let Some(action) = Yukon.autoplay(&board) {
            board.apply(&action);
            played += 1;
            assert!(played <= DECK, "autoplay must eventually stop");
        }
        assert!(Yukon.is_won(&board));
        assert_eq!(played, DECK);
        assert!(Yukon.autoplay(&board).is_none(), "and it stops once won");
    }

    #[test]
    fn autoplay_waits_while_anything_is_face_down() {
        assert!(
            Yukon.autoplay(&Yukon.deal(42)).is_none(),
            "an opening deal is not decided"
        );
    }

    #[test]
    fn undo_round_trips_a_played_game() {
        let start = Yukon.deal(2024);
        let mut board = start.clone();
        let mut history = Vec::new();

        for step in 0..400 {
            let moves = Yukon.suggestions(&board);
            if moves.is_empty() {
                break;
            }
            // Deterministic but varied, so the walk explores flips, empties,
            // multi-card tails and foundation sends rather than one move.
            let action = Yukon.action_for(&board, moves[(step * 7 + 3) % moves.len()]);
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
        let mut board = Yukon.deal(7);
        for step in 0..200 {
            let moves = Yukon.suggestions(&board);
            if moves.is_empty() {
                break;
            }
            board.apply(&Yukon.action_for(&board, moves[(step * 5 + 1) % moves.len()]));
            let seen: HashSet<Card> = board
                .piles()
                .iter()
                .flat_map(|pile| pile.cards().iter().copied())
                .collect();
            assert_eq!(seen.len(), DECK, "at step {step}");
        }
    }

    #[test]
    fn the_status_line_counts_the_cards_sent_home() {
        let board = board(
            &[],
            &[Pile::holding(
                PileKind::Foundation,
                0,
                &[c(ACE, Hearts), c(2, Hearts)],
            )],
        );
        let status = Yukon.status(&board);
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].value, "2/52");
    }

    #[test]
    fn a_full_set_of_foundations_is_a_win() {
        let suited = |suit| {
            Pile::holding(
                PileKind::Foundation,
                0,
                &(ACE..=KING).map(|rank| c(rank, suit)).collect::<Vec<_>>(),
            )
        };
        let done: Vec<Pile> = Suit::ALL.into_iter().map(suited).collect();
        assert!(Yukon.is_won(&board(&[], &done)));

        let mut short = done.clone();
        short[3] = Pile::new(PileKind::Foundation);
        assert!(!Yukon.is_won(&board(&[], &short)));
    }
}
