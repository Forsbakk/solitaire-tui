//! Cursor, selection, undo history, and input handling.

use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::Frame;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

use crate::board::{Action, Board, BoardSpec, Fan, Move, MoveError, PileId, PileKind};
use crate::rules::Rules;
use crate::ui::layout::{self, BoardLayout, board_width, min_board_height};
use crate::ui::render::{self, Focus, Tone};

/// Rows reserved at the bottom for the status bar.
const STATUS_H: u16 = 2;

/// Why a session ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Exit {
    Quit,
    /// Back to the game selector.
    Menu,
}

/// One game in progress: its rules, its board, and everything the player has
/// done to it.
pub struct Session {
    rules: Box<dyn Rules>,
    board: Board,
    spec: BoardSpec,
    /// The fanned piles by column, each listed top to bottom — what the digit
    /// keys jump to. Most games have one fanned pile per column; Mod3 stacks
    /// four, and a digit names the column rather than any one of them.
    columns: Vec<Vec<PileId>>,
    cursor_pile: PileId,
    cursor_card: Option<usize>,
    /// The held tail, as `(pile, first card)`.
    selection: Option<(PileId, usize)>,
    /// The move the last `H` press suggested, still shown on the board.
    hint: Option<Move>,
    history: Vec<Action>,
    future: Vec<Action>,
    message: String,
    tone: Tone,
    show_help: bool,
    /// The layout the last frame was drawn from, used to resolve clicks.
    layout: Option<BoardLayout>,
}

impl Session {
    pub fn new(rules: Box<dyn Rules>, seed: u64) -> Self {
        let board = rules.deal(seed);
        let spec = rules.spec(&board);
        let columns = columns(&spec);
        let mut session = Self {
            cursor_pile: first_pile(&columns),
            rules,
            board,
            spec,
            columns,
            cursor_card: None,
            selection: None,
            hint: None,
            history: Vec::new(),
            future: Vec::new(),
            message: String::new(),
            tone: Tone::Warn,
            show_help: false,
            layout: None,
        };
        session.snap_cursor();
        session
    }

    fn focus(&self) -> Focus {
        Focus {
            pile: self.cursor_pile,
            card: self.cursor_card,
            selection: self.selection,
            // A hint points at the card the tail would land on, which is
            // whatever is exposed on the target pile.
            hint_target: self
                .hint
                .map(|mv| (mv.to, self.board.pile(mv.to).len().saturating_sub(1))),
        }
    }

    // -- drawing ----------------------------------------------------------

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let need_w = board_width(&self.spec);
        let need_h = min_board_height(&self.spec) + STATUS_H;
        if area.width < need_w || area.height < need_h {
            self.layout = None;
            render::draw_too_small(frame.buffer_mut(), area, need_w, need_h);
            return;
        }

        // The top row carries the pile numbers when there is height to spare,
        // and is given back to the board when there is not.
        let margin = u16::from(area.height > need_h + 1);
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
        let lengths: Vec<usize> = self.board.piles().iter().map(|pile| pile.len()).collect();
        self.layout = layout::compute(
            &self.spec,
            &lengths,
            board,
            &focus.pins(self.board.piles().len()),
        );

        if let Some(board_layout) = &self.layout {
            render::draw_board(
                frame.buffer_mut(),
                board_layout,
                &self.board,
                self.rules.as_ref(),
                &focus,
            );
            // Draws itself into the row above the tableau, or not at all when
            // there is no such row to spare.
            render::draw_labels(
                frame.buffer_mut(),
                board_layout,
                &self.columns,
                self.cursor_pile,
            );
        }
        render::draw_status(
            frame.buffer_mut(),
            status,
            &self.board,
            self.rules.as_ref(),
            &focus,
            &self.message,
            self.tone,
        );

        if self.show_help {
            render::draw_help(frame.buffer_mut(), area, self.rules.as_ref());
        } else if self.rules.is_won(&self.board) {
            render::draw_win(frame.buffer_mut(), area, &self.board, self.rules.as_ref());
        }
    }

    // -- input ------------------------------------------------------------

    /// Handle `event`. `Some` when the player asked to leave the game.
    pub fn handle(&mut self, event: Event) -> Option<Exit> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => return self.on_key(key),
            Event::Mouse(mouse) => self.on_mouse(mouse),
            _ => {}
        }
        None
    }

    fn on_key(&mut self, key: KeyEvent) -> Option<Exit> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && matches!(key.code, KeyCode::Char('c')) {
            return Some(Exit::Quit);
        }

        // Any key dismisses help, so it never traps the player.
        if self.show_help {
            self.show_help = false;
            return None;
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
            KeyCode::Char('q') => return Some(Exit::Quit),
            KeyCode::Char('m') => return Some(Exit::Menu),
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('H') => self.suggest(),
            KeyCode::Char('r') if ctrl => self.redo(),
            KeyCode::Char('u') => self.undo(),
            KeyCode::Char('n') => self.restart(random_seed()),
            KeyCode::Char('r') => self.restart(self.board.seed()),
            KeyCode::Char('d') => self.deal_stock(),
            KeyCode::Char('f') => self.play(),
            KeyCode::Char('A') => self.finish(),
            KeyCode::Esc => {
                self.selection = None;
                self.clear_hint();
                self.message.clear();
            }
            KeyCode::Left | KeyCode::Char('h') => self.step_pile(-1),
            KeyCode::Right | KeyCode::Char('l') => self.step_pile(1),
            KeyCode::Up | KeyCode::Char('k') => self.step_card(-1),
            KeyCode::Down | KeyCode::Char('j') => self.step_card(1),
            KeyCode::Char(' ') | KeyCode::Enter => self.activate(),
            // `0` is the tenth column, so a Spider board's shortcuts run 1…9
            // then 0 — which is how `render::draw_labels` numbers them too.
            KeyCode::Char(digit @ ('1'..='9' | '0')) => {
                let index = if digit == '0' {
                    9
                } else {
                    digit as usize - '1' as usize
                };
                self.jump_to_column(index);
            }
            _ => {}
        }
        None
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
                    self.drop_on(hit.pile);
                    return;
                }
                self.cursor_pile = hit.pile;
                // Clicking a pile that does something — the stock — does it,
                // rather than trying to pick a card off it.
                if let Some(action) = self.rules.activate(&self.board, hit.pile) {
                    self.perform(action);
                    return;
                }
                match hit.card {
                    Some(index) if self.board.pile(hit.pile).is_face_up(index) => {
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

    /// Put the cursor on the current pile's exposed card.
    fn snap_cursor(&mut self) {
        self.cursor_card = self.board.pile(self.cursor_pile).len().checked_sub(1);
    }

    /// Keep the cursor on a real face-up card after the board changes.
    fn clamp_cursor(&mut self) {
        let pile = self.board.pile(self.cursor_pile);
        let Some(last) = pile.len().checked_sub(1) else {
            self.cursor_card = None;
            return;
        };
        // A pile with nothing face up is still somewhere the cursor can be: the
        // stock has an exposed card, even though it is face down and cannot be
        // picked up. Blanking the cursor here would leave the stock looking
        // unfocused the moment it was drawn from.
        let first = pile.first_face_up().unwrap_or(last);
        self.cursor_card = Some(self.cursor_card.unwrap_or(last).clamp(first, last));
    }

    fn place(&self, pile: PileId) -> (u8, u16) {
        self.spec
            .spot(pile)
            .map_or((0, 0), |spot| (spot.row, spot.slot))
    }

    /// A digit key: drop the held tail on whichever pile of the column takes
    /// it, or move the cursor there — staying in its own row when the column
    /// has a pile in it.
    fn jump_to_column(&mut self, index: usize) {
        let Some(column) = self.columns.get(index) else {
            return;
        };
        let (row, _) = self.place(self.cursor_pile);
        let in_row = column.iter().copied().find(|&p| self.place(p).0 == row);
        let fallback = in_row.or(column.last().copied());
        match self.selection {
            Some((from, at)) => {
                let takes = column.iter().copied().find(|&to| {
                    self.rules
                        .check(&self.board, Move::new(from, at, to))
                        .is_ok()
                });
                // When nothing in the column takes it, aim at the obvious pile
                // so the refusal explains itself.
                if let Some(target) = takes.or(fallback) {
                    self.drop_on(target);
                }
            }
            None => {
                if let Some(pile) = fallback {
                    self.cursor_pile = pile;
                    self.snap_cursor();
                }
            }
        }
    }

    /// Walk to the next pile along the cursor's own row, wrapping.
    fn step_pile(&mut self, delta: isize) {
        let (row, _) = self.place(self.cursor_pile);
        let piles = row_order(&self.spec, row);
        let Some(at) = piles.iter().position(|&p| p == self.cursor_pile) else {
            return;
        };
        let next = (at as isize + delta).rem_euclid(piles.len() as isize) as usize;
        self.cursor_pile = piles[next];
        self.snap_cursor();
    }

    /// Move within a fanned pile; at either end, step to the next row.
    fn step_card(&mut self, delta: isize) {
        let pile = self.board.pile(self.cursor_pile);
        let fanned = self
            .spec
            .spot(self.cursor_pile)
            .is_some_and(|spot| spot.fan == Fan::Down);

        if fanned && let (Some(first), Some(current)) = (pile.first_face_up(), self.cursor_card) {
            let last = pile.len() - 1;
            let target = current as isize + delta;
            if target >= first as isize && target <= last as isize {
                self.cursor_card = Some(target as usize);
                return;
            }
        }
        self.step_row(delta);
    }

    /// Move to the row above or below, landing on the nearest slot.
    fn step_row(&mut self, delta: isize) {
        let rows = self.spec.rows() as isize;
        if rows < 2 {
            return;
        }
        let (row, slot) = self.place(self.cursor_pile);
        let next = (row as isize + delta).rem_euclid(rows) as u8;
        let piles = row_order(&self.spec, next);
        let Some(&closest) = piles.iter().min_by_key(|&&pile| {
            let (_, other) = self.place(pile);
            other.abs_diff(slot)
        }) else {
            return;
        };
        self.cursor_pile = closest;
        self.snap_cursor();
    }

    // -- moves ------------------------------------------------------------

    fn activate(&mut self) {
        if self.selection.is_some() {
            self.drop_on(self.cursor_pile);
            return;
        }
        if let Some(action) = self.rules.activate(&self.board, self.cursor_pile) {
            self.perform(action);
            return;
        }
        match self.cursor_card {
            Some(index) if self.board.pile(self.cursor_pile).is_face_up(index) => {
                self.select(index)
            }
            Some(_) => self.warn(MoveError::FaceDown.message()),
            None => self.warn("That pile is empty."),
        }
    }

    fn select(&mut self, index: usize) {
        self.selection = Some((self.cursor_pile, index));
        self.message.clear();
    }

    fn drop_on(&mut self, target: PileId) {
        let Some((from, index)) = self.selection else {
            return;
        };
        let mv = Move::new(from, index, target);
        match self.rules.check(&self.board, mv) {
            Ok(()) => {
                let action = self.rules.action_for(&self.board, mv);
                self.selection = None;
                // Whatever was suggested has either just happened or gone
                // stale, so stop pointing at it.
                self.hint = None;
                self.perform(action);
                self.cursor_pile = target;
                self.snap_cursor();
            }
            Err(error) => self.warn(error.message()),
        }
    }

    /// Apply `action`, record it, and report if the board has gone dead.
    fn perform(&mut self, action: Action) {
        self.board.apply(&action);
        self.history.push(action);
        self.future.clear();
        self.clamp_cursor();
        self.message.clear();
        self.report_position();
    }

    fn deal_stock(&mut self) {
        let stock = self.board.first_of_kind(PileKind::Stock);
        match stock.and_then(|pile| self.rules.activate(&self.board, pile)) {
            Some(action) => self.perform(action),
            // The game words its own refusal: an empty stock and a stock that
            // will not deal yet are different complaints.
            None if stock.is_some() => {
                let refusal = self.rules.stock_refusal(&self.board);
                self.warn(refusal);
            }
            None => self.warn("This game has no stock."),
        }
    }

    /// Play a card where it has no choice but to go, without naming the pile.
    ///
    /// Acts on the card in hand when one is held, otherwise on the card under
    /// the cursor. The foundation comes first — at most one can take any given
    /// card, so there is nothing to choose between — and then the tableau, where
    /// [`Rules::only_move`] answers only when a single column would take it.
    ///
    /// A held run travels whole or not at all. The foundation branch is skipped
    /// for one, since sending its head home alone would break it up.
    fn play(&mut self) {
        let (from, index) = match self
            .selection
            .or_else(|| self.cursor_card.map(|index| (self.cursor_pile, index)))
        {
            Some(head) => head,
            None => {
                self.warn("That pile is empty.");
                return;
            }
        };

        // The foundation only ever takes the exposed card, which is also the
        // only case where `send_home` and the card under the cursor agree.
        if self.board.pile(from).len() - index == 1
            && let Some(action) = self.rules.send_home(&self.board, from)
        {
            self.selection = None;
            self.hint = None;
            self.perform(action);
            return;
        }

        match self.rules.only_move(&self.board, from, index) {
            Some(mv) => {
                let action = self.rules.action_for(&self.board, mv);
                self.selection = None;
                self.hint = None;
                self.perform(action);
                self.cursor_pile = mv.to;
                self.snap_cursor();
            }
            None => self.warn(self.refusal(from, index)),
        }
    }

    /// Why [`Session::play`] would not move this card. Having a move on offer
    /// while `only_move` declined means the columns disagreed, which is a
    /// different problem from having nowhere to go at all.
    fn refusal(&self, from: PileId, index: usize) -> &'static str {
        if !self.board.pile(from).is_face_up(index) {
            return MoveError::FaceDown.message();
        }
        let ambiguous = self
            .rules
            .suggestions(&self.board)
            .iter()
            .any(|mv| mv.from == from && mv.index == index);
        if ambiguous {
            "More than one column takes that card — choose one."
        } else {
            "That card has nowhere to go yet."
        }
    }

    /// Play the game out from a position that is already decided. Each step is
    /// pushed separately, so undo walks back through the finish.
    fn finish(&mut self) {
        if self.rules.autoplay(&self.board).is_none() {
            self.warn("There is still a game to play here.");
            return;
        }
        while let Some(action) = self.rules.autoplay(&self.board) {
            self.board.apply(&action);
            self.history.push(action);
        }
        self.future.clear();
        self.selection = None;
        self.clamp_cursor();
        self.message.clear();
    }

    fn undo(&mut self) {
        match self.history.pop() {
            Some(action) => {
                self.board.undo(&action);
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
                self.board.redo(&action);
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
        self.board = self.rules.deal(seed);
        self.spec = self.rules.spec(&self.board);
        self.columns = columns(&self.spec);
        self.cursor_pile = first_pile(&self.columns);
        self.selection = None;
        self.hint = None;
        self.history.clear();
        self.future.clear();
        self.message.clear();
        self.show_help = false;
        self.snap_cursor();
    }

    // -- hints ------------------------------------------------------------

    /// Point at a move worth making. Pressing again walks through the rest,
    /// best first, so a rejected suggestion is not a dead end.
    fn suggest(&mut self) {
        let suggestions = self.rules.suggestions(&self.board);
        let Some(first) = suggestions.first().copied() else {
            self.hint = None;
            let stuck = if self.rules.has_progress_move(&self.board) {
                "No moves on the board — try the stock."
            } else {
                "No moves left — u to undo, n for a new deal."
            };
            self.warn(stuck);
            return;
        };

        // Advance past whatever was suggested last time, wrapping around.
        let next = match self.hint {
            Some(current) => suggestions
                .iter()
                .position(|mv| *mv == current)
                .map_or(first, |i| suggestions[(i + 1) % suggestions.len()]),
            None => first,
        };

        // Put the cursor on the card being suggested, so accepting the hint is
        // just space then the target pile.
        self.selection = None;
        self.cursor_pile = next.from;
        self.cursor_card = Some(next.index);
        self.hint = Some(next);

        let card = self.board.pile(next.from).cards()[next.index];
        let position = suggestions.iter().position(|mv| *mv == next).unwrap_or(0) + 1;
        let count = suggestions.len();
        let target = self.board.pile(next.to);
        self.message = match (target.exposed(), target.kind()) {
            (Some(card_below), _) => format!("hint {position}/{count}: {card} onto {card_below}"),
            // An empty foundation is not "an empty pile" in the sense the
            // player means — saying so would read as an empty column.
            (None, PileKind::Foundation) => format!("hint {position}/{count}: {card} home"),
            // Nor is an empty cell: parking a card is not the same suggestion
            // as filling a column with it.
            (None, PileKind::FreeCell) => {
                format!("hint {position}/{count}: {card} to a free cell")
            }
            (None, _) => format!("hint {position}/{count}: {card} to an empty pile"),
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

    /// Say something when the deal is dead or finishable, so the player is not
    /// left hunting.
    fn report_position(&mut self) {
        if self.rules.is_won(&self.board) {
            return;
        }
        if self.rules.autoplay(&self.board).is_some() {
            self.message = "Every card is free — press A to finish.".to_string();
            self.tone = Tone::Hint;
        } else if !self.rules.has_progress_move(&self.board) {
            self.warn("No moves left — u to undo, n for a new deal.");
        }
    }
}

/// The piles drawn with `fan`, in reading order.
fn columns(spec: &BoardSpec) -> Vec<Vec<PileId>> {
    let mut spots: Vec<_> = spec
        .spots
        .iter()
        .filter(|spot| spot.fan == Fan::Down)
        .collect();
    spots.sort_by_key(|spot| (spot.slot, spot.row));
    let mut columns: Vec<Vec<PileId>> = Vec::new();
    let mut slot = None;
    for spot in spots {
        if slot != Some(spot.slot) {
            columns.push(Vec::new());
            slot = Some(spot.slot);
        }
        columns.last_mut().expect("just pushed").push(spot.pile);
    }
    columns
}

/// Where the cursor starts: the top of the first column.
fn first_pile(columns: &[Vec<PileId>]) -> PileId {
    columns
        .first()
        .and_then(|c| c.first())
        .copied()
        .unwrap_or(0)
}

/// The piles in `row`, left to right.
fn row_order(spec: &BoardSpec, row: u8) -> Vec<PileId> {
    let mut spots: Vec<_> = spec.row(row).collect();
    spots.sort_by_key(|spot| spot.slot);
    spots.iter().map(|spot| spot.pile).collect()
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
    use crate::board::{Pile, PileKind};
    use crate::card::{ACE, Card, KING, Suit};
    use crate::rules::scorpion::Scorpion;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyEventState;

    fn scorpion(seed: u64) -> Session {
        Session::new(Box::new(Scorpion), seed)
    }

    /// Render `session` at the given size and return the screen as text.
    fn screen(session: &mut Session, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| session.draw(frame)).unwrap();
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

    /// How many piles drew a box, counted from the board's top border row.
    ///
    /// Counting closing borders instead would over-count: a highlighted card
    /// in the middle of a pile closes itself off from its neighbours.
    fn piles_drawn(view: &str) -> usize {
        view.lines()
            .find(|line| line.contains(['┐', '╗', '┓']))
            .map(|line| line.matches(['┐', '╗', '┓']).count())
            .unwrap_or(0)
    }

    fn press(session: &mut Session, code: KeyCode) {
        session.handle(Event::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
    }

    #[test]
    fn the_cursor_starts_on_an_exposed_card_and_stays_face_up() {
        let mut session = scorpion(3);
        assert_eq!(session.cursor_card, Some(6));

        // Pile 0 has three face-down cards; walking up must stop above them.
        for _ in 0..20 {
            press(&mut session, KeyCode::Up);
        }
        assert_eq!(session.cursor_card, Some(3));

        press(&mut session, KeyCode::Right);
        assert_eq!(session.cursor_pile, 1);
        assert_eq!(
            session.cursor_card,
            Some(6),
            "each pile snaps to its exposed card"
        );
    }

    #[test]
    fn piles_wrap_in_both_directions() {
        let mut session = scorpion(3);
        press(&mut session, KeyCode::Left);
        assert_eq!(session.cursor_pile, 6);
        press(&mut session, KeyCode::Right);
        assert_eq!(session.cursor_pile, 0);
    }

    #[test]
    fn the_cursor_never_reaches_a_pile_that_is_not_drawn() {
        // Scorpion's stock holds cards but has no spot on the board.
        let mut session = scorpion(3);
        for _ in 0..40 {
            press(&mut session, KeyCode::Right);
            press(&mut session, KeyCode::Up);
            assert!(session.spec.spot(session.cursor_pile).is_some());
        }
    }

    #[test]
    fn an_illegal_drop_keeps_the_tail_in_hand() {
        let mut session = scorpion(3);
        press(&mut session, KeyCode::Char(' '));
        let held = session.selection.expect("picked up a card");

        // Pile 1's exposed card almost certainly does not accept it.
        press(&mut session, KeyCode::Char('2'));
        if session.selection.is_some() {
            assert_eq!(session.selection, Some(held), "still holding the same tail");
            assert!(!session.message.is_empty(), "and the player is told why");
        }
    }

    #[test]
    fn escape_puts_a_held_tail_back() {
        let mut session = scorpion(3);
        press(&mut session, KeyCode::Char(' '));
        assert!(session.selection.is_some());
        press(&mut session, KeyCode::Esc);
        assert!(session.selection.is_none());
    }

    #[test]
    fn undo_and_redo_walk_the_history() {
        let mut session = scorpion(3);
        let start = session.board.clone();

        press(&mut session, KeyCode::Char('d'));
        assert_eq!(session.history.len(), 1);
        assert_ne!(session.board, start);

        press(&mut session, KeyCode::Char('u'));
        assert_eq!(session.board, start);
        assert!(session.history.is_empty());

        session.redo();
        assert_eq!(session.history.len(), 1);
        assert_ne!(session.board, start);
    }

    #[test]
    fn a_new_move_discards_the_redo_future() {
        let mut session = scorpion(3);
        press(&mut session, KeyCode::Char('d'));
        press(&mut session, KeyCode::Char('u'));
        assert_eq!(session.future.len(), 1);

        press(&mut session, KeyCode::Char('d'));
        assert!(session.future.is_empty());
    }

    #[test]
    fn replay_restores_the_same_deal_and_new_changes_it() {
        let mut session = scorpion(3);
        let deal = session.board.clone();

        press(&mut session, KeyCode::Char('d'));
        press(&mut session, KeyCode::Char('r'));
        assert_eq!(session.board, deal, "r replays the same seed");
        assert!(session.history.is_empty());

        press(&mut session, KeyCode::Char('n'));
        assert_ne!(session.board.seed(), deal.seed());
    }

    fn key(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn only_the_leaving_keys_leave() {
        let mut session = scorpion(3);
        assert_eq!(
            session.handle(key(KeyCode::Char('j'), KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            session.handle(key(KeyCode::Char('m'), KeyModifiers::NONE)),
            Some(Exit::Menu)
        );
        assert_eq!(
            session.handle(key(KeyCode::Char('q'), KeyModifiers::NONE)),
            Some(Exit::Quit)
        );
        assert_eq!(
            session.handle(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Exit::Quit)
        );
    }

    #[test]
    fn help_swallows_the_key_that_closes_it_rather_than_quitting() {
        let mut session = scorpion(3);
        press(&mut session, KeyCode::Char('?'));
        assert_eq!(
            session.handle(key(KeyCode::Char('q'), KeyModifiers::NONE)),
            None,
            "dismissing help must not also quit"
        );
        assert!(!session.show_help);
    }

    #[test]
    fn help_opens_and_the_next_key_closes_it() {
        let mut session = scorpion(3);
        press(&mut session, KeyCode::Char('?'));
        assert!(session.show_help);
        press(&mut session, KeyCode::Char('j'));
        assert!(!session.show_help);
    }

    #[test]
    fn undoing_past_the_start_is_reported_not_ignored() {
        let mut session = scorpion(3);
        press(&mut session, KeyCode::Char('u'));
        assert!(session.message.contains("Nothing to undo"));
    }

    #[test]
    fn the_board_renders_at_a_comfortable_size() {
        let mut session = scorpion(42);
        let view = screen(&mut session, 80, 30);
        println!("{view}");

        assert!(view.contains("┌────┐"), "cards are drawn as boxes");
        assert!(view.contains("▞▞▞▞"), "face-down cards are hatched");
        assert!(view.contains("seed 42"), "the status bar names the deal");
        assert_eq!(piles_drawn(&view), 7, "every pile is drawn");
    }

    #[test]
    fn a_short_terminal_still_shows_every_pile() {
        let mut session = scorpion(42);
        let view = screen(&mut session, 80, 14);
        println!("{view}");

        assert!(view.contains("seed 42"), "the status bar survives");
        assert_eq!(piles_drawn(&view), 7);
    }

    #[test]
    fn piles_that_cannot_fit_are_elided_but_keep_their_exposed_card() {
        let mut session = scorpion(42);
        let view = screen(&mut session, 80, 9);
        println!("{view}");

        assert!(view.contains("⋯"), "the hidden cards are counted");
        assert_eq!(piles_drawn(&view), 7);
        // Pile 7's exposed card is Q♦ in this deal, and it must stay visible
        // because it is the only card that can be dropped on.
        assert!(view.contains("Q♦"), "the exposed card survives elision");
    }

    /// The rows a pile occupies on screen, as plain text.
    fn pile_strip(view: &str, pile: usize) -> Vec<String> {
        let x = view
            .lines()
            .find(|line| line.contains(['┐', '╗', '┓']))
            .map(|line| line.chars().take_while(|c| *c != '┌').count())
            .unwrap_or(0)
            + pile * 7;
        view.lines()
            .map(|line| line.chars().skip(x).take(6).collect::<String>())
            .filter(|cell| cell.contains(['│', '║', '┃', '─', '━', '═']))
            .collect()
    }

    #[test]
    fn a_highlighted_card_is_boxed_in_even_mid_pile() {
        let mut session = scorpion(42);
        // Pile 5 is face up throughout; sit on its middle card.
        press(&mut session, KeyCode::Char('5'));
        for _ in 0..3 {
            press(&mut session, KeyCode::Up);
        }

        let view = screen(&mut session, 80, 30);
        let strip = pile_strip(&view, 4);
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
        let mut session = scorpion(42);
        let view = screen(&mut session, 30, 10);
        assert!(view.contains("too small"), "got:\n{view}");
    }

    #[test]
    fn holding_a_tail_marks_every_card_being_carried() {
        let mut session = scorpion(42);
        press(&mut session, KeyCode::Char('5'));
        press(&mut session, KeyCode::Up);
        press(&mut session, KeyCode::Up);
        press(&mut session, KeyCode::Char(' '));

        let view = screen(&mut session, 80, 30);
        println!("{view}");
        assert!(view.contains("holding 3 cards"), "got:\n{view}");
        // Three carried cards, each with a heavy side on the left and right.
        assert_eq!(view.matches('┃').count(), 6, "got:\n{view}");
        assert_eq!(view.matches("┗━━━━┛").count(), 1, "the tail closes once");
    }

    fn click(session: &mut Session, x: u16, y: u16) {
        session.handle(Event::Mouse(MouseEvent {
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
        let mut session = scorpion(42);
        // Locate a card by where it actually appears, not by asking the layout
        // — that is what makes this a check of render/hit-test agreement.
        let view = screen(&mut session, 80, 30);
        let (x, y) = find(&view, "K♦");

        click(&mut session, x, y);
        let (pile, index) = session
            .selection
            .expect("clicking a face-up card picks it up");
        assert_eq!(session.board.pile(pile).cards()[index].to_string(), "K♦");
    }

    #[test]
    fn clicking_a_face_down_card_explains_itself_and_holds_nothing() {
        let mut session = scorpion(42);
        let view = screen(&mut session, 80, 30);
        let (x, y) = find(&view, "▞▞▞▞");

        click(&mut session, x, y);
        assert!(session.selection.is_none());
        assert_eq!(session.message, MoveError::FaceDown.message());
    }

    #[test]
    fn a_second_click_drops_onto_the_pile_clicked() {
        let mut session = scorpion(42);
        let view = screen(&mut session, 80, 30);

        // In this deal 9♣ sits face up with 10♣ exposed on another pile, so
        // the 9♣ and everything below it may move across.
        let (x, y) = find(&view, "9♣");
        click(&mut session, x, y);
        assert!(session.selection.is_some());

        let (target_x, target_y) = find(&view, "10♣");
        click(&mut session, target_x, target_y);
        assert!(session.selection.is_none(), "the tail was released");
        assert_eq!(session.history.len(), 1, "and the move was recorded");
    }

    #[test]
    fn the_keyboard_can_play_a_move_end_to_end() {
        let mut session = scorpion(42);

        // Pile 6, up to the 9♣, pick it up, drop it on pile 4's 10♣.
        press(&mut session, KeyCode::Char('6'));
        for _ in 0..3 {
            press(&mut session, KeyCode::Up);
        }
        assert_eq!(
            session.board.pile(5).cards()[session.cursor_card.unwrap()].to_string(),
            "9♣"
        );

        press(&mut session, KeyCode::Char(' '));
        press(&mut session, KeyCode::Char('4'));

        assert_eq!(
            session.history.len(),
            1,
            "the move went through: {}",
            session.message
        );
        assert_eq!(session.board.pile(5).len(), 3, "the tail left pile 6");
        assert_eq!(session.board.pile(3).exposed().unwrap().to_string(), "6♠");
        assert_eq!(session.cursor_pile, 3, "the cursor follows the cards");
        assert!(session.selection.is_none());
    }

    #[test]
    fn a_hint_moves_the_cursor_onto_a_move_that_can_be_played() {
        let mut session = scorpion(42);
        press(&mut session, KeyCode::Char('H'));

        let suggestion = session.hint.expect("a hint was offered");
        assert_eq!(session.cursor_pile, suggestion.from);
        assert_eq!(session.cursor_card, Some(suggestion.index));
        assert_eq!(session.tone, Tone::Hint);
        assert!(
            session.message.starts_with("hint 1/"),
            "got {:?}",
            session.message
        );

        // Accepting the hint is just: pick up, then name the target pile.
        press(&mut session, KeyCode::Char(' '));
        session.drop_on(suggestion.to);
        assert_eq!(session.history.len(), 1, "{}", session.message);
    }

    #[test]
    fn a_hint_onto_an_empty_foundation_says_home_not_empty_pile() {
        // Yukon's opening deal offers an Ace to a bare foundation, and calling
        // that "an empty pile" reads as an empty column — the one place in the
        // game an Ace may not go.
        let mut session = Session::new(Box::new(crate::rules::yukon::Yukon), 42);
        press(&mut session, KeyCode::Char('H'));

        let target = session.board.pile(session.hint.expect("a hint").to);
        assert_eq!(target.kind(), PileKind::Foundation);
        assert!(target.is_empty());
        assert!(
            session.message.ends_with(" home"),
            "got {:?}",
            session.message
        );
    }

    #[test]
    fn a_hint_onto_an_empty_free_cell_says_free_cell() {
        // An empty cell is not "an empty pile" either: parking a card there is
        // a different suggestion from dropping it into an empty column, and
        // this board offers both.
        let mut session = Session::new(Box::new(crate::rules::freecell::FreeCell), 0);
        session.board = crate::rules::freecell::tests_support::a_card_worth_parking();
        press(&mut session, KeyCode::Char('H'));

        let target = session.board.pile(session.hint.expect("a hint").to);
        assert_eq!(target.kind(), PileKind::FreeCell);
        assert!(target.is_empty());
        assert!(
            session.message.ends_with(" to a free cell"),
            "got {:?}",
            session.message
        );
    }

    #[test]
    fn pressing_hint_again_walks_through_the_alternatives() {
        let mut session = scorpion(42);
        let total = session.rules.suggestions(&session.board).len();
        assert!(total > 1, "this deal offers several moves");

        let mut seen = Vec::new();
        for _ in 0..total {
            press(&mut session, KeyCode::Char('H'));
            seen.push(session.hint.expect("still hinting"));
        }
        assert_eq!(
            seen.iter().collect::<std::collections::HashSet<_>>().len(),
            total,
            "every suggestion is offered once before repeating"
        );

        press(&mut session, KeyCode::Char('H'));
        assert_eq!(session.hint, Some(seen[0]), "then it wraps around");
    }

    #[test]
    fn the_hint_highlight_clears_as_soon_as_the_player_does_anything_else() {
        let mut session = scorpion(42);
        press(&mut session, KeyCode::Char('H'));
        assert!(session.hint.is_some());

        press(&mut session, KeyCode::Right);
        assert!(session.hint.is_none());
        assert!(session.message.is_empty(), "and takes its message with it");
    }

    #[test]
    fn the_hint_target_stays_marked_while_the_suggested_tail_is_carried() {
        let mut session = scorpion(42);
        press(&mut session, KeyCode::Char('H'));
        let suggestion = session.hint.expect("a hint was offered");

        press(&mut session, KeyCode::Char(' '));
        assert!(session.selection.is_some());
        assert_eq!(session.hint, Some(suggestion), "the target is still marked");

        // Walking to the target pile must not lose it either.
        press(&mut session, KeyCode::Right);
        assert_eq!(session.hint, Some(suggestion));

        session.drop_on(suggestion.to);
        assert!(
            session.hint.is_none(),
            "the move is done, so the hint retires"
        );
        assert_eq!(session.history.len(), 1);
    }

    #[test]
    fn escape_drops_both_the_tail_and_the_hint() {
        let mut session = scorpion(42);
        press(&mut session, KeyCode::Char('H'));
        press(&mut session, KeyCode::Char(' '));
        press(&mut session, KeyCode::Esc);

        assert!(session.selection.is_none());
        assert!(session.hint.is_none());
    }

    #[test]
    fn a_hint_marks_the_target_card_on_the_board() {
        let mut session = scorpion(42);
        press(&mut session, KeyCode::Char('H'));
        let suggestion = session.hint.unwrap();

        let focus = session.focus();
        let exposed = session.board.pile(suggestion.to).len() - 1;
        assert_eq!(focus.hint_target, Some((suggestion.to, exposed)));

        // Two cards are framed in double lines — the cursor's and the
        // target's — and the caret marks which of them you are holding.
        let view = screen(&mut session, 80, 30);
        assert_eq!(view.matches('║').count(), 4, "got:\n{view}");
        assert_eq!(view.matches('▶').count(), 1);
        assert_eq!(piles_drawn(&view), 7);
    }

    #[test]
    fn a_dead_board_says_so_rather_than_pointing_nowhere() {
        let mut session = scorpion(42);
        // Play the deal out greedily, stock and all, until it is stuck.
        fn play_out(session: &mut Session) {
            while let Some(&next) = session.rules.suggestions(&session.board).first() {
                let action = session.rules.action_for(&session.board, next);
                session.board.apply(&action);
            }
        }
        play_out(&mut session);
        if let Some(action) = session
            .board
            .first_of_kind(PileKind::Stock)
            .and_then(|pile| session.rules.activate(&session.board, pile))
        {
            session.board.apply(&action);
        }
        play_out(&mut session);

        press(&mut session, KeyCode::Char('H'));
        assert!(session.hint.is_none());
        assert!(
            session.message.contains("No moves left"),
            "got {:?}",
            session.message
        );
        assert_eq!(session.tone, Tone::Warn);
    }

    #[test]
    fn help_covers_the_board() {
        let mut session = scorpion(42);
        press(&mut session, KeyCode::Char('?'));
        let view = screen(&mut session, 80, 34);
        println!("{view}");
        assert!(view.contains("Scorpion"));
        assert!(view.contains("undo"));
    }

    #[test]
    fn the_help_popup_is_tall_enough_for_everything_it_lists() {
        // Its height comes from the *wrapped* text, not the raw line count —
        // the blurb wraps, and counting raw lines clips the last bindings off
        // the bottom. Measured against the drawn borders, since single-letter
        // keys like `q` also appear in the status bar and prove nothing.
        for (mut session, name) in [(scorpion(42), "Scorpion"), (klondike(42), "Klondike")] {
            let text = render::help_text(session.rules.as_ref());
            let needed = render::wrapped_height(&text, render::HELP_INNER_W as u16) as usize;

            press(&mut session, KeyCode::Char('?'));
            let view = screen(&mut session, 100, 40);
            let borders: Vec<usize> = view
                .lines()
                .enumerate()
                .filter(|(_, line)| line.contains('╭') || line.contains('╰'))
                .map(|(row, _)| row)
                .collect();

            assert_eq!(borders.len(), 2, "{name}: popup is not closed:\n{view}");
            let inside = borders[1] - borders[0] - 1;
            assert!(
                inside >= needed,
                "{name}: popup holds {inside} rows but the help needs {needed}:\n{view}"
            );
            assert!(
                view.contains("close this help"),
                "{name}: the last bindings were clipped:\n{view}"
            );
        }
    }

    // -- a second game, on the same machinery ----------------------------

    fn klondike(seed: u64) -> Session {
        Session::new(Box::new(crate::rules::klondike::Klondike::default()), seed)
    }

    #[test]
    fn a_two_row_board_draws_both_rows() {
        let mut session = klondike(42);
        let view = screen(&mut session, 80, 30);
        println!("{view}");

        assert!(view.contains("▞▞▞▞"), "the stock is face down");
        assert!(view.contains("seed 42"));
        assert!(view.contains("home 0/52"), "the game's own status shows");
        // Seven tableau piles below, and the stock, waste and four
        // foundations above.
        assert_eq!(piles_drawn(&view), 6, "stock, waste and four foundations");
    }

    #[test]
    fn the_cursor_walks_between_the_rows() {
        let mut session = klondike(42);
        let tableau: Vec<PileId> = session.columns.concat();
        assert_eq!(session.cursor_pile, tableau[0], "starts on the tableau");

        press(&mut session, KeyCode::Up);
        assert!(
            !tableau.contains(&session.cursor_pile),
            "up leaves the tableau for the row above"
        );
        press(&mut session, KeyCode::Down);
        assert!(tableau.contains(&session.cursor_pile), "and comes back");
    }

    #[test]
    fn turning_the_stock_puts_a_card_on_the_waste() {
        let mut session = klondike(42);
        press(&mut session, KeyCode::Char('d'));

        let view = screen(&mut session, 80, 30);
        println!("{view}");
        assert_eq!(session.history.len(), 1);

        press(&mut session, KeyCode::Char('u'));
        assert_eq!(session.board, klondike(42).board, "and undo puts it back");
    }

    #[test]
    fn clicking_the_stock_turns_it_rather_than_picking_it_up() {
        let mut session = klondike(42);
        let view = screen(&mut session, 80, 30);
        let (x, y) = find(&view, "▞▞▞▞");

        click(&mut session, x, y);
        assert!(session.selection.is_none(), "nothing was picked up");
        assert_eq!(session.history.len(), 1, "the stock was turned");
    }

    #[test]
    fn f_sends_a_card_home_and_says_so_when_it_cannot() {
        let mut session = klondike(42);
        // Pile 0 is a lone 7♠ in this deal, which has nowhere to go yet.
        press(&mut session, KeyCode::Char('1'));
        press(&mut session, KeyCode::Char('f'));
        assert!(session.history.is_empty());
        assert!(!session.message.is_empty(), "and it explains why");

        // Deal an ace onto a pile and it goes home at once.
        let mut session = klondike(42);
        let ace = (0..7).find(|&pile| {
            session
                .board
                .pile(pile)
                .exposed()
                .is_some_and(|c| c.rank == 1)
        });
        if let Some(pile) = ace {
            session.cursor_pile = pile;
            session.snap_cursor();
            press(&mut session, KeyCode::Char('f'));
            assert_eq!(session.history.len(), 1, "{}", session.message);
        }
    }

    #[test]
    fn autoplay_finishes_a_decided_board_and_undo_walks_back_through_it() {
        let mut session = klondike(1);
        // Force a decided position: every suit stacked King-down on a pile,
        // nothing hidden, no stock.
        session.board = crate::rules::klondike::tests_support::decided_board();
        session.clamp_cursor();

        press(&mut session, KeyCode::Char('A'));
        assert!(session.rules.is_won(&session.board), "the finish ran");
        assert_eq!(session.history.len(), 52, "one step per card");

        for _ in 0..52 {
            press(&mut session, KeyCode::Char('u'));
        }
        assert!(session.history.is_empty());
        assert!(!session.rules.is_won(&session.board), "all the way back");
    }

    /// Klondike's waste, as `(pile id, drawn cards)`.
    fn waste(session: &Session) -> PileId {
        session
            .board
            .first_of_kind(PileKind::Waste)
            .expect("Klondike has a waste")
    }

    #[test]
    fn a_draw_of_three_shows_all_three_but_plays_only_the_top() {
        let mut session = klondike(42);
        press(&mut session, KeyCode::Char('d'));

        let view = screen(&mut session, 80, 22);
        println!("{view}");
        let turned = session.board.pile(waste(&session)).cards().to_vec();
        assert_eq!(turned.len(), 3);

        // Every card the draw turned up is legible in full — rank and suit —
        // covered ones included, tens included.
        assert!(
            turned.iter().any(|card| card.rank_str() == "10"),
            "this deal's first draw holds a ten, which is the tight case"
        );
        for card in &turned {
            assert!(
                view.contains(&card.to_string()),
                "{card} is not legible:\n{view}"
            );
        }

        // But only the exposed one can be picked up.
        session.cursor_pile = waste(&session);
        session.snap_cursor();
        assert_eq!(session.cursor_card, Some(2));
    }

    #[test]
    fn a_draw_of_one_fans_nothing() {
        let mut game = crate::rules::klondike::Klondike::default();
        crate::rules::Rules::configure(&mut game, 0, 1); // Draw: 1
        let mut session = Session::new(Box::new(game), 42);
        press(&mut session, KeyCode::Char('d'));

        let view = screen(&mut session, 80, 22);
        assert_eq!(session.board.pile(waste(&session)).len(), 1);
        assert_eq!(
            view.lines()
                .find(|line| line.contains('┌'))
                .unwrap()
                .matches("┌────┐")
                .count(),
            6,
            "stock, waste and four foundations, none of them fanned:\n{view}"
        );
    }

    #[test]
    fn the_fanned_waste_stops_before_the_foundations() {
        // Seven slots leave no room for a gap between a three-card fan and the
        // first foundation, so they sit flush — but the fan must never overlap
        // one, which would draw a foundation over the waste's top card.
        let mut session = klondike(42);
        press(&mut session, KeyCode::Char('d'));
        screen(&mut session, 80, 22);

        let layout = session.layout.as_ref().unwrap();
        let fan = layout.pile(waste(&session)).unwrap();
        let foundation = session
            .board
            .first_of_kind(PileKind::Foundation)
            .and_then(|pile| layout.pile(pile))
            .unwrap();
        assert!(
            fan.x + fan.width <= foundation.x,
            "the fan runs into the foundations: {} vs {}",
            fan.x + fan.width,
            foundation.x
        );
    }

    #[test]
    fn clicking_a_covered_waste_card_takes_the_one_on_top() {
        let mut session = klondike(42);
        press(&mut session, KeyCode::Char('d'));
        screen(&mut session, 80, 22);

        // The leftmost, most-covered card of the fan — the one a click must
        // *not* try to pick up, since it is not in play.
        let spread = &session
            .layout
            .as_ref()
            .unwrap()
            .pile(waste(&session))
            .unwrap()
            .spread;
        assert_eq!(spread.len(), 3, "three cards are drawn side by side");
        let (_, x) = spread[0];
        let y = session
            .layout
            .as_ref()
            .unwrap()
            .pile(waste(&session))
            .unwrap()
            .top
            + 1;

        click(&mut session, x + 1, y);
        let (pile, index) = session.selection.expect("the click picked something up");
        assert_eq!(pile, waste(&session));
        assert_eq!(index, 2, "the exposed card, not the one clicked");
    }

    #[test]
    fn the_cursor_stays_findable_once_a_card_is_picked_up() {
        // Selecting used to replace the cursor accent outright, so the caret
        // vanished at the moment the player most needed to know where they were.
        for mut session in [scorpion(42), klondike(42)] {
            let before = screen(&mut session, 80, 30);
            assert_eq!(before.matches('▶').count(), 1, "got:\n{before}");

            press(&mut session, KeyCode::Char(' '));
            assert!(session.selection.is_some());

            let after = screen(&mut session, 80, 30);
            assert_eq!(
                after.matches('▶').count(),
                1,
                "the caret survives selection:\n{after}"
            );
        }
    }

    #[test]
    fn a_selected_waste_card_is_marked_as_held() {
        let mut session = klondike(42);
        press(&mut session, KeyCode::Char('d'));
        session.cursor_pile = waste(&session);
        session.snap_cursor();
        press(&mut session, KeyCode::Char(' '));

        let view = screen(&mut session, 80, 22);
        println!("{view}");
        assert!(view.contains("┏━━━━┓"), "held cards get a heavy frame");
        assert_eq!(view.matches('▶').count(), 1, "and the caret is still there");
        assert!(view.contains("holding 1 card"));
    }

    #[test]
    fn the_stock_stays_marked_after_it_is_drawn_from() {
        // The stock is entirely face down, so it has no face-up card to clamp
        // the cursor onto — which used to blank the cursor on every draw and
        // leave the pile looking unfocused just after it was used.
        let mut session = klondike(42);
        session.cursor_pile = session.board.first_of_kind(PileKind::Stock).unwrap();
        session.snap_cursor();

        let before = screen(&mut session, 80, 24);
        assert_eq!(
            before.matches('╔').count(),
            1,
            "the stock is marked:\n{before}"
        );
        assert_eq!(before.matches('▶').count(), 1);

        for draw in 1..=3 {
            press(&mut session, KeyCode::Char('d'));
            let after = screen(&mut session, 80, 24);
            assert_eq!(
                after.matches('╔').count(),
                1,
                "still marked after draw {draw}:\n{after}"
            );
            assert_eq!(after.matches('▶').count(), 1, "draw {draw}:\n{after}");
            assert!(session.cursor_card.is_some(), "draw {draw}");
        }
    }

    #[test]
    fn an_emptied_stock_still_shows_the_cursor() {
        let mut session = klondike(42);
        session.cursor_pile = session.board.first_of_kind(PileKind::Stock).unwrap();
        session.snap_cursor();
        while !session.board.pile(session.cursor_pile).is_empty() {
            press(&mut session, KeyCode::Char('d'));
        }

        let view = screen(&mut session, 80, 24);
        println!("{view}");
        assert_eq!(session.cursor_card, None, "there is no card to sit on");
        assert_eq!(view.matches('╔').count(), 1, "but the pile is still marked");
        assert!(view.contains('↻'), "and says it can be refilled");
    }

    #[test]
    fn f_sends_the_card_you_are_holding_without_naming_a_foundation() {
        let mut session = klondike(42);
        // Pile 3 opens with A♠ exposed in this deal.
        let ace = (0..7)
            .find(|&pile| {
                session
                    .board
                    .pile(pile)
                    .exposed()
                    .is_some_and(|card| card.rank == 1)
            })
            .expect("this deal exposes an ace");

        session.cursor_pile = ace;
        session.snap_cursor();
        press(&mut session, KeyCode::Char(' '));
        assert!(session.selection.is_some(), "the ace is in hand");

        press(&mut session, KeyCode::Char('f'));
        assert!(session.selection.is_none(), "and it was played");
        assert_eq!(session.history.len(), 1, "{}", session.message);

        let home: usize = session
            .board
            .of_kind(PileKind::Foundation)
            .map(|pile| session.board.pile(pile).len())
            .sum();
        assert_eq!(home, 1, "onto a foundation, without choosing which");
    }

    #[test]
    fn an_ace_goes_to_the_first_free_foundation() {
        let mut session = klondike(42);
        let foundations: Vec<PileId> = session.board.of_kind(PileKind::Foundation).collect();

        // Send the first two aces this deal offers and watch them fill the
        // foundations in order — any empty one will take an ace, so the choice
        // has to be made for the player rather than left to them.
        let mut filled = 0;
        for pile in 0..7 {
            if session
                .board
                .pile(pile)
                .exposed()
                .is_some_and(|card| card.rank == 1)
            {
                session.cursor_pile = pile;
                session.snap_cursor();
                press(&mut session, KeyCode::Char('f'));
                filled += 1;
                assert_eq!(
                    session.board.pile(foundations[filled - 1]).len(),
                    1,
                    "ace {filled} took foundation {}",
                    filled - 1
                );
            }
        }
        assert!(filled > 0, "this deal exposes at least one ace");
        for &pile in &foundations[filled..] {
            assert!(session.board.pile(pile).is_empty(), "the rest stay empty");
        }
    }

    #[test]
    fn f_leaves_a_held_run_alone_when_it_has_nowhere_to_go() {
        // Scorpion seed 42: pile 0's first face-up card is A♣, and the only
        // card that would take it is the 2♣ sitting right above it on the same
        // pile. A run that cannot move must stay in hand, whole.
        let mut session = scorpion(42);
        press(&mut session, KeyCode::Char('1'));
        for _ in 0..3 {
            press(&mut session, KeyCode::Up);
        }
        press(&mut session, KeyCode::Char(' '));
        assert_eq!(session.selection, Some((0, 3)), "holding a four-card tail");

        press(&mut session, KeyCode::Char('f'));
        assert_eq!(session.selection, Some((0, 3)), "still holding it");
        assert_eq!(session.board.pile(0).len(), 7, "and nothing moved");
        assert_eq!(session.message, "That card has nowhere to go yet.");
    }

    #[test]
    fn f_plays_a_card_onto_its_only_column() {
        // Scorpion seed 42: pile 1 holds J♦ under 2♦ and 4♣, and Q♦ is exposed
        // on pile 6 — the one column in the game that takes a J♦.
        let mut session = scorpion(42);
        press(&mut session, KeyCode::Char('2'));
        press(&mut session, KeyCode::Up);
        press(&mut session, KeyCode::Up);
        assert_eq!(
            session.board.pile(1).cards()[session.cursor_card.unwrap()],
            Card::new(11, Suit::Diamonds)
        );

        press(&mut session, KeyCode::Char('f'));
        assert_eq!(session.history.len(), 1, "{}", session.message);
        assert_eq!(session.board.pile(6).len(), 10, "J♦ and its tail landed");
        assert_eq!(
            session.board.pile(6).exposed(),
            Some(Card::new(4, Suit::Clubs))
        );
        assert_eq!(session.board.pile(1).len(), 4);
        assert_eq!(session.cursor_pile, 6, "the cursor follows the card");
    }

    #[test]
    fn f_plays_a_held_run_onto_its_only_column() {
        // Pile 5 is dealt face up, so lifting 9♣ takes K♦ 5♦ 6♠ with it. Only
        // pile 3's exposed 10♣ takes a 9♣.
        let mut session = scorpion(42);
        press(&mut session, KeyCode::Char('6'));
        for _ in 0..3 {
            press(&mut session, KeyCode::Up);
        }
        press(&mut session, KeyCode::Char(' '));
        assert_eq!(session.selection, Some((5, 3)));

        press(&mut session, KeyCode::Char('f'));
        assert!(session.selection.is_none(), "{}", session.message);
        assert_eq!(session.board.pile(3).len(), 11, "all four cards travelled");
        assert_eq!(
            session.board.pile(3).exposed(),
            Some(Card::new(6, Suit::Spades))
        );
        assert_eq!(session.board.pile(5).len(), 3);
    }

    #[test]
    fn a_played_card_comes_back_in_one_undo() {
        let mut session = scorpion(42);
        press(&mut session, KeyCode::Char('2'));
        press(&mut session, KeyCode::Up);
        press(&mut session, KeyCode::Up);
        press(&mut session, KeyCode::Char('f'));
        press(&mut session, KeyCode::Char('u'));
        assert_eq!(session.board, Scorpion.deal(42));
    }

    #[test]
    fn f_says_nothing_takes_the_card_when_nothing_does() {
        // Pile 0 exposes A♦, and the 2♦ that would take it is buried on pile 1.
        let mut session = scorpion(42);
        press(&mut session, KeyCode::Char('1'));
        press(&mut session, KeyCode::Char('f'));
        assert!(session.history.is_empty());
        assert_eq!(session.message, "That card has nowhere to go yet.");
    }

    #[test]
    fn f_refuses_to_choose_between_two_columns() {
        // Spider builds down regardless of suit, so a 9♥ sitting under two
        // exposed tens is a decision the player has to make themselves.
        let mut session = spider(42);
        session.board = spider_board(&[
            Pile::tableau(
                1,
                &[Card::new(KING, Suit::Spades), Card::new(9, Suit::Hearts)],
            ),
            Pile::tableau(0, &[Card::new(10, Suit::Clubs)]),
            Pile::tableau(0, &[Card::new(10, Suit::Diamonds)]),
        ]);
        session.clamp_cursor();

        press(&mut session, KeyCode::Char('1'));
        press(&mut session, KeyCode::Char('f'));
        assert!(session.history.is_empty(), "nothing was guessed at");
        assert_eq!(
            session.message,
            "More than one column takes that card — choose one."
        );
    }

    #[test]
    fn f_prefers_a_real_card_to_an_empty_column() {
        // Empty columns are scarce in Spider, and any card may take one — so
        // the shortcut must not spend one while a card would take the move.
        let mut session = spider(42);
        session.board = spider_board(&[
            Pile::tableau(
                1,
                &[Card::new(KING, Suit::Spades), Card::new(9, Suit::Hearts)],
            ),
            Pile::tableau(0, &[]),
            Pile::tableau(0, &[Card::new(10, Suit::Clubs)]),
        ]);
        session.clamp_cursor();

        press(&mut session, KeyCode::Char('1'));
        press(&mut session, KeyCode::Char('f'));
        assert_eq!(session.board.pile(2).len(), 2, "{}", session.message);
        assert!(session.board.pile(1).is_empty(), "the column is still free");
    }

    #[test]
    fn f_takes_the_leftmost_empty_column_when_that_is_all_there_is() {
        // Several empty columns are one destination, not several: they are
        // interchangeable, so there is nothing for the player to decide.
        let mut session = spider(42);
        session.board = spider_board(&[
            Pile::tableau(
                1,
                &[Card::new(KING, Suit::Spades), Card::new(9, Suit::Hearts)],
            ),
            Pile::tableau(0, &[Card::new(12, Suit::Diamonds)]),
        ]);
        session.clamp_cursor();

        press(&mut session, KeyCode::Char('1'));
        press(&mut session, KeyCode::Char('f'));
        assert_eq!(
            session.board.pile(2).exposed(),
            Some(Card::new(9, Suit::Hearts)),
            "{}",
            session.message
        );
        assert!(session.board.pile(3).is_empty());
    }

    #[test]
    fn a_play_that_finishes_a_spider_run_files_it_in_the_same_move() {
        // The shortcut goes through `action_for`, so Spider still appends the
        // steps that file the run — one move, and one undo.
        let mut session = spider(42);
        let run: Vec<Card> = (0..12).map(|i| Card::new(KING - i, Suit::Spades)).collect();
        session.board = spider_board(&[
            Pile::tableau(0, &run),
            Pile::tableau(0, &[Card::new(ACE, Suit::Spades)]),
        ]);
        session.clamp_cursor();

        press(&mut session, KeyCode::Char('2'));
        press(&mut session, KeyCode::Char('f'));
        assert_eq!(session.history.len(), 1, "{}", session.message);
        assert!(session.board.pile(0).is_empty(), "the run left the tableau");
        assert_eq!(session.board.pile(10).len(), 13, "onto a foundation");

        press(&mut session, KeyCode::Char('u'));
        assert_eq!(
            session.board.pile(0).len(),
            12,
            "and one undo walks it back"
        );
        assert!(session.board.pile(10).is_empty());
    }

    #[test]
    fn f_still_sends_a_card_home_before_looking_at_columns() {
        // A 2♥ would take the A♠, but the foundation is where an ace belongs.
        let mut session = klondike(42);
        let mut piles: Vec<Pile> = session.board.piles().to_vec();
        piles[0] = Pile::tableau(0, &[Card::new(ACE, Suit::Spades)]);
        piles[1] = Pile::tableau(0, &[Card::new(2, Suit::Hearts)]);
        for pile in piles.iter_mut().take(7).skip(2) {
            *pile = Pile::tableau(0, &[]);
        }
        session.board = Board::new(42, piles);
        session.clamp_cursor();

        press(&mut session, KeyCode::Char('1'));
        press(&mut session, KeyCode::Char('f'));
        assert_eq!(session.board.pile(1).len(), 1, "the 2♥ was passed over");
        let home: usize = session
            .board
            .of_kind(PileKind::Foundation)
            .map(|pile| session.board.pile(pile).len())
            .sum();
        assert_eq!(home, 1, "{}", session.message);
    }

    #[test]
    fn the_status_line_offers_the_shortcut_when_it_would_work() {
        let mut session = klondike(42);
        let ace = (0..7)
            .find(|&pile| {
                session
                    .board
                    .pile(pile)
                    .exposed()
                    .is_some_and(|card| card.rank == 1)
            })
            .unwrap();

        session.cursor_pile = ace;
        session.snap_cursor();
        press(&mut session, KeyCode::Char(' '));
        let view = screen(&mut session, 80, 24);
        assert!(view.contains("f to send it home"), "got:\n{view}");

        // A card with nowhere to go says nothing about it.
        let mut session = scorpion(42);
        press(&mut session, KeyCode::Char(' '));
        let view = screen(&mut session, 80, 30);
        assert!(!view.contains("send it home"), "got:\n{view}");
        assert!(!view.contains("f to play it"), "got:\n{view}");

        // A run with exactly one column waiting is offered too, even in a game
        // with no foundations to send anything home to.
        let mut session = scorpion(42);
        press(&mut session, KeyCode::Char('2'));
        press(&mut session, KeyCode::Up);
        press(&mut session, KeyCode::Up);
        press(&mut session, KeyCode::Char(' '));
        let view = screen(&mut session, 80, 30);
        assert!(view.contains("f to play it"), "got:\n{view}");
    }

    #[test]
    fn every_game_survives_every_awkward_terminal_size() {
        // Spider's board is 69 cells wide against Klondike's 48, so the small
        // sizes here are the ones that must fall back rather than panic.
        for game in crate::rules::games() {
            let mut session = Session::new(game, 42);
            for (width, height) in [(80, 30), (80, 14), (69, 12), (60, 12), (48, 10), (30, 8)] {
                screen(&mut session, width, height);
            }
        }
    }

    fn spider(seed: u64) -> Session {
        Session::new(Box::new(crate::rules::spider::Spider::default()), seed)
    }

    /// A ten-column Spider board built column by column, with bare
    /// foundations and an empty stock behind it.
    fn spider_board(columns: &[Pile]) -> Board {
        let mut piles: Vec<Pile> = columns.to_vec();
        piles.resize(10, Pile::tableau(0, &[]));
        piles.resize(18, Pile::new(PileKind::Foundation));
        piles.push(Pile::new(PileKind::Stock));
        Board::new(0, piles)
    }

    #[test]
    fn a_ten_column_board_draws_every_pile_and_numbers_the_tenth_zero() {
        let mut session = spider(42);
        let view = screen(&mut session, 80, 30);
        println!("{view}");

        assert_eq!(
            piles_drawn(&view),
            9,
            "the stock and eight foundations share the top row",
        );
        assert!(view.contains("runs 0/8"), "the game's own status shows");
        assert!(view.contains("deals 5"));

        let labels = view
            .lines()
            .find(|line| line.contains("[1]"))
            .expect("the tableau is numbered");
        assert!(
            labels.ends_with(" 0"),
            "the tenth pile is labelled 0: {labels:?}",
        );
        assert!(!labels.contains("10"), "which is the key, not its ordinal");
    }

    fn mod3(seed: u64) -> Session {
        Session::new(Box::new(crate::rules::mod3::Mod3), seed)
    }

    #[test]
    fn a_column_of_several_fanned_piles_is_numbered_once() {
        let mut session = mod3(42);
        let view = screen(&mut session, 80, 40);
        println!("{view}");

        let labels: Vec<&str> = view
            .lines()
            .find(|line| line.contains("[1]"))
            .expect("the columns are numbered")
            .split_whitespace()
            .collect();
        // Every fanned row used to number itself into this one line, so the
        // working row's 5…2 overwrote the rest.
        assert_eq!(labels, ["[1]", "2", "3", "4", "5", "6", "7", "8"]);
    }

    #[test]
    fn a_digit_drops_on_whichever_pile_of_its_column_takes_the_card() {
        // Mod3's piles: working row 0–7, then the 2-, 3- and 4-rows, talon,
        // aces. Every row spot holds a misplaced 9♣ but column six's 2-row
        // spot, which is bare and so takes a 2.
        let mut piles: Vec<Pile> = (0..8)
            .map(|_| Pile::tableau(0, &[Card::new(10, Suit::Clubs)]))
            .collect();
        piles[0] = Pile::tableau(0, &[Card::new(2, Suit::Hearts)]);
        piles.extend((0..24).map(|i| {
            let cards: &[Card] = if i == 5 {
                &[]
            } else {
                &[Card::new(9, Suit::Clubs)]
            };
            Pile::holding(PileKind::Foundation, 0, cards)
        }));
        piles.push(Pile::new(PileKind::Stock));
        piles.push(Pile::new(PileKind::Foundation));

        let mut session = mod3(42);
        session.board = Board::new(0, piles);
        session.cursor_pile = 0;
        session.snap_cursor();
        press(&mut session, KeyCode::Char(' '));
        assert!(session.selection.is_some(), "2♥ is held");

        press(&mut session, KeyCode::Char('6'));
        assert_eq!(
            session.board.pile(8 + 5).exposed(),
            Some(Card::new(2, Suit::Hearts)),
            "the 2 went to column six's 2-row, not its working pile",
        );
    }

    #[test]
    fn the_zero_key_jumps_to_the_tenth_pile() {
        let mut session = spider(42);
        session.board = spider_board(&[
            Pile::tableau(0, &[Card::new(8, Suit::Spades)]),
            Pile::tableau(0, &[]),
        ]);
        session.board = {
            let mut piles: Vec<Pile> = session.board.piles().to_vec();
            piles[9] = Pile::tableau(0, &[Card::new(9, Suit::Spades)]);
            Board::new(0, piles)
        };
        session.clamp_cursor();

        press(&mut session, KeyCode::Char('0'));
        assert_eq!(session.cursor_pile, 9, "0 is the tenth pile");

        // And it drops there too, which is the half a lone jump would miss.
        press(&mut session, KeyCode::Char('1'));
        press(&mut session, KeyCode::Char(' '));
        press(&mut session, KeyCode::Char('0'));
        assert_eq!(session.board.pile(9).len(), 2, "the 8♠ landed on the 9♠");
        assert!(session.board.pile(0).is_empty());
    }

    #[test]
    fn the_deal_key_says_why_the_stock_refused() {
        // Spider is the first game that can refuse a deal from a stock that is
        // still full, so the shared "nothing left to give" would be a lie.
        let mut session = spider(42);
        let mut piles: Vec<Pile> = session.board.piles().to_vec();
        piles[0] = Pile::tableau(0, &[]);
        session.board = Board::new(42, piles);
        session.clamp_cursor();

        press(&mut session, KeyCode::Char('d'));
        assert_eq!(session.message, "Fill every empty column before dealing.");
        assert_eq!(session.board.pile(1).len(), 6, "and nothing was dealt");

        let spent = scorpion(42);
        assert_eq!(
            spent.rules.stock_refusal(&spent.board),
            "The stock has nothing left to give.",
            "games that only run dry keep the shared wording",
        );
    }

    #[test]
    fn filing_a_run_is_one_undo() {
        let mut session = spider(42);
        let run: Vec<Card> = (2..=KING)
            .rev()
            .map(|rank| Card::new(rank, Suit::Spades))
            .collect();
        session.board = spider_board(&[
            Pile::tableau(0, &run),
            Pile::tableau(0, &[Card::new(ACE, Suit::Spades)]),
        ]);
        let start = session.board.clone();
        session.clamp_cursor();

        press(&mut session, KeyCode::Char('2'));
        press(&mut session, KeyCode::Char(' '));
        press(&mut session, KeyCode::Char('1'));

        assert_eq!(
            session.history.len(),
            1,
            "the run files itself as part of the move"
        );
        assert!(
            session.board.pile(0).is_empty(),
            "thirteen cards left the tableau"
        );
        assert_eq!(
            session.board.pile(10).exposed(),
            Some(Card::new(KING, Suit::Spades)),
            "and turned over onto the first foundation",
        );

        press(&mut session, KeyCode::Char('u'));
        assert_eq!(session.board, start, "one undo takes the whole thing back");
    }
}
