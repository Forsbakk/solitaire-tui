//! Drawing the board, the status bar, and the overlays.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Padding, Paragraph, Widget, Wrap};

use super::layout::{BoardLayout, CARD_W, Density, PileLayout};
use crate::board::{Board, Fan, PileId, PileKind};
use crate::card::Card;
use crate::rules::{Rules, StatusTone};

const BORDER: Color = Color::DarkGray;
const CURSOR: Color = Color::Cyan;
const SELECTED: Color = Color::Yellow;
const HINT: Color = Color::Green;
const CARD_BACK: Color = Color::Blue;
const MUTED: Color = Color::DarkGray;

const BACK_FACE: &str = "▞▞▞▞";
const CARET: &str = "▶";
/// Shown on an empty stock that can still be refilled from the waste.
const RECYCLE: &str = " ↻  ";
/// Shown on an empty free cell, which otherwise reads as one more empty box in
/// a row of them.
const FREE_CELL: &str = "free";

/// Keys that mean the same thing in every game. A game's own `help` adds to
/// this rather than repeating it.
///
/// These start on the opening quote rather than after a `\` continuation: that
/// escape swallows the *leading whitespace* of the line it joins, which would
/// quietly strip the first entry's indent and leave it hanging in the popup.
const SHARED_HELP: &str = "  ← →  h l      move between piles
  ↑ ↓  k j      move up and down a pile
  1 … 9  0      jump to a tableau pile, or drop there
  space enter   pick up the card under the cursor, or drop
  esc           put a held card back
  f             play the card home, or onto its only column";

const SHARED_HELP_TAIL: &str = "  H             suggest a move; press again for the next one
  u  ctrl-r     undo / redo
  n  r          new game / replay this deal
  m             back to the menu
  click         pick up a card, then click a pile
  ?             close this help
  q             quit";

/// Width of the help popup. Its key lines must not wrap — see `help_text`.
const HELP_W: u16 = 68;
/// The cells a help line has, once borders and padding are taken off.
pub const HELP_INNER_W: usize = HELP_W as usize - 4;

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
    pub pile: PileId,
    /// `None` when the focused pile is empty.
    pub card: Option<usize>,
    /// The held tail, as `(pile, first card)`.
    pub selection: Option<(PileId, usize)>,
    /// The card the active hint points at, as `(pile, card)`.
    pub hint_target: Option<(PileId, usize)>,
}

impl Focus {
    /// The card each pile must keep visible when it has to be elided.
    pub fn pins(&self, piles: usize) -> Vec<Option<usize>> {
        let mut pins = vec![None; piles];
        if let Some(card) = self.card {
            pins[self.pile] = Some(card);
        }
        if let Some((pile, index)) = self.selection {
            pins[pile] = Some(index);
        }
        pins
    }

    /// Whether the cursor is on this exact card. Kept separate from the accent
    /// because picking a card up turns it from `Cursor` to `Selected` — the
    /// caret has to stay put through that, or the cursor appears to vanish at
    /// the very moment the player needs to know where it is.
    fn is_cursor(&self, pile: PileId, index: usize) -> bool {
        self.pile == pile && self.card == Some(index)
    }

    fn accent(&self, pile: PileId, index: usize) -> Accent {
        if let Some((selected_pile, first)) = self.selection
            && pile == selected_pile
            && index >= first
        {
            return Accent::Selected;
        }
        if self.pile == pile && self.card == Some(index) {
            return Accent::Cursor;
        }
        if self.hint_target == Some((pile, index)) {
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

pub fn draw_board(
    buffer: &mut Buffer,
    layout: &BoardLayout,
    board: &Board,
    rules: &dyn Rules,
    focus: &Focus,
) {
    for pile_layout in &layout.piles {
        match pile_layout.fan {
            Fan::Down => draw_fanned(buffer, pile_layout, board, focus, layout.density),
            Fan::None | Fan::Right(_) => draw_flat(buffer, pile_layout, board, rules, focus),
        }
    }
}

/// Numbered labels directly above the tableau's columns, with the cursor's
/// column called out. These double as the digit shortcuts, so the player can
/// see where those jump to — which only works if they sit over the columns
/// they number, not over whichever row happens to be at the top of the board.
///
/// Each column is labelled once, over its top fanned pile, however many fanned
/// rows it runs through.
pub fn draw_labels(
    buffer: &mut Buffer,
    layout: &BoardLayout,
    columns: &[Vec<PileId>],
    active: PileId,
) {
    for (index, column) in columns.iter().enumerate() {
        // The row above the column: the board's own margin when the tableau is
        // the top row, and the gap between rows otherwise.
        let Some(pile_layout) = column.first().and_then(|&pile| layout.pile(pile)) else {
            continue;
        };
        let Some(y) = pile_layout.top.checked_sub(1) else {
            continue;
        };
        // The tenth column is labelled `0`, because `0` is the key that jumps
        // to it. Boards with fewer than ten columns are unaffected.
        let number = (index + 1) % 10;
        let (label, style) = if column.contains(&active) {
            (
                format!("[{number}]"),
                Style::new().fg(CURSOR).add_modifier(Modifier::BOLD),
            )
        } else {
            (number.to_string(), Style::new().fg(MUTED))
        };
        put(buffer, pile_layout.x, y, &format!("{label:^6}"), style);
    }
}

/// An empty pile, which is still a place the cursor can sit and cards can land.
fn draw_empty(buffer: &mut Buffer, x: u16, top: u16, accent: Accent, marker: Option<&str>) {
    let frame = accent.frame();
    let style = accent.style();
    put(buffer, x, top, frame.first, style);
    put(buffer, x, top + 1, frame.side, style);
    put(buffer, x + CARD_W - 1, top + 1, frame.side, style);
    put(buffer, x, top + 2, frame.last, style);
    if let Some(marker) = marker {
        put(buffer, x + 1, top + 1, marker, Style::new().fg(MUTED));
    }
    if accent == Accent::Cursor {
        draw_caret(buffer, x, top + 1);
    }
}

/// A whole card box, drawn on its own rather than overlapping a pile.
fn draw_card_box(
    buffer: &mut Buffer,
    x: u16,
    top: u16,
    card: Option<Card>,
    accent: Accent,
    caret: bool,
) {
    let style = accent.style();
    put(buffer, x, top, accent.frame().first, style);
    draw_face(buffer, x, top + 1, card, accent, caret);
    put(buffer, x, top + 2, accent.frame().last, style);
}

/// A pile drawn as a row of cards: stock, a foundation, or a waste showing the
/// last few cards a draw turned up.
fn draw_flat(
    buffer: &mut Buffer,
    layout: &PileLayout,
    board: &Board,
    rules: &dyn Rules,
    focus: &Focus,
) {
    let pile = board.pile(layout.pile);
    let Some((&(top_index, top_x), covered)) = layout.spread.split_last() else {
        // An empty stock that can be refilled says so, rather than looking
        // like a pile that has simply run out, and an empty cell says what it
        // is — otherwise a FreeCell board opens as eight identical boxes.
        let marker = match pile.kind() {
            PileKind::Stock if rules.activate(board, layout.pile).is_some() => Some(RECYCLE),
            PileKind::FreeCell => Some(FREE_CELL),
            _ => None,
        };
        let accent = if focus.pile == layout.pile {
            Accent::Cursor
        } else if focus.hint_target.is_some_and(|(p, _)| p == layout.pile) {
            Accent::Target
        } else {
            Accent::Plain
        };
        draw_empty(buffer, layout.x, layout.top, accent, marker);
        return;
    };

    let face = |index: usize| pile.is_face_up(index).then(|| pile.cards()[index]);

    // The covered cards first, so the exposed one overlaps them. They are drawn
    // plain whatever the cursor is doing — they cannot be taken, and accenting
    // them would suggest otherwise.
    // The covered cards first, so the exposed one overlaps them. Each keeps its
    // border and three cells of its face — the whole thing bar the trailing
    // space. They are drawn plain whatever the cursor is doing, since accenting
    // a card that cannot be taken would only mislead.
    for &(index, x) in covered {
        draw_card_box(buffer, x, layout.top, face(index), Accent::Plain, false);
    }
    draw_card_box(
        buffer,
        top_x,
        layout.top,
        face(top_index),
        focus.accent(layout.pile, top_index),
        // A fanned pile puts its caret at the pile's own left edge rather than
        // beside the exposed card, where it would sit on top of a covered one.
        covered.is_empty() && focus.is_cursor(layout.pile, top_index),
    );
    if !covered.is_empty() && focus.is_cursor(layout.pile, top_index) {
        draw_caret(buffer, layout.x, layout.top + 1);
    }
}

fn draw_fanned(
    buffer: &mut Buffer,
    layout: &PileLayout,
    board: &Board,
    focus: &Focus,
    density: Density,
) {
    let pile = board.pile(layout.pile);
    let x = layout.x;

    if pile.is_empty() {
        // An empty pile is still a drop target, so the cursor and a hint
        // pointing at it both need to show.
        let accent = if focus.pile == layout.pile {
            Accent::Cursor
        } else if focus.hint_target.is_some_and(|(p, _)| p == layout.pile) {
            Accent::Target
        } else {
            Accent::Plain
        };
        draw_empty(buffer, x, layout.top, accent, None);
        return;
    }

    let face = |card_index: usize| -> Option<Card> {
        pile.is_face_up(card_index)
            .then(|| pile.cards()[card_index])
    };
    let is_last = |card_index: usize| card_index + 1 == pile.len();
    let accent_of = |position: usize| focus.accent(layout.pile, layout.slots[position].index);

    match density {
        // Cards overlap, so each border row is shared: it closes the card above
        // and opens the one below. Drawing it as a junction of the two is what
        // lets a highlighted card be boxed in on all four sides.
        Density::Roomy => {
            for (position, slot) in layout.slots.iter().enumerate() {
                let accent = accent_of(position);
                let above = position.checked_sub(1).map(accent_of);
                draw_junction(buffer, x, slot.top, above, Some(accent));
                let caret = focus.is_cursor(layout.pile, slot.index);
                draw_face(buffer, x, slot.top + 1, face(slot.index), accent, caret);
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
                let accent = focus.accent(layout.pile, slot.index);
                let caret = focus.is_cursor(layout.pile, slot.index);
                draw_face(buffer, x, slot.top, face(slot.index), accent, caret);
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
/// `None` means there is no card on that side — the top or bottom of a pile.
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
///
/// `caret` is passed rather than derived from `accent`, so the gutter pointer
/// stays on the cursor's card even once it has been picked up.
fn draw_face(buffer: &mut Buffer, x: u16, y: u16, card: Option<Card>, accent: Accent, caret: bool) {
    let style = accent.style();
    put(buffer, x, y, accent.frame().side, style);
    put(buffer, x + CARD_W - 1, y, accent.frame().side, style);
    if caret {
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
    board: &Board,
    rules: &dyn Rules,
    focus: &Focus,
    message: &str,
    tone: Tone,
) {
    if area.height == 0 {
        return;
    }

    let mut spans = vec![
        Span::styled("seed ", Style::new().fg(MUTED)),
        Span::raw(board.seed().to_string()),
        Span::styled("  moves ", Style::new().fg(MUTED)),
        Span::raw(board.moves().to_string()),
    ];
    for item in rules.status(board) {
        spans.push(Span::styled(
            format!("  {} ", item.label),
            Style::new().fg(MUTED),
        ));
        spans.push(Span::styled(
            item.value,
            match item.tone {
                StatusTone::Good => Style::new().fg(Color::Green),
                StatusTone::Muted => Style::new().fg(MUTED),
            },
        ));
    }

    let top = Rect { height: 1, ..area };
    Paragraph::new(Line::from(spans)).render(top, buffer);
    Paragraph::new(Line::from(Span::styled(
        "H hint   ? help   q quit",
        Style::new().fg(MUTED),
    )))
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
    } else if let Some((pile, index)) = focus.selection {
        let held = board.pile(pile).len() - index;
        // A card with only one place to go can be played without hunting for
        // which pile that is, so say so rather than leaving it to the help.
        let home = if held == 1 && rules.send_home(board, pile).is_some() {
            ", or f to send it home"
        } else if rules.only_move(board, pile, index).is_some() {
            ", or f to play it"
        } else {
            ""
        };
        Span::styled(
            format!(
                "holding {held} card{} — choose a pile to drop on{home} (esc to cancel)",
                if held == 1 { "" } else { "s" }
            ),
            Style::new().fg(SELECTED),
        )
    } else {
        Span::styled(
            "space picks up a card, then choose where to drop it",
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

pub fn draw_too_small(buffer: &mut Buffer, area: Rect, need_w: u16, need_h: u16) {
    let text = format!(
        "Terminal too small.\n\nNeed at least {}x{}, have {}x{}.",
        need_w, need_h, area.width, area.height
    );
    Paragraph::new(text)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .render(area, buffer);
}

pub fn draw_win(buffer: &mut Buffer, area: Rect, board: &Board, rules: &dyn Rules) {
    let popup = centered(44, 7, area);
    Clear.render(popup, buffer);

    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::new().fg(Color::Green))
        .padding(Padding::uniform(1));
    let text = vec![
        Line::from(Span::styled(
            format!("{} solved. You win!", rules.name()),
            Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("seed {} in {} moves", board.seed(), board.moves()),
            Style::new().fg(MUTED),
        )),
        Line::from(Span::styled(
            "n new game   r replay this deal   m menu   q quit",
            Style::new().fg(MUTED),
        )),
    ];
    Paragraph::new(text)
        .alignment(Alignment::Center)
        .block(block)
        .render(popup, buffer);
}

/// The full help text for `rules`: its own rules and keys, wrapped in the ones
/// every game shares.
pub fn help_text(rules: &dyn Rules) -> String {
    let mut text = String::new();
    for line in rules.blurb().lines() {
        text.push_str(line);
        text.push('\n');
    }
    text.push('\n');
    text.push_str(SHARED_HELP);
    text.push('\n');
    let own = rules.help();
    if !own.is_empty() {
        text.push_str(own);
        text.push('\n');
    }
    text.push_str(SHARED_HELP_TAIL);
    text
}

pub fn draw_help(buffer: &mut Buffer, area: Rect, rules: &dyn Rules) {
    let text = help_text(rules);
    // Counting raw lines would undercount: the blurb wraps, and the popup
    // would then clip its last few key bindings.
    let height = wrapped_height(&text, HELP_INNER_W as u16) + 2;
    let popup = centered(HELP_W, height, area);
    Clear.render(popup, buffer);

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(CURSOR))
        .title(format!(" {} ", rules.name()))
        .padding(Padding::symmetric(1, 0));
    Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .block(block)
        .render(popup, buffer);
}

/// How many rows `text` needs once wrapped to `width`.
pub fn wrapped_height(text: &str, width: u16) -> u16 {
    if width == 0 {
        return 0;
    }
    text.lines()
        .map(|line| {
            let mut rows = 1;
            let mut used = 0;
            for word in line.split_whitespace() {
                let len = word.chars().count() as u16;
                if used == 0 {
                    used = len;
                } else if used + 1 + len <= width {
                    used += 1 + len;
                } else {
                    rows += 1;
                    used = len;
                }
            }
            rows
        })
        .sum()
}

/// A `width` × `height` rect centred in `area`, clamped to fit.
pub fn centered(width: u16, height: u16, area: Rect) -> Rect {
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
    use crate::rules::games;
    use crate::rules::scorpion::Scorpion;

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
    fn the_win_banner_names_the_game_and_the_deal() {
        let area = Rect::new(0, 0, 80, 24);
        let mut buffer = Buffer::empty(area);
        draw_win(&mut buffer, area, &Scorpion.deal(7), &Scorpion);

        let view = text(&buffer);
        assert!(view.contains("You win!"), "got:\n{view}");
        assert!(view.contains("Scorpion"), "got:\n{view}");
        assert!(view.contains("seed 7 in 0 moves"), "got:\n{view}");
    }

    #[test]
    fn an_empty_free_cell_says_what_it_is() {
        // Four cells and four foundations sit side by side in one row, and
        // empty they are the same box. The marker is the only thing that tells
        // a player which half of the board they are looking at.
        let rules = crate::rules::freecell::FreeCell;
        let board = rules.deal(42);
        let spec = rules.spec(&board);
        let lengths: Vec<usize> = board.piles().iter().map(|pile| pile.len()).collect();
        let area = Rect::new(0, 0, 80, 28);
        let layout =
            crate::ui::layout::compute(&spec, &lengths, area, &vec![None; lengths.len()]).unwrap();

        let mut buffer = Buffer::empty(area);
        let focus = Focus {
            pile: 0,
            card: None,
            selection: None,
            hint_target: None,
        };
        draw_board(&mut buffer, &layout, &board, &rules, &focus);

        let view = text(&buffer);
        assert_eq!(view.matches(FREE_CELL).count(), 4, "got:\n{view}");
    }

    #[test]
    fn every_help_key_line_fits_the_popup_and_keeps_its_indent() {
        // A key line that overflows gets wrapped, and the wrapped remainder
        // lands hard against the left border with the columns broken.
        for game in games() {
            for line in help_text(game.as_ref()).lines() {
                let Some(rest) = line.strip_prefix("  ") else {
                    continue; // blurb prose, which is meant to wrap
                };
                assert!(
                    !rest.starts_with(' '),
                    "{}: {line:?} is indented past its key column",
                    game.name()
                );
                assert!(
                    line.chars().count() <= HELP_INNER_W,
                    "{}: {line:?} is {} cells, over the {HELP_INNER_W} that fit",
                    game.name(),
                    line.chars().count()
                );
            }
        }
    }

    #[test]
    fn help_carries_the_shared_keys_and_the_games_own() {
        for game in games() {
            let text = help_text(game.as_ref());
            assert!(text.contains("undo / redo"), "{}", game.name());
            assert!(text.contains("back to the menu"), "{}", game.name());
            // `f` works in every game now, so it is listed once, by the shared
            // block — a game repeating it would print it twice.
            assert_eq!(
                text.matches("  f  ").count(),
                1,
                "{} lists f once:\n{text}",
                game.name()
            );
            for line in game.help().lines() {
                assert!(text.contains(line), "{} lost {line:?}", game.name());
            }
        }
    }

    #[test]
    fn overlays_shrink_to_fit_a_small_screen_instead_of_panicking() {
        for (width, height) in [(80, 24), (48, 8), (20, 4), (1, 1)] {
            let area = Rect::new(0, 0, width, height);
            let mut buffer = Buffer::empty(area);
            draw_help(&mut buffer, area, &Scorpion);
            draw_win(&mut buffer, area, &Scorpion.deal(1), &Scorpion);
            draw_too_small(&mut buffer, area, 48, 8);
        }
    }
}
