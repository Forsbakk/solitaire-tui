//! Cards, suits, and a deterministic shuffle.
//!
//! The shuffle deliberately uses a small built-in PRNG rather than the `rand`
//! crate: a given `--seed` should deal the same game forever, and pinning that
//! guarantee to a dependency's internals would break it on the next upgrade.

use std::fmt;

pub const ACE: u8 = 1;
pub const KING: u8 = 13;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Suit {
    Spades,
    Hearts,
    Diamonds,
    Clubs,
}

impl Suit {
    pub const ALL: [Self; 4] = [Self::Spades, Self::Hearts, Self::Diamonds, Self::Clubs];

    /// Position in [`Suit::ALL`] — the inverse of indexing it.
    ///
    /// Useful wherever a suit needs an order it does not otherwise have:
    /// squashing a deck into fewer suits, or sorting cards for a count.
    pub const fn index(self) -> usize {
        match self {
            Self::Spades => 0,
            Self::Hearts => 1,
            Self::Diamonds => 2,
            Self::Clubs => 3,
        }
    }

    pub const fn symbol(self) -> char {
        match self {
            Self::Spades => '♠',
            Self::Hearts => '♥',
            Self::Diamonds => '♦',
            Self::Clubs => '♣',
        }
    }

    pub const fn is_red(self) -> bool {
        matches!(self, Self::Hearts | Self::Diamonds)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Card {
    pub rank: u8,
    pub suit: Suit,
}

impl Card {
    pub const fn new(rank: u8, suit: Suit) -> Self {
        debug_assert!(rank >= ACE && rank <= KING);
        Self { rank, suit }
    }

    pub const fn rank_str(self) -> &'static str {
        match self.rank {
            1 => "A",
            2 => "2",
            3 => "3",
            4 => "4",
            5 => "5",
            6 => "6",
            7 => "7",
            8 => "8",
            9 => "9",
            10 => "10",
            11 => "J",
            12 => "Q",
            13 => "K",
            _ => "?",
        }
    }

    /// The four cells drawn inside a card box, e.g. `" A♠ "` or `"10♥ "`.
    pub fn face(self) -> String {
        format!("{:>2}{} ", self.rank_str(), self.suit.symbol())
    }
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.rank_str(), self.suit.symbol())
    }
}

/// A full 52-card deck shuffled with `seed`.
pub fn shuffled_deck(seed: u64) -> Vec<Card> {
    let mut deck: Vec<Card> = Suit::ALL
        .iter()
        .flat_map(|&suit| (ACE..=KING).map(move |rank| Card::new(rank, suit)))
        .collect();

    let mut rng = SplitMix64::new(seed);
    for i in (1..deck.len()).rev() {
        deck.swap(i, rng.below(i as u64 + 1) as usize);
    }
    deck
}

/// `decks` full 52-card decks shuffled together with `seed`.
///
/// Deliberately not written in terms of [`shuffled_deck`], nor it in terms of
/// this: the test that the two agree for one deck is what says no existing
/// seed's deal moved when two-deck games arrived, and it is only worth
/// anything while they are two pieces of code that could disagree.
///
/// With more than one deck a `Card` value no longer names a single card —
/// `Card` derives `Eq` and `Hash`, so anything counting cards needs a multiset
/// rather than a set.
pub fn shuffled_decks(seed: u64, decks: usize) -> Vec<Card> {
    let mut deck: Vec<Card> = (0..decks)
        .flat_map(|_| {
            Suit::ALL
                .iter()
                .flat_map(|&suit| (ACE..=KING).map(move |rank| Card::new(rank, suit)))
        })
        .collect();

    let mut rng = SplitMix64::new(seed);
    for i in (1..deck.len()).rev() {
        deck.swap(i, rng.below(i as u64 + 1) as usize);
    }
    deck
}

/// Vigna's splitmix64. Tiny, well-distributed, and stable across releases.
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform value in `0..n`, rejecting the biased tail of the u64 range.
    ///
    /// `2^64 % n` values at the top of the range would map to over-represented
    /// residues, so they are drawn again instead.
    fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0);
        // 2^64 % n, computed without needing 2^64 to fit in a u64.
        let remainder = (u64::MAX % n + 1) % n;
        loop {
            let v = self.next_u64();
            if remainder == 0 || v < u64::MAX - remainder + 1 {
                return v % n;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn deck_holds_every_card_once() {
        let deck = shuffled_deck(1);
        assert_eq!(deck.len(), 52);
        assert_eq!(deck.iter().copied().collect::<HashSet<_>>().len(), 52);
    }

    #[test]
    fn shuffle_is_seed_stable() {
        assert_eq!(shuffled_deck(7), shuffled_deck(7));
        assert_ne!(shuffled_deck(7), shuffled_deck(8));
    }

    #[test]
    fn every_suit_is_its_own_index() {
        for suit in Suit::ALL {
            assert_eq!(Suit::ALL[suit.index()], suit);
        }
    }

    #[test]
    fn one_deck_shuffled_as_decks_is_the_deck() {
        // The assertion that says two-deck games arrived without moving a
        // single existing seed's deal. It is worth having only because the
        // two functions are written separately.
        for seed in [0, 1, 42, 2024, u64::MAX] {
            assert_eq!(shuffled_decks(seed, 1), shuffled_deck(seed));
        }
    }

    #[test]
    fn two_decks_hold_every_card_twice() {
        let deck = shuffled_decks(9, 2);
        assert_eq!(deck.len(), 104);

        // A `HashSet` would say 52 here and pass just as happily if half the
        // cards were missing, so count the multiset instead.
        let mut census: Vec<(usize, u8)> = deck
            .iter()
            .map(|card| (card.suit.index(), card.rank))
            .collect();
        census.sort_unstable();
        for suit in Suit::ALL {
            for rank in ACE..=KING {
                let held = census
                    .iter()
                    .filter(|&&c| c == (suit.index(), rank))
                    .count();
                assert_eq!(held, 2, "{}", Card::new(rank, suit));
            }
        }
    }

    #[test]
    fn a_multi_deck_shuffle_is_seed_stable() {
        assert_eq!(shuffled_decks(7, 2), shuffled_decks(7, 2));
        assert_ne!(shuffled_decks(7, 2), shuffled_decks(8, 2));
    }

    #[test]
    fn no_decks_is_no_cards() {
        assert!(shuffled_decks(7, 0).is_empty());
    }

    #[test]
    fn card_faces_are_four_cells_wide() {
        for suit in Suit::ALL {
            for rank in ACE..=KING {
                assert_eq!(Card::new(rank, suit).face().chars().count(), 4);
            }
        }
    }
}
