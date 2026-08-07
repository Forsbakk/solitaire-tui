//! Cursor, selection, undo history, and input handling.

use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::Frame;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

use crate::game::{Action, COLUMNS, Game, MoveError, Suggestion};
use crate::ui::layout::{self, BOARD_W, BoardLayout, MIN_BOARD_H};
use crate::ui::render::{self, Focus, Tone};

/// Rows reserved at the bottom for the status bar.
const STATUS_H: u16 = 2;

pub struct App {
    game: Game,
    cursor_column: usize,
    cursor_card: Option<usize>,
    /// The held tail, as `(column, first card)`.
    selection: Option<(usize, usize)>,
    /// The move the last `H` press suggested, still shown on the board.
    hint: Option<Suggestion>,
    history: Vec<Action>,
    future: Vec<Action>,
    message: String,
    tone: Tone,
    show_help: bool,
    /// The layout the last frame was drawn from, used to resolve clicks.
    layout: Option<BoardLayout>,
    pub quit: bool,
}

impl App {
    pub fn new(seed: u64) -> Self {
        let mut app = Self {
            game: Game::new(seed),
            cursor_column: 0,
            cursor_card: None,
            selection: None,
            hint: None,
            history: Vec::new(),
            future: Vec::new(),
            message: String::new(),
            tone: Tone::Warn,
            show_help: false,
            layout: None,
            quit: false,
        };
        app.snap_cursor();
        app
    }

    fn focus(&self) -> Focus {
        Focus {
            column: self.cursor_column,
            card: self.cursor_card,
            selection: self.selection,
            // A hint points at the card the tail would land on, which is
            // whatever is exposed on the target column.
            hint_target: self.hint.map(|suggestion| {
                (
                    suggestion.to,
                    self.game.column(suggestion.to).len().saturating_sub(1),
                )
            }),
        }
    }

    // -- drawing ----------------------------------------------------------

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        if area.width < BOARD_W || area.height < MIN_BOARD_H + STATUS_H {
            self.layout = None;
            render::draw_too_small(frame.buffer_mut(), area);
            return;
        }

        // The top row carries the column numbers when there is height to
        // spare, and is given back to the board when there is not.
        let margin = u16::from(area.height > MIN_BOARD_H + STATUS_H + 1);
        let board = Rect {
            y: area.y + margin,
            height: area.height - STATUS_H - margin,
            ..area
        };
        let status = Rect {
            y: area.y + area.height - STATUS_H,
            height: STATUS_H,
            ..area
        };

        let focus = self.focus();
        let lengths = std::array::from_fn(|i| self.game.column(i).len());
        self.layout = layout::compute(&lengths, board, &focus.pins());

        if let Some(board_layout) = &self.layout {
            render::draw_board(frame.buffer_mut(), board_layout, &self.game, &focus);
            if margin > 0 {
                render::draw_column_labels(
                    frame.buffer_mut(),
                    board_layout,
                    area.y,
                    self.cursor_column,
                );
            }
        }
        render::draw_status(
            frame.buffer_mut(),
            status,
            &self.game,
            &focus,
            &self.message,
            self.tone,
        );

        if self.show_help {
            render::draw_help(frame.buffer_mut(), area);
        } else if self.game.is_won() {
            render::draw_win(frame.buffer_mut(), area, &self.game);
        }
    }

    // -- input ------------------------------------------------------------

    pub fn handle(&mut self, event: Event) {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key(key),
            Event::Mouse(mouse) => self.on_mouse(mouse),
            _ => {}
        }
    }

    fn on_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && matches!(key.code, KeyCode::Char('c')) {
            self.quit = true;
            return;
        }

        // Any key dismisses help, so it never traps the player.
        if self.show_help {
            self.show_help = false;
            return;
        }

        // The hint stays up while the player acts on it — pressing H advances
        // it, and picking the card up keeps the target marked all the way to
        // the drop. Moving the cursor elsewhere means they have moved on.
        let acting_on_hint = matches!(
            key.code,
            KeyCode::Char('H') | KeyCode::Char(' ') | KeyCode::Enter
        ) || self.selection.is_some();
        if !acting_on_hint {
            self.clear_hint();
        }

        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('H') => self.suggest(),
            KeyCode::Char('r') if ctrl => self.redo(),
            KeyCode::Char('u') => self.undo(),
            KeyCode::Char('n') => self.restart(random_seed()),
            KeyCode::Char('r') => self.restart(self.game.seed()),
            KeyCode::Char('d') => self.deal_stock(),
            KeyCode::Esc => {
                self.selection = None;
                self.clear_hint();
                self.message.clear();
            }
            KeyCode::Left | KeyCode::Char('h') => self.step_column(-1),
            KeyCode::Right | KeyCode::Char('l') => self.step_column(1),
            KeyCode::Up | KeyCode::Char('k') => self.step_card(-1),
            KeyCode::Down | KeyCode::Char('j') => self.step_card(1),
            KeyCode::Char(' ') | KeyCode::Enter => self.activate(),
            KeyCode::Char(digit @ '1'..='7') => {
                let column = digit as usize - '1' as usize;
                if self.selection.is_some() {
                    self.drop_on(column);
                } else {
                    self.cursor_column = column;
                    self.snap_cursor();
                }
            }
            _ => {}
        }
    }

    fn on_mouse(&mut self, mouse: MouseEvent) {
        if self.show_help {
            if matches!(mouse.kind, MouseEventKind::Down(_)) {
                self.show_help = false;
            }
            return;
        }
        self.clear_hint();

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let hit = self
                    .layout
                    .as_ref()
                    .and_then(|layout| layout.hit(mouse.column, mouse.row));
                let Some(hit) = hit else { return };

                if self.selection.is_some() {
                    self.drop_on(hit.column);
                    return;
                }
                self.cursor_column = hit.column;
                match hit.card {
                    Some(index) if self.game.column(hit.column).is_face_up(index) => {
                        self.cursor_card = Some(index);
                        self.select(index);
                    }
                    Some(_) => {
                        self.snap_cursor();
                        self.warn(MoveError::FaceDown.message());
                    }
                    None => self.snap_cursor(),
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                self.selection = None;
                self.message.clear();
            }
            MouseEventKind::ScrollUp => self.step_card(-1),
            MouseEventKind::ScrollDown => self.step_card(1),
            _ => {}
        }
    }

    // -- cursor -----------------------------------------------------------

    /// Put the cursor on the current column's exposed card.
    fn snap_cursor(&mut self) {
        let column = self.game.column(self.cursor_column);
        self.cursor_card = column.len().checked_sub(1);
    }

    /// Keep the cursor on a real face-up card after the board changes.
    fn clamp_cursor(&mut self) {
        let column = self.game.column(self.cursor_column);
        self.cursor_card = match (column.first_face_up(), column.len()) {
            (Some(first), len) => Some(self.cursor_card.unwrap_or(len - 1).clamp(first, len - 1)),
            _ => None,
        };
    }

    fn step_column(&mut self, delta: isize) {
        let next = self.cursor_column as isize + delta;
        self.cursor_column = next.rem_euclid(COLUMNS as isize) as usize;
        self.snap_cursor();
    }

    fn step_card(&mut self, delta: isize) {
        let column = self.game.column(self.cursor_column);
        let (Some(first), Some(current)) = (column.first_face_up(), self.cursor_card) else {
            return;
        };
        let target = (current as isize + delta).clamp(first as isize, column.len() as isize - 1);
        self.cursor_card = Some(target as usize);
    }

    // -- moves ------------------------------------------------------------

    fn activate(&mut self) {
        if self.selection.is_some() {
            self.drop_on(self.cursor_column);
            return;
        }
        match self.cursor_card {
            Some(index) => self.select(index),
            None => self.warn("That column is empty."),
        }
    }

    fn select(&mut self, index: usize) {
        self.selection = Some((self.cursor_column, index));
        self.message.clear();
    }

    fn drop_on(&mut self, target: usize) {
        let Some((from, index)) = self.selection else {
            return;
        };
        match self.game.apply_move(from, index, target) {
            Ok(action) => {
                self.history.push(action);
                self.future.clear();
                self.selection = None;
                // Whatever was suggested has either just happened or gone
                // stale, so stop pointing at it.
                self.hint = None;
                self.cursor_column = target;
                self.snap_cursor();
                self.message.clear();
                self.report_position();
            }
            Err(error) => self.warn(error.message()),
        }
    }

    fn deal_stock(&mut self) {
        match self.game.deal_stock() {
            Some(action) => {
                self.history.push(action);
                self.future.clear();
                self.clamp_cursor();
                self.message.clear();
                self.report_position();
            }
            None => self.warn("The stock has already been dealt."),
        }
    }

    fn undo(&mut self) {
        match self.history.pop() {
            Some(action) => {
                self.game.undo(action);
                self.future.push(action);
                self.selection = None;
                self.clamp_cursor();
                self.message.clear();
            }
            None => self.warn("Nothing to undo."),
        }
    }

    fn redo(&mut self) {
        match self.future.pop() {
            Some(action) => {
                self.game.redo(action);
                self.history.push(action);
                self.selection = None;
                self.clamp_cursor();
                self.message.clear();
                self.report_position();
            }
            None => self.warn("Nothing to redo."),
        }
    }

    fn restart(&mut self, seed: u64) {
        *self = Self::new(seed);
    }

    // -- hints ------------------------------------------------------------

    /// Point at a move worth making. Pressing again walks through the rest,
    /// best first, so a rejected suggestion is not a dead end.
    fn suggest(&mut self) {
        let suggestions = self.game.suggestions();
        let Some(first) = suggestions.first().copied() else {
            self.hint = None;
            self.warn(if self.game.stock_len() > 0 {
                "No moves — press d to deal the stock."
            } else {
                "No moves left — u to undo, n for a new deal."
            });
            return;
        };

        // Advance past whatever was suggested last time, wrapping around.
        let next = match self.hint {
            Some(current) => suggestions
                .iter()
                .position(|s| *s == current)
                .map_or(first, |i| suggestions[(i + 1) % suggestions.len()]),
            None => first,
        };

        // Put the cursor on the card being suggested, so accepting the hint is
        // just space then the target column.
        self.selection = None;
        self.cursor_column = next.from;
        self.cursor_card = Some(next.index);
        self.hint = Some(next);

        let card = self.game.column(next.from).cards()[next.index];
        let position = suggestions.iter().position(|s| *s == next).unwrap_or(0) + 1;
        let count = suggestions.len();
        self.message = match self.game.column(next.to).exposed() {
            Some(target) => format!(
                "hint {position}/{count}: {card} onto {target} in column {}",
                next.to + 1
            ),
            None => format!(
                "hint {position}/{count}: {card} to the empty column {}",
                next.to + 1
            ),
        };
        self.tone = Tone::Hint;
    }

    /// Drop the highlight, and the message explaining it along with it.
    fn clear_hint(&mut self) {
        if self.hint.take().is_some() {
            self.message.clear();
        }
    }

    fn warn(&mut self, message: &str) {
        self.message = message.to_string();
        self.tone = Tone::Warn;
    }

    /// Warn when the deal is dead, so the player is not left hunting.
    fn report_position(&mut self) {
        if !self.game.is_won() && !self.game.has_progress_move() {
            self.warn("No moves left — u to undo, n for a new deal.");
        }
    }
}

/// A seed for a fresh deal. Only needs to differ run to run.
pub fn random_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyEventState;

    /// Render `app` at the given size and return the screen as text.
    fn screen(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// How many columns drew a box, counted from the board's top border row.
    ///
    /// Counting closing borders instead would over-count: a highlighted card
    /// in the middle of a column closes itself off from its neighbours.
    fn columns_drawn(view: &str) -> usize {
        view.lines()
            .find(|line| line.contains(['┐', '╗', '┓']))
            .map(|line| line.matches(['┐', '╗', '┓']).count())
            .unwrap_or(0)
    }

    fn press(app: &mut App, code: KeyCode) {
        app.handle(Event::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
    }

    #[test]
    fn the_cursor_starts_on_an_exposed_card_and_stays_face_up() {
        let mut app = App::new(3);
        assert_eq!(app.cursor_card, Some(6));

        // Column 0 has three face-down cards; walking up must stop above them.
        for _ in 0..20 {
            press(&mut app, KeyCode::Up);
        }
        assert_eq!(app.cursor_card, Some(3));

        press(&mut app, KeyCode::Right);
        assert_eq!(app.cursor_column, 1);
        assert_eq!(
            app.cursor_card,
            Some(6),
            "each column snaps to its exposed card"
        );
    }

    #[test]
    fn columns_wrap_in_both_directions() {
        let mut app = App::new(3);
        press(&mut app, KeyCode::Left);
        assert_eq!(app.cursor_column, COLUMNS - 1);
        press(&mut app, KeyCode::Right);
        assert_eq!(app.cursor_column, 0);
    }

    #[test]
    fn an_illegal_drop_keeps_the_tail_in_hand() {
        let mut app = App::new(3);
        press(&mut app, KeyCode::Char(' '));
        let held = app.selection.expect("picked up a card");

        // Column 1's exposed card almost certainly does not accept it.
        press(&mut app, KeyCode::Char('2'));
        if app.selection.is_some() {
            assert_eq!(app.selection, Some(held), "still holding the same tail");
            assert!(!app.message.is_empty(), "and the player is told why");
        }
    }

    #[test]
    fn escape_puts_a_held_tail_back() {
        let mut app = App::new(3);
        press(&mut app, KeyCode::Char(' '));
        assert!(app.selection.is_some());
        press(&mut app, KeyCode::Esc);
        assert!(app.selection.is_none());
    }

    #[test]
    fn undo_and_redo_walk_the_history() {
        let mut app = App::new(3);
        let start = app.game.clone();

        press(&mut app, KeyCode::Char('d'));
        assert_eq!(app.history.len(), 1);
        assert_ne!(app.game, start);

        press(&mut app, KeyCode::Char('u'));
        assert_eq!(app.game, start);
        assert!(app.history.is_empty());

        app.redo();
        assert_eq!(app.history.len(), 1);
        assert_ne!(app.game, start);
    }

    #[test]
    fn a_new_move_discards_the_redo_future() {
        let mut app = App::new(3);
        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Char('u'));
        assert_eq!(app.future.len(), 1);

        press(&mut app, KeyCode::Char('d'));
        assert!(app.future.is_empty());
    }

    #[test]
    fn replay_restores_the_same_deal_and_new_changes_it() {
        let mut app = App::new(3);
        let deal = app.game.clone();

        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Char('r'));
        assert_eq!(app.game, deal, "r replays the same seed");
        assert!(app.history.is_empty());

        press(&mut app, KeyCode::Char('n'));
        assert_ne!(app.game.seed(), deal.seed());
    }

    #[test]
    fn help_opens_and_the_next_key_closes_it() {
        let mut app = App::new(3);
        press(&mut app, KeyCode::Char('?'));
        assert!(app.show_help);
        press(&mut app, KeyCode::Char('j'));
        assert!(!app.show_help);
        assert!(!app.quit, "the dismissing key does nothing else");
    }

    #[test]
    fn undoing_past_the_start_is_reported_not_ignored() {
        let mut app = App::new(3);
        press(&mut app, KeyCode::Char('u'));
        assert!(app.message.contains("Nothing to undo"));
    }

    #[test]
    fn the_board_renders_at_a_comfortable_size() {
        let mut app = App::new(42);
        let view = screen(&mut app, 80, 30);
        println!("{view}");

        assert!(view.contains("┌────┐"), "cards are drawn as boxes");
        assert!(view.contains("▞▞▞▞"), "face-down cards are hatched");
        assert!(view.contains("seed 42"), "the status bar names the deal");
        assert_eq!(columns_drawn(&view), COLUMNS, "every column is drawn");
    }

    #[test]
    fn a_short_terminal_still_shows_every_column() {
        let mut app = App::new(42);
        let view = screen(&mut app, 80, 14);
        println!("{view}");

        assert!(view.contains("seed 42"), "the status bar survives");
        assert_eq!(columns_drawn(&view), COLUMNS);
    }

    #[test]
    fn columns_that_cannot_fit_are_elided_but_keep_their_exposed_card() {
        let mut app = App::new(42);
        let view = screen(&mut app, 80, 9);
        println!("{view}");

        assert!(view.contains("⋯"), "the hidden cards are counted");
        assert_eq!(columns_drawn(&view), COLUMNS);
        // Column 7's exposed card is Q♦ in this deal, and it must stay visible
        // because it is the only card that can be dropped on.
        assert!(view.contains("Q♦"), "the exposed card survives elision");
    }

    /// The rows a column occupies on screen, as plain text.
    fn column_strip(view: &str, column: usize) -> Vec<String> {
        let x = view
            .lines()
            .find(|line| line.contains(['┐', '╗', '┓']))
            .map(|line| line.chars().take_while(|c| *c != '┌').count())
            .unwrap_or(0)
            + column * 7;
        view.lines()
            .map(|line| line.chars().skip(x).take(6).collect::<String>())
            .filter(|cell| cell.contains(['│', '║', '┃', '─', '━', '═']))
            .collect()
    }

    #[test]
    fn a_highlighted_card_is_boxed_in_even_mid_column() {
        let mut app = App::new(42);
        // Column 5 is face up throughout; sit on its middle card.
        press(&mut app, KeyCode::Char('5'));
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Up);

        let view = screen(&mut app, 80, 30);
        let strip = column_strip(&view, 4);
        println!("{}", strip.join("\n"));

        // The cursor card owns the row above it *and* the row below it, so it
        // reads as one closed box rather than an open-bottomed one.
        let top = strip
            .iter()
            .position(|row| row.contains('╗'))
            .expect("the cursor card opens");
        assert!(strip[top + 1].contains('║'), "its face has heavy sides");
        assert!(
            strip[top + 2].contains('╝'),
            "and it closes underneath: {:?}",
            &strip[top..],
        );
        assert!(
            !strip[top + 2].contains('┤'),
            "the next card must not open over the top of it"
        );
    }

    #[test]
    fn a_tiny_terminal_says_so_instead_of_drawing_a_broken_board() {
        let mut app = App::new(42);
        let view = screen(&mut app, 30, 10);
        assert!(view.contains("too small"), "got:\n{view}");
    }

    #[test]
    fn holding_a_tail_marks_every_card_being_carried() {
        let mut app = App::new(42);
        press(&mut app, KeyCode::Char('5'));
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Char(' '));

        let view = screen(&mut app, 80, 30);
        println!("{view}");
        assert!(view.contains("holding 3 cards"), "got:\n{view}");
        // Three carried cards, each with a heavy side on the left and right.
        assert_eq!(view.matches('┃').count(), 6, "got:\n{view}");
        assert_eq!(view.matches("┗━━━━┛").count(), 1, "the tail closes once");
    }

    fn click(app: &mut App, x: u16, y: u16) {
        app.handle(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }));
    }

    /// Find where `text` was drawn on screen.
    fn find(view: &str, text: &str) -> (u16, u16) {
        view.lines()
            .enumerate()
            .find_map(|(y, line)| {
                line.find(text).map(|byte| {
                    let x = line[..byte].chars().count();
                    (x as u16, y as u16)
                })
            })
            .unwrap_or_else(|| panic!("{text} is not on screen:\n{view}"))
    }

    #[test]
    fn clicking_a_card_picks_up_the_card_that_was_drawn_there() {
        let mut app = App::new(42);
        // Locate a card by where it actually appears, not by asking the layout
        // — that is what makes this a check of render/hit-test agreement.
        let view = screen(&mut app, 80, 30);
        let (x, y) = find(&view, "K♦");

        click(&mut app, x, y);
        let (column, index) = app.selection.expect("clicking a face-up card picks it up");
        assert_eq!(app.game.column(column).cards()[index].to_string(), "K♦");
    }

    #[test]
    fn clicking_a_face_down_card_explains_itself_and_holds_nothing() {
        let mut app = App::new(42);
        let view = screen(&mut app, 80, 30);
        let (x, y) = find(&view, "▞▞▞▞");

        click(&mut app, x, y);
        assert!(app.selection.is_none());
        assert_eq!(app.message, MoveError::FaceDown.message());
    }

    #[test]
    fn a_second_click_drops_onto_the_column_clicked() {
        let mut app = App::new(42);
        let view = screen(&mut app, 80, 30);

        // In this deal 9♣ sits face up with 10♣ exposed on another column, so
        // the 9♣ and everything below it may move across.
        let (x, y) = find(&view, "9♣");
        click(&mut app, x, y);
        assert!(app.selection.is_some());

        let (target_x, target_y) = find(&view, "10♣");
        click(&mut app, target_x, target_y);
        assert!(app.selection.is_none(), "the tail was released");
        assert_eq!(app.history.len(), 1, "and the move was recorded");
    }

    #[test]
    fn the_keyboard_can_play_a_move_end_to_end() {
        let mut app = App::new(42);

        // Column 6, up to the 9♣, pick it up, drop it on column 4's 10♣.
        press(&mut app, KeyCode::Char('6'));
        for _ in 0..3 {
            press(&mut app, KeyCode::Up);
        }
        assert_eq!(
            app.game.column(5).cards()[app.cursor_card.unwrap()].to_string(),
            "9♣"
        );

        press(&mut app, KeyCode::Char(' '));
        press(&mut app, KeyCode::Char('4'));

        assert_eq!(
            app.history.len(),
            1,
            "the move went through: {}",
            app.message
        );
        assert_eq!(app.game.column(5).len(), 3, "the tail left column 6");
        assert_eq!(app.game.column(3).exposed().unwrap().to_string(), "6♠");
        assert_eq!(app.cursor_column, 3, "the cursor follows the cards");
        assert!(app.selection.is_none());
    }

    #[test]
    fn a_hint_moves_the_cursor_onto_a_move_that_can_be_played() {
        let mut app = App::new(42);
        press(&mut app, KeyCode::Char('H'));

        let suggestion = app.hint.expect("a hint was offered");
        assert_eq!(app.cursor_column, suggestion.from);
        assert_eq!(app.cursor_card, Some(suggestion.index));
        assert_eq!(app.tone, Tone::Hint);
        assert!(app.message.starts_with("hint 1/"), "got {:?}", app.message);

        // Accepting the hint is just: pick up, then name the target column.
        press(&mut app, KeyCode::Char(' '));
        app.drop_on(suggestion.to);
        assert_eq!(app.history.len(), 1, "{}", app.message);
    }

    #[test]
    fn pressing_hint_again_walks_through_the_alternatives() {
        let mut app = App::new(42);
        let total = app.game.suggestions().len();
        assert!(total > 1, "this deal offers several moves");

        let mut seen = Vec::new();
        for _ in 0..total {
            press(&mut app, KeyCode::Char('H'));
            seen.push(app.hint.expect("still hinting"));
        }
        assert_eq!(
            seen.iter().collect::<std::collections::HashSet<_>>().len(),
            total,
            "every suggestion is offered once before repeating"
        );

        press(&mut app, KeyCode::Char('H'));
        assert_eq!(app.hint, Some(seen[0]), "then it wraps around");
    }

    #[test]
    fn the_hint_highlight_clears_as_soon_as_the_player_does_anything_else() {
        let mut app = App::new(42);
        press(&mut app, KeyCode::Char('H'));
        assert!(app.hint.is_some());

        press(&mut app, KeyCode::Right);
        assert!(app.hint.is_none());
        assert!(app.message.is_empty(), "and takes its message with it");
    }

    #[test]
    fn the_hint_target_stays_marked_while_the_suggested_tail_is_carried() {
        let mut app = App::new(42);
        press(&mut app, KeyCode::Char('H'));
        let suggestion = app.hint.expect("a hint was offered");

        press(&mut app, KeyCode::Char(' '));
        assert!(app.selection.is_some());
        assert_eq!(app.hint, Some(suggestion), "the target is still marked");

        // Walking to the target column must not lose it either.
        press(&mut app, KeyCode::Right);
        assert_eq!(app.hint, Some(suggestion));

        app.drop_on(suggestion.to);
        assert!(app.hint.is_none(), "the move is done, so the hint retires");
        assert_eq!(app.history.len(), 1);
    }

    #[test]
    fn escape_drops_both_the_tail_and_the_hint() {
        let mut app = App::new(42);
        press(&mut app, KeyCode::Char('H'));
        press(&mut app, KeyCode::Char(' '));
        press(&mut app, KeyCode::Esc);

        assert!(app.selection.is_none());
        assert!(app.hint.is_none());
    }

    #[test]
    fn a_hint_marks_the_target_card_on_the_board() {
        let mut app = App::new(42);
        press(&mut app, KeyCode::Char('H'));
        let suggestion = app.hint.unwrap();

        let focus = app.focus();
        let exposed = app.game.column(suggestion.to).len() - 1;
        assert_eq!(focus.hint_target, Some((suggestion.to, exposed)));

        // Two cards are framed in double lines — the cursor's and the
        // target's — and the caret marks which of them you are holding.
        let view = screen(&mut app, 80, 30);
        assert_eq!(view.matches('║').count(), 4, "got:\n{view}");
        assert_eq!(view.matches('▶').count(), 1);
        assert_eq!(columns_drawn(&view), COLUMNS);
    }

    #[test]
    fn a_dead_board_says_so_rather_than_pointing_nowhere() {
        let mut app = App::new(42);
        // Play the deal out greedily, stock and all, until it is stuck.
        fn play_out(app: &mut App) {
            while let Some(&next) = app.game.suggestions().first() {
                app.game.apply_move(next.from, next.index, next.to).unwrap();
            }
        }
        play_out(&mut app);
        app.game.deal_stock();
        play_out(&mut app);

        press(&mut app, KeyCode::Char('H'));
        assert!(app.hint.is_none());
        assert!(
            app.message.contains("No moves left"),
            "got {:?}",
            app.message
        );
        assert_eq!(app.tone, Tone::Warn);
    }

    #[test]
    fn help_covers_the_board() {
        let mut app = App::new(42);
        press(&mut app, KeyCode::Char('?'));
        let view = screen(&mut app, 80, 30);
        println!("{view}");
        assert!(view.contains("Scorpion Solitaire"));
        assert!(view.contains("undo"));
    }
}
