//! Drawing the table: felt, empty-pile outlines, cards, highlights.
//!
//! Every position comes from the [`Layout`] or, for a card on the move, from
//! the [`Animator`] interpolating towards the layout. Nothing here works out
//! where a card goes — that is what keeps a press landing on what was drawn.

use std::f64::consts::PI;

use solitaire_tui_core::board::{Board, Fan, PileId, PileKind};
use solitaire_tui_core::card::Suit;
use web_sys::CanvasRenderingContext2d as Ctx;

use crate::anim::{Animator, Pose};
use crate::cards;
use crate::layout::{Layout, Rect};

const CURSOR: &str = "#4fd8eb";
const SELECTED: &str = "#ffd34d";
const TARGET: &str = "#6cf2a2";

/// Everything one frame shows.
pub struct Scene<'a> {
    pub board: &'a Board,
    pub layout: &'a Layout,
    pub anim: &'a Animator,
    /// The cursor's pile and card, when it should be shown at all.
    pub cursor: Option<(PileId, Option<usize>)>,
    /// The held tail, as `(pile, first card)`.
    pub selection: Option<(PileId, usize)>,
    pub hint_target: Option<(PileId, usize)>,
    /// How far the held tail has been dragged from its home.
    pub drag: Option<(f64, f64)>,
    pub particles: &'a [Particle],
    pub now: f64,
}

/// One falling suit of the winning shower.
#[derive(Clone, Copy)]
pub struct Particle {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub turn: f64,
    pub spin: f64,
    pub size: f64,
    pub suit: Suit,
}

impl Particle {
    /// Advance by `dt` seconds. `false` once it has fallen off the table.
    pub fn step(&mut self, dt: f64, height: f64) -> bool {
        self.vy += 420.0 * dt;
        self.x += self.vx * dt;
        self.y += self.vy * dt;
        self.turn += self.spin * dt;
        self.y < height + self.size
    }
}

pub fn felt(ctx: &Ctx, width: f64, height: f64) {
    let r = width.max(height) * 0.85;
    if let Ok(gradient) = ctx.create_radial_gradient(
        width / 2.0,
        height * 0.35,
        0.0,
        width / 2.0,
        height * 0.35,
        r,
    ) {
        let _ = gradient.add_color_stop(0.0, "#2f8f58");
        let _ = gradient.add_color_stop(0.55, "#1f6b40");
        let _ = gradient.add_color_stop(1.0, "#0e3b23");
        ctx.set_fill_style_canvas_gradient(&gradient);
    }
    ctx.fill_rect(0.0, 0.0, width, height);
}

pub fn too_small(ctx: &Ctx, width: f64, height: f64) {
    felt(ctx, width, height);
    ctx.set_fill_style_str("rgba(255, 255, 255, 0.8)");
    ctx.set_font("600 15px system-ui, sans-serif");
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    let _ = ctx.fill_text("Make the window a little bigger", width / 2.0, height / 2.0);
}

pub fn draw(ctx: &Ctx, scene: &Scene) {
    let layout = scene.layout;
    felt(ctx, layout.width, layout.height);

    let lifted = |pile: PileId, index: usize| {
        scene.drag.is_some()
            && scene
                .selection
                .is_some_and(|(p, first)| p == pile && index >= first)
    };
    let mut moving: Vec<(Pose, Option<solitaire_tui_core::card::Card>)> = Vec::new();

    for geom in &layout.piles {
        let pile = scene.board.pile(geom.pile);
        outline(ctx, geom.base, pile.kind(), pile.is_empty());

        let pose_of = |i: usize| {
            scene
                .anim
                .pose(geom.pile, i, geom.cards[i], pile.is_face_up(i), scene.now)
        };
        // A stacked pile only shows its top card — but when that card is on
        // its way in or out, the one beneath it has to show through.
        let mut start = geom.visible_from;
        while start > 0 && (pose_of(start).moving || lifted(geom.pile, start)) {
            start -= 1;
        }
        if pile.kind() == PileKind::Stock && geom.fan == Fan::None {
            depth(ctx, geom.base, pile.len());
        }
        for i in start..pile.len() {
            let pose = pose_of(i);
            let face = pose.face_up.then(|| pile.cards()[i]);
            if lifted(geom.pile, i) {
                continue;
            }
            if pose.moving {
                moving.push((pose, face));
                continue;
            }
            cards::draw(ctx, face, pose.rect, pose.squash);
        }
    }

    // Highlights sit between the still cards and anything in the air.
    if let Some((pile, index)) = scene.hint_target {
        glow(
            ctx,
            card_or_base(layout, pile, Some(index)),
            TARGET,
            layout.card_w,
        );
    }
    if let Some((pile, card)) = scene.cursor
        && !scene
            .selection
            .is_some_and(|(p, i)| p == pile && card == Some(i))
    {
        glow(ctx, card_or_base(layout, pile, card), CURSOR, layout.card_w);
    }
    if let Some((pile, first)) = scene.selection
        && scene.drag.is_none()
        && let Some(run) = run_rect(layout, pile, first, 0.0, 0.0)
    {
        glow(ctx, run, SELECTED, layout.card_w);
    }

    for (pose, face) in moving {
        cards::draw(ctx, face, pose.rect, pose.squash);
    }

    if let (Some((dx, dy)), Some((pile, first))) = (scene.drag, scene.selection)
        && let Some(geom) = layout.pile(pile)
    {
        let cards = scene.board.pile(pile).cards();
        if let Some(run) = run_rect(layout, pile, first, dx, dy) {
            glow(ctx, run, SELECTED, layout.card_w);
        }
        for (&card, home) in cards.iter().zip(&geom.cards).skip(first) {
            cards::draw(ctx, Some(card), home.offset(dx, dy), 1.0);
        }
    }

    for particle in scene.particles {
        ctx.save();
        let _ = ctx.translate(particle.x, particle.y);
        let _ = ctx.rotate(particle.turn);
        ctx.set_fill_style_str(if particle.suit.is_red() {
            "#ff4d6a"
        } else {
            "#f4f1e8"
        });
        cards::fill_suit(ctx, particle.suit, 0.0, 0.0, particle.size, false);
        ctx.restore();
    }
}

fn card_or_base(layout: &Layout, pile: PileId, card: Option<usize>) -> Rect {
    let Some(geom) = layout.pile(pile) else {
        return Rect::default();
    };
    card.and_then(|i| geom.cards.get(i).copied())
        .or_else(|| geom.cards.last().copied())
        .unwrap_or(geom.base)
}

/// The outline around a held tail, from its head to the bottom of the pile.
fn run_rect(layout: &Layout, pile: PileId, first: usize, dx: f64, dy: f64) -> Option<Rect> {
    let geom = layout.pile(pile)?;
    let head = geom.cards.get(first)?;
    let last = geom.cards.last()?;
    Some(Rect::new(
        head.x + dx,
        head.y + dy,
        head.w,
        last.bottom() - head.y,
    ))
}

fn glow(ctx: &Ctx, rect: Rect, colour: &str, card_w: f64) {
    let grow = (card_w * 0.03).max(2.0);
    let r = Rect::new(
        rect.x - grow,
        rect.y - grow,
        rect.w + 2.0 * grow,
        rect.h + 2.0 * grow,
    );
    ctx.save();
    ctx.set_shadow_color(colour);
    ctx.set_shadow_blur((card_w * 0.12).max(6.0));
    ctx.set_stroke_style_str(colour);
    ctx.set_line_width((card_w * 0.035).max(2.0));
    cards::rounded(ctx, r, cards::radius(card_w) + grow);
    ctx.stroke();
    ctx.restore();
}

/// The few card edges peeking out under a deep stock, so it reads as a pile.
fn depth(ctx: &Ctx, base: Rect, len: usize) {
    let layers = (len.saturating_sub(1) / 10).min(3);
    for layer in (1..=layers).rev() {
        let shift = layer as f64 * (base.w * 0.02).max(1.0);
        cards::draw(ctx, None, base.offset(-shift, -shift), 1.0);
    }
}

/// An empty pile's place on the felt: a recessed outline, marked by what the
/// pile is for.
fn outline(ctx: &Ctx, base: Rect, kind: PileKind, empty: bool) {
    let r = cards::radius(base.w);
    cards::rounded(ctx, base, r);
    ctx.set_fill_style_str("rgba(0, 0, 0, 0.16)");
    ctx.fill();
    ctx.set_line_width(1.5);
    ctx.set_stroke_style_str("rgba(255, 255, 255, 0.28)");
    ctx.stroke();
    if !empty {
        return;
    }
    let (cx, cy) = (base.x + base.w / 2.0, base.y + base.h / 2.0);
    ctx.set_fill_style_str("rgba(255, 255, 255, 0.22)");
    ctx.set_stroke_style_str("rgba(255, 255, 255, 0.26)");
    match kind {
        PileKind::Foundation => {
            ctx.set_font(&format!("700 {:.1}px Georgia, serif", base.w * 0.42));
            ctx.set_text_align("center");
            ctx.set_text_baseline("middle");
            let _ = ctx.fill_text("A", cx, cy);
        }
        PileKind::Stock => {
            // A ring with an arrowhead: turn the pile over.
            let radius = base.w * 0.22;
            ctx.set_line_width((base.w * 0.06).max(2.0));
            ctx.begin_path();
            let _ = ctx.arc(cx, cy, radius, -PI * 0.35, PI * 1.45);
            ctx.stroke();
            let end = PI * 1.45;
            let (ex, ey) = (cx + radius * end.cos(), cy + radius * end.sin());
            let size = base.w * 0.1;
            ctx.begin_path();
            ctx.move_to(ex + size, ey - size * 0.2);
            ctx.line_to(ex - size * 0.3, ey - size);
            ctx.line_to(ex - size * 0.4, ey + size * 0.6);
            ctx.close_path();
            ctx.fill();
        }
        _ => {}
    }
}
