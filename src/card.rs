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
    fn card_faces_are_four_cells_wide() {
        for suit in Suit::ALL {
            for rank in ACE..=KING {
                assert_eq!(Card::new(rank, suit).face().chars().count(), 4);
            }
        }
    }
}
