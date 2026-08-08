//! The contract every solitaire variant implements, and the registry of them.
//!
//! A game supplies rules only: how to deal, what a legal move is, and when the
//! player has won. The board, the undo stacks, the cursor and the drawing are
//! all shared, so a new variant is one file here plus a line in [`games`].
//!
//! Most of the trait is defaulted. The default [`Rules::suggestions`] walks
//! every legal move and ranks it with [`Rules::rank`], which is enough for any
//! game that does not need to think harder than that.

pub mod freecell;
pub mod klondike;
pub mod russian;
pub mod scorpion;
pub mod spider;
pub mod spiderette;
pub mod yukon;

use crate::board::{Action, Board, BoardSpec, Move, MoveError, PileId};
use crate::card::Card;

/// How a tableau card may be covered — the one rule that separates Yukon from
/// Russian Solitaire, and FreeCell from Baker's Game.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Build {
    AlternatingColour,
    SameSuit,
}

impl Build {
    /// Whether `head` may land on `target`.
    ///
    /// A King's rank + 1 is 14, so no card ever satisfies this against one —
    /// which is what confines Kings to empty piles in the games that say so.
    pub(crate) fn accepts(self, target: Card, head: Card) -> bool {
        target.rank == head.rank + 1
            && match self {
                Self::AlternatingColour => target.suit.is_red() != head.suit.is_red(),
                Self::SameSuit => target.suit == head.suit,
            }
    }
}

/// A rule the player can change before dealing, shown in the menu's preview
/// pane. The game stores which value is selected and reads it when dealing.
pub struct OptionSpec {
    pub label: &'static str,
    pub values: &'static [&'static str],
}

/// How a status-bar value is coloured.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatusTone {
    /// Something still available — cards left in the stock, redeals in hand.
    Good,
    /// Something spent.
    Muted,
}

pub struct StatusItem {
    pub label: &'static str,
    pub value: String,
    pub tone: StatusTone,
}

impl StatusItem {
    pub fn new(label: &'static str, value: impl Into<String>, tone: StatusTone) -> Self {
        Self {
            label,
            value: value.into(),
            tone,
        }
    }
}

pub trait Rules {
    /// The name shown in the menu and used by `--game`.
    fn name(&self) -> &'static str;

    /// A few lines of rules for the menu's preview pane.
    fn blurb(&self) -> &'static str;

    /// The key bindings and rules shown by `?`, minus the shared ones.
    fn help(&self) -> &'static str;

    /// Deal a new game. The same seed must always deal the same cards.
    fn deal(&self, seed: u64) -> Board;

    /// Where this game's piles sit on screen.
    fn spec(&self, board: &Board) -> BoardSpec;

    /// Whether the cards at `mv.from[mv.index]` may be dropped on `mv.to`.
    fn check(&self, board: &Board, mv: Move) -> Result<(), MoveError>;

    fn is_won(&self, board: &Board) -> bool;

    // -- defaulted ---------------------------------------------------------

    /// Rules the player may change before dealing.
    fn options(&self) -> &[OptionSpec] {
        &[]
    }

    /// Which value each option currently sits on, as indices into its `values`.
    fn selected(&self) -> Vec<usize> {
        Vec::new()
    }

    /// Move option `index` on by `delta` values, wrapping.
    fn configure(&mut self, index: usize, delta: i32) {
        let _ = (index, delta);
    }

    /// Turn a legal move into the action that performs it. The default lifts a
    /// tail from one pile to another, which is what nearly every move is.
    fn action_for(&self, board: &Board, mv: Move) -> Action {
        Action::moving(board, mv.from, mv.index, mv.to)
    }

    /// What pressing the action key on `pile` does — dealing the stock, turning
    /// a card, recycling the waste. `None` if the pile does nothing.
    fn activate(&self, board: &Board, pile: PileId) -> Option<Action> {
        let _ = (board, pile);
        None
    }

    /// Why [`Rules::activate`] would not deal the stock, shown when the player
    /// presses the deal key and nothing happens.
    ///
    /// The default assumes the only reason is an exhausted stock, which is
    /// true of every game whose stock deals whenever it has cards. Override it
    /// when a full stock can still refuse — Spider will not deal onto an empty
    /// column, and saying the stock is spent would be a lie.
    fn stock_refusal(&self, board: &Board) -> &'static str {
        let _ = board;
        "The stock has nothing left to give."
    }

    /// Send the exposed card of `from` to wherever it belongs, if it fits.
    fn send_home(&self, board: &Board, from: PileId) -> Option<Action> {
        let _ = (board, from);
        None
    }

    /// One step of an automatic finish, once the game is decided but not over.
    /// The app drains this, so each step stays individually undoable.
    fn autoplay(&self, board: &Board) -> Option<Action> {
        let _ = board;
        None
    }

    /// Whether a legal move is worth offering as a hint. Legal-but-pointless
    /// moves — sliding a whole pile into an empty one — are filtered out here.
    fn is_useful(&self, board: &Board, mv: Move) -> bool {
        let _ = (board, mv);
        true
    }

    /// How much a move is worth, in the rough order a player would think of it.
    fn rank(&self, board: &Board, mv: Move) -> i32 {
        let _ = (board, mv);
        0
    }

    /// Every useful legal move, best first.
    ///
    /// The sort is stable over an already deterministic list, so repeated calls
    /// in the same position always offer hints in the same order.
    fn suggestions(&self, board: &Board) -> Vec<Move> {
        let mut found = Vec::new();
        for from in 0..board.piles().len() {
            let source = board.pile(from);
            let Some(first) = source.first_face_up() else {
                continue;
            };
            for index in first..source.len() {
                for to in 0..board.piles().len() {
                    let mv = Move::new(from, index, to);
                    if self.check(board, mv).is_ok() && self.is_useful(board, mv) {
                        found.push(mv);
                    }
                }
            }
        }
        found.sort_by_key(|mv| -self.rank(board, *mv));
        found
    }

    /// Whether any move that changes the position remains.
    fn has_progress_move(&self, board: &Board) -> bool {
        !self.suggestions(board).is_empty()
    }

    /// Game-specific counters for the status bar. Seed and move count are shown
    /// for every game and do not belong here.
    fn status(&self, board: &Board) -> Vec<StatusItem> {
        let _ = board;
        Vec::new()
    }
}

/// Every game the app can play, in the order the menu lists them.
pub fn games() -> Vec<Box<dyn Rules>> {
    vec![
        Box::new(klondike::Klondike::default()),
        Box::new(freecell::FreeCell),
        Box::new(scorpion::Scorpion),
        Box::new(yukon::Yukon),
        Box::new(russian::Russian),
        Box::new(spiderette::Spiderette),
        Box::new(spider::Spider::default()),
    ]
}

/// Look a game up by name, case-insensitively, for `--game`.
pub fn find(name: &str) -> Option<Box<dyn Rules>> {
    games()
        .into_iter()
        .find(|game| game.name().eq_ignore_ascii_case(name))
}

/// Apply a `key=value` rule setting to `game`, as `--rule` supplies it.
///
/// Returns the label and value it matched so the caller can report what it did,
/// or an error naming what was actually available.
pub fn set_option(game: &mut Box<dyn Rules>, key: &str, value: &str) -> Result<(), String> {
    let index = game
        .options()
        .iter()
        .position(|option| option.label.eq_ignore_ascii_case(key))
        .ok_or_else(|| {
            let known: Vec<_> = game.options().iter().map(|o| o.label).collect();
            if known.is_empty() {
                format!("{} has no rules to set", game.name())
            } else {
                format!(
                    "{} has no rule {key:?}; try {}",
                    game.name(),
                    known.join(", ")
                )
            }
        })?;

    let target = game.options()[index]
        .values
        .iter()
        .position(|candidate| candidate.eq_ignore_ascii_case(value))
        .ok_or_else(|| {
            format!(
                "{:?} is not a value for {:?}; try {}",
                value,
                game.options()[index].label,
                game.options()[index].values.join(", ")
            )
        })?;

    // `configure` is the only way a game changes an option, so step to the
    // wanted value rather than reaching into the game's own state.
    let steps = target as i32 - game.selected()[index] as i32;
    game.configure(index, steps);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_game_is_findable_by_its_own_name() {
        for game in games() {
            let found = find(game.name()).expect("registered games are findable");
            assert_eq!(found.name(), game.name());
        }
        assert!(find("patience").is_none());
    }

    #[test]
    fn game_names_are_unique() {
        let mut names: Vec<_> = games().iter().map(|game| game.name()).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count);
    }

    #[test]
    fn every_game_deals_the_same_board_for_the_same_seed() {
        for game in games() {
            assert_eq!(
                game.deal(1234),
                game.deal(1234),
                "{} is not seed-stable",
                game.name()
            );
            assert_ne!(
                game.deal(1).piles(),
                game.deal(2).piles(),
                "{}",
                game.name()
            );
        }
    }

    #[test]
    fn every_game_deals_a_full_deck_and_offers_an_opening_move() {
        for game in games() {
            let board = game.deal(42);
            let dealt: usize = board.piles().iter().map(|pile| pile.len()).sum();
            assert_eq!(dealt % 52, 0, "{} deals whole decks", game.name());
            assert!(dealt >= 52, "{} deals at least one deck", game.name());
            assert!(!game.is_won(&board), "{} does not open won", game.name());
            assert!(
                game.has_progress_move(&board),
                "{} opens with something to do",
                game.name()
            );
        }
    }

    #[test]
    fn every_suggestion_is_a_legal_move() {
        for game in games() {
            let board = game.deal(42);
            for mv in game.suggestions(&board) {
                assert_eq!(
                    game.check(&board, mv),
                    Ok(()),
                    "{} suggested an illegal move",
                    game.name()
                );
            }
        }
    }

    #[test]
    fn suggestions_are_stable_for_the_same_position() {
        for game in games() {
            let board = game.deal(7);
            assert_eq!(
                game.suggestions(&board),
                game.suggestions(&board),
                "{}",
                game.name()
            );
        }
    }

    #[test]
    fn every_spot_names_a_real_pile_and_no_two_share_a_place() {
        for game in games() {
            let board = game.deal(3);
            let spec = game.spec(&board);
            let mut seen = Vec::new();
            for spot in &spec.spots {
                assert!(spot.pile < board.piles().len(), "{}", game.name());
                assert!(
                    !seen.contains(&(spot.row, spot.slot)),
                    "{} draws two piles in the same place",
                    game.name()
                );
                seen.push((spot.row, spot.slot));
            }
        }
    }

    #[test]
    fn every_declared_option_can_be_set_by_name() {
        for name in games().iter().map(|game| game.name()) {
            let mut game = find(name).unwrap();
            for index in 0..game.options().len() {
                let option = &game.options()[index];
                let (label, values) = (option.label, option.values.to_vec());
                for value in values {
                    set_option(&mut game, label, value)
                        .unwrap_or_else(|error| panic!("{name}: {error}"));
                    assert_eq!(game.options()[index].values[game.selected()[index]], value);
                }
                assert!(set_option(&mut game, label, "nonsense").is_err());
            }
            assert!(set_option(&mut game, "nonsense", "1").is_err());
        }
    }
}
