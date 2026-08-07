//! Drawing the board, the status bar, and the overlays.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Padding, Paragraph, Widget, Wrap};

use super::layout::{BoardLayout, CARD_W, Density};
use crate::card::Card;
use crate::game::{Column, Game};

const BORDER: Color = Color::DarkGray;
const CURSOR: Color = Color::Cyan;
const SELECTED: Color = Color::Yellow;
const HINT: Color = Color::Green;
const CARD_BACK: Color = Color::Blue;
const MUTED: Color = Color::DarkGray;

const BACK_FACE: &str = "▞▞▞▞";
const CARET: &str = "▶";

/// The four rows a card box can need. Highlighted cards switch to a heavier
/// line so they stand out even where colour is washed out or absent.
struct Frame {
    first: &'static str,
    join: &'static str,
    last: &'static str,
    side: &'static str,
}

const LIGHT: Frame = Frame {
    first: "┌────┐",
    join: "├────┤",
    last: "└────┘",
    side: "│",
};
const DOUBLE: Frame = Frame {
    first: "╔════╗",
    join: "╠════╣",
    last: "╚════╝",
    side: "║",
};
const HEAVY: Frame = Frame {
    first: "┏━━━━┓",
    join: "┣━━━━┫",
    last: "┗━━━━┛",
    side: "┃",
};

/// Why a card is drawn differently from the rest of the board.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Accent {
    Plain,
    /// Under the cursor.
    Cursor,
    /// Part of the tail being carried.
    Selected,
    /// Where the current hint says to drop.
    Target,
}

impl Accent {
    fn frame(self) -> &'static Frame {
        match self {
            Self::Plain => &LIGHT,
            Self::Cursor | Self::Target => &DOUBLE,
            Self::Selected => &HEAVY,
        }
    }

    fn style(self) -> Style {
        let color = match self {
            Self::Plain => return Style::new().fg(BORDER),
            Self::Cursor => CURSOR,
            Self::Selected => SELECTED,
            Self::Target => HINT,
        };
        Style::new().fg(color).add_modifier(Modifier::BOLD)
    }

    /// Which accent wins a border row that two cards share.
    const fn weight(self) -> u8 {
        match self {
            Self::Plain => 0,
            Self::Target => 1,
            Self::Cursor => 2,
            Self::Selected => 3,
        }
    }
}

/// What the player is pointing at, holding, and being pointed towards.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Focus {
    pub column: usize,
    /// `None` when the focused column is empty.
    pub card: Option<usize>,
    /// The held tail, as `(column, first card)`.
    pub selection: Option<(usize, usize)>,
    /// The card the active hint points at, as `(column, card)`.
    pub hint_target: Option<(usize, usize)>,
}

impl Focus {
    /// The card each column must keep visible when it has to be elided.
    pub fn pins(&self) -> [Option<usize>; crate::game::COLUMNS] {
        let mut pins = [None; crate::game::COLUMNS];
        if let Some(card) = self.card {
            pins[self.column] = Some(card);
        }
        if let Some((column, index)) = self.selection {
            pins[column] = Some(index);
        }
        pins
    }

    fn accent(&self, column: usize, index: usize) -> Accent {
        if let Some((selected_column, first)) = self.selection
            && column == selected_column
            && index >= first
        {
            return Accent::Selected;
        }
        if self.column == column && self.card == Some(index) {
            return Accent::Cursor;
        }
        if self.hint_target == Some((column, index)) {
            return Accent::Target;
        }
        Accent::Plain
    }
}

/// Write `text` at `(x, y)`, ignoring anything outside the buffer.
fn put(buffer: &mut Buffer, x: u16, y: u16, text: &str, style: Style) {
    let area = buffer.area;
    if y >= area.top() && y < area.bottom() && x >= area.left() && x < area.right() {
        buffer.set_string(x, y, text, style);
    }
}

pub fn draw_board(buffer: &mut Buffer, layout: &BoardLayout, game: &Game, focus: &Focus) {
    for (index, column_layout) in layout.columns.iter().enumerate() {
        draw_column(
            buffer,
            column_layout,
            game.column(index),
            index,
            focus,
            layout.density,
        );
    }
}

/// Numbered labels above the board, with the cursor's column called out. These
/// double as the `1`-`7` shortcuts, so the player can see where those jump to.
pub fn draw_column_labels(buffer: &mut Buffer, layout: &BoardLayout, y: u16, active: usize) {
    for (index, column) in layout.columns.iter().enumerate() {
        let number = index + 1;
        let (label, style) = if index == active {
            (
                format!("[{number}]"),
                Style::new().fg(CURSOR).add_modifier(Modifier::BOLD),
            )
        } else {
            (number.to_string(), Style::new().fg(MUTED))
        };
        put(buffer, column.x, y, &format!("{label:^6}"), style);
    }
}

fn draw_column(
    buffer: &mut Buffer,
    layout: &super::layout::ColumnLayout,
    column: &Column,
    index: usize,
    focus: &Focus,
    density: Density,
) {
    let x = layout.x;

    if column.is_empty() {
        // An empty column is still a drop target, so the cursor and a hint
        // pointing at it both need to show.
        let accent = if focus.column == index {
            Accent::Cursor
        } else if focus.hint_target.is_some_and(|(c, _)| c == index) {
            Accent::Target
        } else {
            Accent::Plain
        };
        let frame = accent.frame();
        let style = accent.style();
        put(buffer, x, layout.top, frame.first, style);
        put(buffer, x, layout.top + 1, frame.side, style);
        put(buffer, x + CARD_W - 1, layout.top + 1, frame.side, style);
        put(buffer, x, layout.top + 2, frame.last, style);
        if accent == Accent::Cursor {
            draw_caret(buffer, x, layout.top + 1);
        }
        return;
    }

    let face = |card_index: usize| -> Option<Card> {
        column
            .is_face_up(card_index)
            .then(|| column.cards()[card_index])
    };
    let is_last = |card_index: usize| card_index + 1 == column.len();

    let accent_of = |position: usize| focus.accent(index, layout.slots[position].index);

    match density {
        // Cards overlap, so each border row is shared: it closes the card above
        // and opens the one below. Drawing it as a junction of the two is what
        // lets a highlighted card be boxed in on all four sides.
        Density::Roomy => {
            for (position, slot) in layout.slots.iter().enumerate() {
                let accent = accent_of(position);
                let above = position.checked_sub(1).map(accent_of);
                draw_junction(buffer, x, slot.top, above, Some(accent));
                draw_face(buffer, x, slot.top + 1, face(slot.index), accent);
                if is_last(slot.index) {
                    draw_junction(buffer, x, slot.top + 2, Some(accent), None);
                }
            }
        }
        // One shared top border, then a face per row — there is no row between
        // cards here, so a highlight shows in its sides alone.
        Density::Tight => {
            draw_junction(buffer, x, layout.top, None, Some(accent_of(0)));
            for slot in &layout.slots {
                let accent = focus.accent(index, slot.index);
                draw_face(buffer, x, slot.top, face(slot.index), accent);
                if is_last(slot.index) {
                    draw_junction(buffer, x, slot.top + 1, Some(accent), None);
                }
            }
            if let Some((hidden, row)) = layout.elision {
                let style = Style::new().fg(MUTED);
                put(buffer, x, row, LIGHT.side, style);
                put(
                    buffer,
                    x + 1,
                    row,
                    &format!("{:^4}", format!("⋯{hidden}")),
                    style,
                );
                put(buffer, x + CARD_W - 1, row, LIGHT.side, style);
            }
        }
    }
}

/// A border row shared by the card above it and the card below it.
///
/// `None` means there is no card on that side — the top or bottom of a column.
/// When the two disagree, the stronger accent claims the row and draws its own
/// edge there: a highlighted card closes itself rather than letting its plain
/// neighbour open over the top of it.
fn draw_junction(
    buffer: &mut Buffer,
    x: u16,
    y: u16,
    above: Option<Accent>,
    below: Option<Accent>,
) {
    let (accent, text) = match (above, below) {
        (Some(a), Some(b)) if a == b => (a, a.frame().join),
        (Some(a), Some(b)) if a.weight() >= b.weight() => (a, a.frame().last),
        (Some(_), Some(b)) => (b, b.frame().first),
        (Some(a), None) => (a, a.frame().last),
        (None, Some(b)) => (b, b.frame().first),
        (None, None) => return,
    };
    put(buffer, x, y, text, accent.style());
}

/// A pointer in the gutter beside the cursor, for when a colour or a heavier
/// border is not enough to find it on a crowded board.
fn draw_caret(buffer: &mut Buffer, card_x: u16, y: u16) {
    if let Some(x) = card_x.checked_sub(1) {
        put(
            buffer,
            x,
            y,
            CARET,
            Style::new().fg(CURSOR).add_modifier(Modifier::BOLD),
        );
    }
}

/// A card's face row: `│ K♠ │`, or a hatched back when face down.
fn draw_face(buffer: &mut Buffer, x: u16, y: u16, card: Option<Card>, accent: Accent) {
    let style = accent.style();
    put(buffer, x, y, accent.frame().side, style);
    put(buffer, x + CARD_W - 1, y, accent.frame().side, style);
    if accent == Accent::Cursor {
        draw_caret(buffer, x, y);
    }

    match card {
        Some(card) => {
            let color = if card.suit.is_red() {
                Color::Red
            } else {
                Color::Reset
            };
            put(buffer, x + 1, y, &card.face(), Style::new().fg(color));
        }
        None => put(buffer, x + 1, y, BACK_FACE, Style::new().fg(CARD_BACK)),
    }
}

/// Why the status line is saying something, which decides how it looks.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tone {
    /// A move was refused, or there is nothing to undo.
    #[default]
    Warn,
    /// A suggested move, matching the colour of the card it points at.
    Hint,
}

pub fn draw_status(
    buffer: &mut Buffer,
    area: Rect,
    game: &Game,
    focus: &Focus,
    message: &str,
    tone: Tone,
) {
    if area.height == 0 {
        return;
    }

    let stock = match game.stock_len() {
        0 => Span::styled("dealt", Style::new().fg(MUTED)),
        n => Span::styled("●".repeat(n), Style::new().fg(Color::Green)),
    };
    let left = Line::from(vec![
        Span::styled("seed ", Style::new().fg(MUTED)),
        Span::raw(game.seed().to_string()),
        Span::styled("  moves ", Style::new().fg(MUTED)),
        Span::raw(game.moves().to_string()),
        Span::styled("  stock ", Style::new().fg(MUTED)),
        stock,
    ]);
    let right = Line::from(Span::styled(
        "H hint   ? help   q quit",
        Style::new().fg(MUTED),
    ));

    let top = Rect { height: 1, ..area };
    Paragraph::new(left).render(top, buffer);
    Paragraph::new(right)
        .alignment(Alignment::Right)
        .render(top, buffer);

    if area.height < 2 {
        return;
    }
    let line = if !message.is_empty() {
        let color = match tone {
            Tone::Warn => Color::LightRed,
            Tone::Hint => HINT,
        };
        Span::styled(message, Style::new().fg(color))
    } else if let Some((column, index)) = focus.selection {
        let held = game.column(column).len() - index;
        Span::styled(
            format!(
                "holding {held} card{} — choose a column to drop on (esc to cancel)",
                if held == 1 { "" } else { "s" }
            ),
            Style::new().fg(SELECTED),
        )
    } else {
        Span::styled(
            "space picks up a card and everything below it",
            Style::new().fg(MUTED),
        )
    };
    Paragraph::new(Line::from(line)).render(
        Rect {
            y: area.y + 1,
            height: 1,
            ..area
        },
        buffer,
    );
}

pub fn draw_too_small(buffer: &mut Buffer, area: Rect) {
    let text = format!(
        "Terminal too small.\n\nNeed at least {}x{}, have {}x{}.",
        super::layout::BOARD_W,
        super::layout::MIN_BOARD_H + 2,
        area.width,
        area.height
    );
    Paragraph::new(text)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .render(area, buffer);
}

pub fn draw_win(buffer: &mut Buffer, area: Rect, game: &Game) {
    let popup = centered(44, 7, area);
    Clear.render(popup, buffer);

    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::new().fg(Color::Green))
        .padding(Padding::uniform(1));
    let text = vec![
        Line::from(Span::styled(
            "All four suits, King to Ace. You win!",
            Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("seed {} in {} moves", game.seed(), game.moves()),
            Style::new().fg(MUTED),
        )),
        Line::from(Span::styled(
            "n new game   r replay this deal   q quit",
            Style::new().fg(MUTED),
        )),
    ];
    Paragraph::new(text)
        .alignment(Alignment::Center)
        .block(block)
        .render(popup, buffer);
}

const HELP: &str = "\
Move any face-up card, together with every card below it,
onto a card of the same suit and one rank higher. The
cards you carry do not need to be in order. Only a King
may move to an empty column. Win by building all four
suits from King down to Ace.

  ← →  h l      move between columns
  ↑ ↓  k j      move up and down a column
  1 … 7         jump to a column, or drop a held tail there
  space enter   pick up the card under the cursor, or drop
  esc           put a held tail back
  d             deal the three stock cards (once per game)
  H             suggest a move; press again for the next one
  u  ctrl-r     undo / redo
  n  r          new game / replay this deal
  click         pick up a card, then click a column
  ?             close this help
  q             quit";

pub fn draw_help(buffer: &mut Buffer, area: Rect) {
    let popup = centered(62, 23, area);
    Clear.render(popup, buffer);

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(CURSOR))
        .title(" Scorpion Solitaire ")
        .padding(Padding::symmetric(1, 0));
    Paragraph::new(HELP).block(block).render(popup, buffer);
}

/// A `width` × `height` rect centred in `area`, clamped to fit.
fn centered(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(buffer: &Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_win_banner_names_the_deal() {
        let area = Rect::new(0, 0, 80, 24);
        let mut buffer = Buffer::empty(area);
        draw_win(&mut buffer, area, &Game::new(7));

        let view = text(&buffer);
        assert!(view.contains("You win!"), "got:\n{view}");
        assert!(view.contains("seed 7 in 0 moves"), "got:\n{view}");
    }

    #[test]
    fn overlays_shrink_to_fit_a_small_screen_instead_of_panicking() {
        for (width, height) in [(80, 24), (48, 8), (20, 4), (1, 1)] {
            let area = Rect::new(0, 0, width, height);
            let mut buffer = Buffer::empty(area);
            draw_help(&mut buffer, area);
            draw_win(&mut buffer, area, &Game::new(1));
            draw_too_small(&mut buffer, area);
        }
    }
}
