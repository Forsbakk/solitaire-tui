//! Card movement, worked out by comparing two pictures of the table.
//!
//! The engine does not report what a key or a click did — a move, an undo, a
//! stock deal, Spider filing a run, `A` playing a whole game out — and it does
//! not need to. Before each input the frontend notes where every card is shown
//! ([`Animator::snapshot`]); afterwards it lays the board out again and
//! [`Animator::diff`] pairs the cards up by value. Whatever moved or turned
//! over gets a tween from where it was shown to where it now belongs.
//!
//! Two-deck games hold every card twice. Pairing prefers a card that kept its
//! place, then the nearest unclaimed twin — two identical cards trading places
//! look the same either way.
//!
//! Tweens are keyed by where a card *ends up*, so drawing asks "is the card at
//! this pile and index still on its way?" and never needs a card identity.

use std::collections::HashMap;

use solitaire_tui_core::board::{Board, PileId};
use solitaire_tui_core::card::Card;

use crate::layout::{Layout, Rect};

/// How long one card takes to travel, in milliseconds.
const DURATION: f64 = 230.0;
/// The most a whole batch's stagger may add up to.
const STAGGER_TOTAL: f64 = 900.0;
const STAGGER_STEP: f64 = 32.0;

/// One card as it was last shown.
#[derive(Clone, Copy, Debug)]
pub struct Seen {
    pub card: Card,
    pub face_up: bool,
    pub pile: PileId,
    pub index: usize,
    pub rect: Rect,
}

#[derive(Clone, Copy, Debug)]
pub struct Tween {
    pub from: Rect,
    pub from_face: bool,
    pub start: f64,
    pub duration: f64,
}

/// Where a card is drawn this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub rect: Rect,
    pub face_up: bool,
    /// Horizontal squash while turning over: 1 flat, 0 edge-on.
    pub squash: f64,
    /// Whether it is still on its way, and so belongs above the still cards.
    pub moving: bool,
}

#[derive(Default)]
pub struct Animator {
    tweens: HashMap<(PileId, usize), Tween>,
}

fn ease_out(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

impl Animator {
    pub fn is_busy(&self, now: f64) -> bool {
        self.tweens.values().any(|t| now < t.start + t.duration)
    }

    pub fn clear(&mut self) {
        self.tweens.clear();
    }

    /// Where the card at `pile`/`index` is drawn at `now`, given where it
    /// belongs and which way up it ends.
    pub fn pose(&self, pile: PileId, index: usize, home: Rect, face_up: bool, now: f64) -> Pose {
        let still = Pose {
            rect: home,
            face_up,
            squash: 1.0,
            moving: false,
        };
        let Some(tween) = self.tweens.get(&(pile, index)) else {
            return still;
        };
        let t = ((now - tween.start) / tween.duration).clamp(0.0, 1.0);
        if t >= 1.0 {
            return still;
        }
        let rect = tween.from.lerp(&home, ease_out(t));
        let (face_up, squash) = if tween.from_face == face_up {
            (face_up, 1.0)
        } else if t < 0.5 {
            (tween.from_face, 1.0 - 2.0 * t)
        } else {
            (face_up, 2.0 * t - 1.0)
        };
        Pose {
            rect,
            face_up,
            squash,
            moving: true,
        }
    }

    /// Every card of `board` as drawn at `now`, with `lifted` naming the held
    /// tail and how far it has been dragged.
    pub fn snapshot(
        &self,
        board: &Board,
        layout: &Layout,
        lifted: Option<((PileId, usize), (f64, f64))>,
        now: f64,
    ) -> Vec<Seen> {
        let mut seen = Vec::new();
        for geom in &layout.piles {
            let pile = board.pile(geom.pile);
            for (index, (&card, &home)) in pile.cards().iter().zip(&geom.cards).enumerate() {
                let face = pile.is_face_up(index);
                let mut rect = self.pose(geom.pile, index, home, face, now).rect;
                if let Some(((from, first), (dx, dy))) = lifted
                    && from == geom.pile
                    && index >= first
                {
                    rect = home.offset(dx, dy);
                }
                seen.push(Seen {
                    card,
                    face_up: face,
                    pile: geom.pile,
                    index,
                    rect,
                });
            }
        }
        seen
    }

    /// Start a tween for every card of `board` that is not where `before` last
    /// showed it. A card with no counterpart in `before` — a fresh deal —
    /// flies in from `origin`, face down, when one is given.
    pub fn diff(
        &mut self,
        before: &[Seen],
        board: &Board,
        layout: &Layout,
        origin: Option<Rect>,
        now: f64,
    ) {
        self.tweens.clear();
        let mut by_card: HashMap<Card, Vec<usize>> = HashMap::new();
        for (i, seen) in before.iter().enumerate() {
            by_card.entry(seen.card).or_default().push(i);
        }
        let mut used = vec![false; before.len()];

        // Pair the cards that stayed put first, so a twin that moved cannot
        // claim the spot of one that did not.
        let mut pending = Vec::new();
        for geom in &layout.piles {
            let pile = board.pile(geom.pile);
            for (index, (&card, &home)) in pile.cards().iter().zip(&geom.cards).enumerate() {
                let face = pile.is_face_up(index);
                let same_place = by_card.get(&card).and_then(|list| {
                    list.iter().copied().find(|&i| {
                        !used[i] && before[i].pile == geom.pile && before[i].index == index
                    })
                });
                match same_place {
                    Some(i) => {
                        used[i] = true;
                        pending.push((geom.pile, index, home, face, Some(i)));
                    }
                    None => pending.push((geom.pile, index, home, face, None)),
                }
            }
        }
        for entry in &mut pending {
            let (_, _, home, _, matched) = entry;
            if matched.is_none() {
                let card = board.pile(entry.0).cards()[entry.1];
                let nearest = by_card.get(&card).and_then(|list| {
                    list.iter().copied().filter(|&i| !used[i]).min_by(|&a, &b| {
                        before[a]
                            .rect
                            .distance(home)
                            .total_cmp(&before[b].rect.distance(home))
                    })
                });
                if let Some(i) = nearest {
                    used[i] = true;
                }
                *matched = nearest;
            }
        }

        let mut moving: Vec<_> = pending
            .into_iter()
            .filter_map(|(pile, index, home, face, matched)| {
                let (from, from_face) = match matched {
                    Some(i) => (before[i].rect, before[i].face_up),
                    None => (origin?, false),
                };
                let moved = from.distance(&home) > 0.5 || (from.w - home.w).abs() > 0.5;
                (moved || from_face != face).then_some((pile, index, home, from, from_face))
            })
            .collect();

        // Cards leave in dealing order — row by row down the piles — which is
        // also bottom-first for a run, so the run unrolls the way it is held.
        moving.sort_by_key(|&(pile, index, home, ..)| (index, home.x as i64, pile));
        let step = if moving.len() > 1 {
            STAGGER_STEP.min(STAGGER_TOTAL / moving.len() as f64)
        } else {
            0.0
        };
        // A run moving together should arrive together: only stagger when
        // cards head for different piles.
        let one_pile = moving.windows(2).all(|w| w[0].0 == w[1].0);
        let step = if one_pile && moving.len() < 14 {
            step / 3.0
        } else {
            step
        };

        for (order, (pile, index, _, from, from_face)) in moving.into_iter().enumerate() {
            self.tweens.insert(
                (pile, index),
                Tween {
                    from,
                    from_face,
                    start: now + order as f64 * step,
                    duration: DURATION,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout;
    use solitaire_tui_core::board::Move;
    use solitaire_tui_core::rules;

    #[test]
    fn a_played_move_animates_exactly_the_cards_that_moved() {
        let game = rules::find("klondike").unwrap();
        let mut board = game.deal(42);
        let spec = game.spec(&board);
        let before_layout = layout::compute(&spec, &board, 1280.0, 760.0).unwrap();
        let mut animator = Animator::default();
        let before = animator.snapshot(&board, &before_layout, None, 0.0);

        let mv: Move = game.suggestions(&board)[0];
        let moved = board.pile(mv.from).len() - mv.index;
        let flips = usize::from(mv.index > 0 && !board.pile(mv.from).is_face_up(mv.index - 1));
        board.apply(&game.action_for(&board, mv));
        let after = layout::compute(&spec, &board, 1280.0, 760.0).unwrap();
        animator.diff(&before, &board, &after, None, 0.0);

        assert_eq!(animator.tweens.len(), moved + flips);
        assert!(animator.is_busy(1.0));
        assert!(!animator.is_busy(5000.0));
    }

    #[test]
    fn nothing_moves_when_nothing_changed() {
        let game = rules::find("spider").unwrap();
        let board = game.deal(3);
        let layout = layout::compute(&game.spec(&board), &board, 1280.0, 760.0).unwrap();
        let mut animator = Animator::default();
        let before = animator.snapshot(&board, &layout, None, 0.0);
        animator.diff(&before, &board, &layout, None, 0.0);
        assert!(
            animator.tweens.is_empty(),
            "two decks of twins still pair up in place"
        );
    }

    #[test]
    fn a_fresh_deal_flies_in_from_the_origin() {
        let game = rules::find("scorpion").unwrap();
        let board = game.deal(42);
        let layout = layout::compute(&game.spec(&board), &board, 1280.0, 760.0).unwrap();
        let mut animator = Animator::default();
        let origin = Rect::new(0.0, 0.0, 10.0, 14.0);
        animator.diff(&[], &board, &layout, Some(origin), 0.0);
        let placed: usize = layout.piles.iter().map(|p| p.cards.len()).sum();
        assert_eq!(animator.tweens.len(), placed);
        let pose = animator.pose(0, 0, layout.card(0, 0).unwrap(), true, 0.0);
        assert_eq!(pose.rect, origin);
        assert!(!pose.face_up);
    }
}
