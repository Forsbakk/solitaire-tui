//! Painting cards, drawn entirely in code.
//!
//! No artwork is shipped: suits are paths, faces are pips laid out the way a
//! printed deck lays them out, and court cards are a framed letter. Drawing
//! from paths keeps every suit identical on every device — a font's `♣` is a
//! different shape on each platform, and some have none — and lets a card be
//! any size at any pixel density.
//!
//! Every card is painted straight onto the table, every frame. An earlier
//! version painted each card once into an offscreen canvas and stamped copies
//! from it, but some Gecko builds with GPU canvas (Zen, at least) drew only the
//! shadow of each copy. A canvas-to-canvas copy is not worth a browser bug: a
//! full table is a few thousand path operations, which any device draws well
//! within a frame. Shadows are a plain offset shape rather than a blur for the
//! same reason — a blur per card per frame is what would cost a phone.

use std::f64::consts::PI;

use solitaire_tui_core::card::{Card, Suit};
use web_sys::CanvasRenderingContext2d as Ctx;

use crate::layout::{Rect, SMALL_CARD};

pub const RED: &str = "#c4122f";
pub const BLACK: &str = "#1c1d24";
const PAPER: &str = "#fdfcf7";
const SERIF: &str = "Georgia, 'Times New Roman', 'DejaVu Serif', serif";

pub fn ink(suit: Suit) -> &'static str {
    if suit.is_red() { RED } else { BLACK }
}

/// Trace `suit` centred on `(cx, cy)`, `size` tall, as the current path.
pub fn suit_path(ctx: &Ctx, suit: Suit, cx: f64, cy: f64, size: f64) {
    let p = |x: f64, y: f64| (cx + x * size, cy + y * size);
    ctx.begin_path();
    match suit {
        Suit::Hearts => {
            let (x, y) = p(0.0, 0.44);
            ctx.move_to(x, y);
            bez(ctx, p(-0.16, 0.30), p(-0.5, 0.06), p(-0.5, -0.17));
            bez(ctx, p(-0.5, -0.43), p(-0.2, -0.54), p(0.0, -0.27));
            bez(ctx, p(0.2, -0.54), p(0.5, -0.43), p(0.5, -0.17));
            bez(ctx, p(0.5, 0.06), p(0.16, 0.30), p(0.0, 0.44));
        }
        Suit::Diamonds => {
            let (x, y) = p(0.0, -0.5);
            ctx.move_to(x, y);
            quad(ctx, p(0.16, -0.2), p(0.38, 0.0));
            quad(ctx, p(0.16, 0.2), p(0.0, 0.5));
            quad(ctx, p(-0.16, 0.2), p(-0.38, 0.0));
            quad(ctx, p(-0.16, -0.2), p(0.0, -0.5));
        }
        Suit::Spades => {
            let (x, y) = p(0.0, -0.5);
            ctx.move_to(x, y);
            bez(ctx, p(0.16, -0.30), p(0.5, -0.1), p(0.5, 0.12));
            bez(ctx, p(0.5, 0.34), p(0.22, 0.42), p(0.05, 0.24));
            line(ctx, p(0.14, 0.5));
            line(ctx, p(-0.14, 0.5));
            line(ctx, p(-0.05, 0.24));
            bez(ctx, p(-0.22, 0.42), p(-0.5, 0.34), p(-0.5, 0.12));
            bez(ctx, p(-0.5, -0.1), p(-0.16, -0.30), p(0.0, -0.5));
        }
        Suit::Clubs => {
            let r = 0.215 * size;
            for (x, y) in [p(0.0, -0.25), p(-0.25, 0.07), p(0.25, 0.07)] {
                ctx.move_to(x + r, y);
                let _ = ctx.arc(x, y, r, 0.0, 2.0 * PI);
            }
            let (x, y) = p(0.0, -0.05);
            ctx.move_to(x, y);
            line(ctx, p(0.08, 0.12));
            line(ctx, p(0.15, 0.5));
            line(ctx, p(-0.15, 0.5));
            line(ctx, p(-0.08, 0.12));
        }
    }
    ctx.close_path();
}

fn bez(ctx: &Ctx, a: (f64, f64), b: (f64, f64), c: (f64, f64)) {
    ctx.bezier_curve_to(a.0, a.1, b.0, b.1, c.0, c.1);
}

fn quad(ctx: &Ctx, a: (f64, f64), b: (f64, f64)) {
    ctx.quadratic_curve_to(a.0, a.1, b.0, b.1);
}

fn line(ctx: &Ctx, a: (f64, f64)) {
    ctx.line_to(a.0, a.1);
}

pub fn fill_suit(ctx: &Ctx, suit: Suit, cx: f64, cy: f64, size: f64, flipped: bool) {
    if flipped {
        ctx.save();
        let _ = ctx.translate(cx, cy);
        let _ = ctx.rotate(PI);
        suit_path(ctx, suit, 0.0, 0.0, size);
        ctx.fill();
        ctx.restore();
    } else {
        suit_path(ctx, suit, cx, cy, size);
        ctx.fill();
    }
}

/// Trace a rounded rectangle as the current path.
pub fn rounded(ctx: &Ctx, r: Rect, radius: f64) {
    let radius = radius.min(r.w / 2.0).min(r.h / 2.0);
    ctx.begin_path();
    ctx.move_to(r.x + radius, r.y);
    let _ = ctx.arc_to(r.right(), r.y, r.right(), r.bottom(), radius);
    let _ = ctx.arc_to(r.right(), r.bottom(), r.x, r.bottom(), radius);
    let _ = ctx.arc_to(r.x, r.bottom(), r.x, r.y, radius);
    let _ = ctx.arc_to(r.x, r.y, r.right(), r.y, radius);
    ctx.close_path();
}

pub fn radius(w: f64) -> f64 {
    (w * 0.075).max(3.0)
}

/// Paint `card` — `None` for its back — into `rect`, squashed horizontally by
/// `squash` about its centre while it turns over.
pub fn draw(ctx: &Ctx, card: Option<Card>, rect: Rect, squash: f64) {
    let (w, h) = (rect.w, rect.h);
    let r = radius(w);
    ctx.save();
    let _ = ctx.translate(rect.x + w / 2.0, rect.y);
    let _ = ctx.scale(squash.max(0.02), 1.0);
    let _ = ctx.translate(-w / 2.0, 0.0);
    let card_rect = Rect::new(0.0, 0.0, w, h);

    // A shadow under the lower edge, lifting the card off the felt.
    let drop = (w * 0.018).max(1.0);
    rounded(ctx, Rect::new(0.0, drop, w, h), r);
    ctx.set_fill_style_str("rgba(0, 0, 0, 0.3)");
    ctx.fill();

    rounded(ctx, card_rect, r);
    ctx.set_fill_style_str(PAPER);
    ctx.fill();
    ctx.set_line_width(1.0);
    ctx.set_stroke_style_str("rgba(0, 0, 0, 0.28)");
    rounded(ctx, Rect::new(0.5, 0.5, w - 1.0, h - 1.0), r);
    ctx.stroke();

    match card {
        None => paint_back(ctx, card_rect, r),
        Some(card) => paint_face(ctx, card, card_rect),
    }
    ctx.restore();
}

fn paint_back(ctx: &Ctx, rect: Rect, r: f64) {
    let inset = (rect.w * 0.06).max(2.0);
    let inner = Rect::new(inset, inset, rect.w - 2.0 * inset, rect.h - 2.0 * inset);
    let gradient = ctx.create_linear_gradient(0.0, 0.0, rect.w, rect.h);
    let _ = gradient.add_color_stop(0.0, "#2a4fa3");
    let _ = gradient.add_color_stop(1.0, "#142a63");
    rounded(ctx, inner, r * 0.6);
    ctx.set_fill_style_canvas_gradient(&gradient);
    ctx.fill();

    // A lattice, clipped to the panel.
    ctx.save();
    rounded(ctx, inner, r * 0.6);
    ctx.clip();
    ctx.set_stroke_style_str("rgba(255, 255, 255, 0.16)");
    ctx.set_line_width((rect.w * 0.012).max(0.6));
    let step = (rect.w * 0.12).max(4.0);
    let span = rect.w + rect.h;
    let mut d = -rect.h;
    ctx.begin_path();
    while d < span {
        ctx.move_to(d, 0.0);
        ctx.line_to(d + rect.h, rect.h);
        ctx.move_to(d + rect.h, 0.0);
        ctx.line_to(d, rect.h);
        d += step;
    }
    ctx.stroke();
    ctx.restore();

    // A medallion in the middle: a diamond, gold-edged.
    let (cx, cy) = (rect.w / 2.0, rect.h / 2.0);
    let s = rect.w * 0.2;
    ctx.begin_path();
    ctx.move_to(cx, cy - s * 1.3);
    ctx.line_to(cx + s, cy);
    ctx.line_to(cx, cy + s * 1.3);
    ctx.line_to(cx - s, cy);
    ctx.close_path();
    ctx.set_fill_style_str("#1a3680");
    ctx.fill();
    ctx.set_stroke_style_str("#d9b45a");
    ctx.set_line_width((rect.w * 0.02).max(0.8));
    ctx.stroke();

    ctx.set_stroke_style_str("rgba(255, 255, 255, 0.55)");
    ctx.set_line_width((rect.w * 0.012).max(0.6));
    rounded(ctx, inner, r * 0.6);
    ctx.stroke();
}

/// Where the pips of a 2–10 sit, as (column, row) over the pip area: column
/// 0, ½ or 1 across, row 0 to 1 down.
fn pips(rank: u8) -> &'static [(f64, f64)] {
    const T: f64 = 1.0 / 3.0;
    match rank {
        2 => &[(0.5, 0.0), (0.5, 1.0)],
        3 => &[(0.5, 0.0), (0.5, 0.5), (0.5, 1.0)],
        4 => &[(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)],
        5 => &[(0.0, 0.0), (1.0, 0.0), (0.5, 0.5), (0.0, 1.0), (1.0, 1.0)],
        6 => &[
            (0.0, 0.0),
            (1.0, 0.0),
            (0.0, 0.5),
            (1.0, 0.5),
            (0.0, 1.0),
            (1.0, 1.0),
        ],
        7 => &[
            (0.0, 0.0),
            (1.0, 0.0),
            (0.5, 0.25),
            (0.0, 0.5),
            (1.0, 0.5),
            (0.0, 1.0),
            (1.0, 1.0),
        ],
        8 => &[
            (0.0, 0.0),
            (1.0, 0.0),
            (0.5, 0.25),
            (0.0, 0.5),
            (1.0, 0.5),
            (0.5, 0.75),
            (0.0, 1.0),
            (1.0, 1.0),
        ],
        9 => &[
            (0.0, 0.0),
            (1.0, 0.0),
            (0.0, T),
            (1.0, T),
            (0.5, 0.5),
            (0.0, 2.0 * T),
            (1.0, 2.0 * T),
            (0.0, 1.0),
            (1.0, 1.0),
        ],
        10 => &[
            (0.0, 0.0),
            (1.0, 0.0),
            (0.5, T / 2.0),
            (0.0, T),
            (1.0, T),
            (0.0, 2.0 * T),
            (1.0, 2.0 * T),
            (0.5, 1.0 - T / 2.0),
            (0.0, 1.0),
            (1.0, 1.0),
        ],
        _ => &[],
    }
}

/// The rank and suit in a corner, side by side so the strip of a card left
/// showing in a fanned column is enough to read it.
fn index(ctx: &Ctx, card: Card, w: f64, size: f64) {
    let rank = card.rank_str();
    let font = if rank.len() > 1 { size * 0.86 } else { size };
    ctx.set_font(&format!("700 {font:.1}px {SERIF}"));
    ctx.set_text_baseline("alphabetic");
    ctx.set_text_align("left");
    let x = w * 0.07;
    let baseline = size * 0.92 + w * 0.04;
    let _ = if rank.len() > 1 {
        // "10" in one glyph's width, so it does not crowd the suit.
        ctx.fill_text_with_max_width(rank, x - w * 0.01, baseline, size * 0.95)
    } else {
        ctx.fill_text(rank, x, baseline)
    };
    let advance = ctx
        .measure_text(rank)
        .map_or(size * 0.6, |m| m.width())
        .min(size * 0.95);
    let pip = size * 0.78;
    fill_suit(
        ctx,
        card.suit,
        x + advance + pip * 0.55,
        baseline - size * 0.36,
        pip,
        false,
    );
}

fn paint_face(ctx: &Ctx, card: Card, rect: Rect) {
    let (w, h) = (rect.w, rect.h);
    ctx.set_fill_style_str(ink(card.suit));

    if w < SMALL_CARD {
        // Small: one big index, one big suit — all that reads at this size,
        // and all a phone needs.
        index(ctx, card, w, w * 0.36);
        fill_suit(ctx, card.suit, w / 2.0, h * 0.64, w * 0.52, false);
        return;
    }

    let size = w * 0.22;
    index(ctx, card, w, size);
    // The same index upside down in the far corner.
    ctx.save();
    let _ = ctx.translate(w, h);
    let _ = ctx.rotate(PI);
    index(ctx, card, w, size);
    ctx.restore();

    // Kept clear of the corner indices, which run across rather than down.
    let left = w * 0.3;
    let right = w * 0.7;
    let top = h * 0.295;
    let bottom = h * 0.705;
    match card.rank {
        1 => {
            let big = if card.suit == Suit::Spades {
                0.56
            } else {
                0.44
            };
            fill_suit(ctx, card.suit, w / 2.0, h / 2.0, w * big, false);
        }
        2..=10 => {
            let pip = w * 0.17;
            for &(col, row) in pips(card.rank) {
                let x = left + (right - left) * col;
                let y = top + (bottom - top) * row;
                fill_suit(ctx, card.suit, x, y, pip, row > 0.5);
            }
        }
        _ => paint_court(ctx, card, rect),
    }
}

/// A court card: a tinted frame holding its letter, a crown over it, and a
/// suit pip at each end.
fn paint_court(ctx: &Ctx, card: Card, rect: Rect) {
    let (w, h) = (rect.w, rect.h);
    let frame = Rect::new(w * 0.2, h * 0.25, w * 0.6, h * 0.5);
    let tint = ctx.create_linear_gradient(0.0, frame.y, 0.0, frame.bottom());
    let (light, mid) = if card.suit.is_red() {
        ("#fff3ec", "#f7d9cf")
    } else {
        ("#f1f4fb", "#d8deee")
    };
    let _ = tint.add_color_stop(0.0, light);
    let _ = tint.add_color_stop(0.5, mid);
    let _ = tint.add_color_stop(1.0, light);
    rounded(ctx, frame, w * 0.04);
    ctx.set_fill_style_canvas_gradient(&tint);
    ctx.fill();
    ctx.set_line_width((w * 0.018).max(1.0));
    ctx.set_stroke_style_str("#c9a13b");
    ctx.stroke();
    let inner = Rect::new(
        frame.x + w * 0.03,
        frame.y + w * 0.03,
        frame.w - w * 0.06,
        frame.h - w * 0.06,
    );
    rounded(ctx, inner, w * 0.03);
    ctx.set_line_width((w * 0.008).max(0.6));
    ctx.set_stroke_style_str(ink(card.suit));
    ctx.stroke();

    let cx = w / 2.0;
    let colour = ink(card.suit);
    ctx.set_fill_style_str(colour);
    fill_suit(ctx, card.suit, cx, frame.y + h * 0.075, w * 0.12, false);
    fill_suit(
        ctx,
        card.suit,
        cx,
        frame.bottom() - h * 0.075,
        w * 0.12,
        true,
    );

    // The crown: a jack gets a band, a queen three points, a king five.
    let points = match card.rank {
        11 => 0,
        12 => 3,
        _ => 5,
    };
    let crown_w = w * 0.3;
    let base_y = h * 0.445;
    let top_y = base_y - h * 0.075;
    ctx.set_fill_style_str("#d4a72c");
    ctx.begin_path();
    ctx.move_to(cx - crown_w / 2.0, base_y);
    if points == 0 {
        ctx.line_to(cx - crown_w / 2.0, base_y - h * 0.035);
        ctx.line_to(cx + crown_w / 2.0, base_y - h * 0.035);
    } else {
        for i in 0..=points * 2 {
            let x = cx - crown_w / 2.0 + crown_w * f64::from(i) / f64::from(points * 2);
            let y = if i % 2 == 0 {
                top_y
            } else {
                base_y - h * 0.035
            };
            ctx.line_to(x, y);
        }
    }
    ctx.line_to(cx + crown_w / 2.0, base_y);
    ctx.close_path();
    ctx.fill();
    ctx.set_stroke_style_str("#8a6a14");
    ctx.set_line_width((w * 0.01).max(0.6));
    ctx.stroke();

    ctx.set_fill_style_str(colour);
    ctx.set_font(&format!("700 {:.1}px {SERIF}", w * 0.34));
    ctx.set_text_align("center");
    ctx.set_text_baseline("alphabetic");
    let _ = ctx.fill_text(card.rank_str(), cx, h * 0.645);
}
