//! The game selector shown at startup.
//!
//! Two panes: the games on the left, and on the right the highlighted game's
//! rules along with the ones the player may change before dealing. `tab` moves
//! between the panes, so the arrow keys mean one thing at a time.

use anyhow::Result;
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Padding, Paragraph, Widget, Wrap};
use ratatui::{DefaultTerminal, Frame};

use crate::rules::{self, Rules};
use crate::ui::render::{centered, wrapped_height};

const CURSOR: Color = Color::Cyan;
const MUTED: Color = Color::DarkGray;

const LIST_W: u16 = 22;
const PREVIEW_W: u16 = 52;
const WIDTH: u16 = LIST_W + PREVIEW_W;
const MIN_W: u16 = 48;
const MIN_H: u16 = 12;

/// Which pane the arrow keys are steering.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pane {
    Games,
    Rules,
}

/// What the player did with the menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Choice {
    Play,
    Quit,
}

pub struct Menu {
    games: Vec<Box<dyn Rules>>,
    selected: usize,
    option: usize,
    pane: Pane,
    /// Where each game was drawn last frame, so clicks can find it.
    game_rows: Vec<Rect>,
    /// `(left arrow, right arrow)` cells for each drawn option.
    arrows: Vec<(Rect, Rect)>,
}

impl Default for Menu {
    fn default() -> Self {
        Self::new()
    }
}

impl Menu {
    pub fn new() -> Self {
        Self {
            games: rules::games(),
            selected: 0,
            option: 0,
            pane: Pane::Games,
            game_rows: Vec::new(),
            arrows: Vec::new(),
        }
    }

    /// Run the menu until a game is chosen or the player quits.
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<Option<Box<dyn Rules>>> {
        loop {
            terminal.draw(|frame| self.draw(frame))?;
            match self.handle(event::read()?) {
                Some(Choice::Play) => return Ok(Some(self.take())),
                Some(Choice::Quit) => return Ok(None),
                None => {}
            }
        }
    }

    /// A fresh copy of the highlighted game, carrying the rules chosen here.
    ///
    /// The menu keeps its own games so the settings survive coming back to it,
    /// which is why this rebuilds rather than handing its copy over.
    fn take(&self) -> Box<dyn Rules> {
        let source = &self.games[self.selected];
        let mut game = rules::find(source.name()).expect("the menu only lists real games");
        for (index, &value) in source.selected().iter().enumerate() {
            let delta = value as i32 - game.selected()[index] as i32;
            game.configure(index, delta);
        }
        game
    }

    fn current(&self) -> &dyn Rules {
        self.games[self.selected].as_ref()
    }

    fn options_len(&self) -> usize {
        self.current().options().len()
    }

    // -- input ------------------------------------------------------------

    pub fn handle(&mut self, event: Event) -> Option<Choice> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key(key),
            Event::Mouse(mouse) => self.on_mouse(mouse),
            _ => None,
        }
    }

    fn on_key(&mut self, key: KeyEvent) -> Option<Choice> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Some(Choice::Quit);
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Some(Choice::Quit),
            KeyCode::Enter | KeyCode::Char(' ') => return Some(Choice::Play),
            KeyCode::Tab | KeyCode::BackTab => self.switch_pane(),
            KeyCode::Up | KeyCode::Char('k') => self.step(-1),
            KeyCode::Down | KeyCode::Char('j') => self.step(1),
            KeyCode::Left | KeyCode::Char('h') => self.adjust(-1),
            KeyCode::Right | KeyCode::Char('l') => self.adjust(1),
            KeyCode::Char(digit @ '1'..='9') => {
                let index = digit as usize - '1' as usize;
                if index < self.games.len() {
                    self.select(index);
                }
            }
            _ => {}
        }
        None
    }

    fn on_mouse(&mut self, mouse: MouseEvent) -> Option<Choice> {
        let (x, y) = (mouse.column, mouse.row);
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(index) = self.game_rows.iter().position(|row| contains(row, x, y)) {
                    // Clicking the game already highlighted starts it, so the
                    // mouse can play without reaching for the keyboard.
                    if index == self.selected {
                        return Some(Choice::Play);
                    }
                    self.select(index);
                    return None;
                }
                for (index, (left, right)) in self.arrows.clone().into_iter().enumerate() {
                    if contains(&left, x, y) {
                        self.pane = Pane::Rules;
                        self.option = index;
                        self.adjust(-1);
                        return None;
                    }
                    if contains(&right, x, y) {
                        self.pane = Pane::Rules;
                        self.option = index;
                        self.adjust(1);
                        return None;
                    }
                }
            }
            MouseEventKind::ScrollUp => self.step(-1),
            MouseEventKind::ScrollDown => self.step(1),
            _ => {}
        }
        None
    }

    fn select(&mut self, index: usize) {
        self.selected = index;
        self.option = 0;
        // A game with no rules to change has nothing for that pane to steer.
        if self.options_len() == 0 {
            self.pane = Pane::Games;
        }
    }

    fn switch_pane(&mut self) {
        self.pane = match self.pane {
            Pane::Games if self.options_len() > 0 => Pane::Rules,
            _ => Pane::Games,
        };
    }

    fn step(&mut self, delta: isize) {
        match self.pane {
            Pane::Games => {
                let next = (self.selected as isize + delta).rem_euclid(self.games.len() as isize);
                self.select(next as usize);
            }
            Pane::Rules => {
                let count = self.options_len() as isize;
                if count > 0 {
                    self.option = (self.option as isize + delta).rem_euclid(count) as usize;
                }
            }
        }
    }

    /// Move the highlighted rule on by `delta` values.
    fn adjust(&mut self, delta: i32) {
        if self.options_len() == 0 {
            return;
        }
        let option = self.option.min(self.options_len() - 1);
        self.games[self.selected].configure(option, delta);
    }

    // -- drawing ----------------------------------------------------------

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        self.game_rows.clear();
        self.arrows.clear();

        if area.width < MIN_W || area.height < MIN_H {
            Paragraph::new(format!(
                "Terminal too small.\n\nNeed at least {MIN_W}x{MIN_H}, have {}x{}.",
                area.width, area.height
            ))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .render(area, frame.buffer_mut());
            return;
        }

        let popup = centered(WIDTH, area.height.min(20), area);
        Clear.render(popup, frame.buffer_mut());

        // The footer takes the last row; the panes share what is left.
        let body = Rect {
            height: popup.height - 1,
            ..popup
        };
        let list = Rect {
            width: LIST_W.min(body.width),
            ..body
        };
        let preview = Rect {
            x: body.x + list.width,
            width: body.width - list.width,
            ..body
        };

        self.draw_list(frame, list);
        self.draw_preview(frame, preview);
        self.draw_footer(
            frame,
            Rect {
                y: popup.y + popup.height - 1,
                height: 1,
                ..popup
            },
        );
    }

    fn draw_list(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.pane == Pane::Games;
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(if focused { CURSOR } else { MUTED }))
            .title(" Games ")
            .padding(Padding::symmetric(1, 0));
        let inner = block.inner(area);
        block.render(area, frame.buffer_mut());

        for (index, game) in self.games.iter().enumerate() {
            let y = inner.y + index as u16;
            if y >= inner.y + inner.height {
                break;
            }
            let row = Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            };
            let chosen = index == self.selected;
            let style = if chosen && focused {
                Style::new().fg(CURSOR).add_modifier(Modifier::BOLD)
            } else if chosen {
                Style::new().add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(MUTED)
            };
            let marker = if chosen { "▸ " } else { "  " };
            Paragraph::new(Line::from(Span::styled(
                format!("{marker}{}", game.name()),
                style,
            )))
            .render(row, frame.buffer_mut());
            self.game_rows.push(row);
        }
    }

    fn draw_preview(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.pane == Pane::Rules;
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(if focused { CURSOR } else { MUTED }))
            .title(format!(" {} ", self.current().name()))
            .padding(Padding::symmetric(1, 0));
        let inner = block.inner(area);
        block.render(area, frame.buffer_mut());
        if inner.height == 0 {
            return;
        }

        // The blurb wraps, so measure it before deciding where the rules start.
        let blurb = self.current().blurb();
        let wrapped = wrapped_height(blurb, inner.width);
        let blurb_area = Rect {
            height: wrapped.min(inner.height),
            ..inner
        };
        Paragraph::new(blurb)
            .wrap(Wrap { trim: true })
            .style(Style::new().fg(Color::Reset))
            .render(blurb_area, frame.buffer_mut());

        // Copied out before the loop, which also pushes into `self.arrows`.
        let selected = self.current().selected();
        let options: Vec<(&'static str, &'static str)> = self
            .current()
            .options()
            .iter()
            .zip(&selected)
            .map(|(option, &value)| (option.label, option.values[value]))
            .collect();
        if options.is_empty() {
            return;
        }
        let mut y = inner.y + blurb_area.height + 1;
        if y >= inner.y + inner.height {
            return;
        }

        Paragraph::new(Line::from(Span::styled("Rules", Style::new().fg(MUTED)))).render(
            Rect {
                y,
                height: 1,
                ..inner
            },
            frame.buffer_mut(),
        );
        y += 1;

        for (index, &(label, value)) in options.iter().enumerate() {
            if y >= inner.y + inner.height {
                break;
            }
            let highlighted = focused && index == self.option;
            let marker = if highlighted { "▸ " } else { "  " };
            let label = format!("{marker}{label:<10}");
            let style = if highlighted {
                Style::new().fg(CURSOR).add_modifier(Modifier::BOLD)
            } else {
                Style::new()
            };

            let left_x = inner.x + label.chars().count() as u16;
            Paragraph::new(Line::from(vec![
                Span::styled(label, style),
                Span::styled("‹ ", style),
                Span::styled(value.to_string(), Style::new().add_modifier(Modifier::BOLD)),
                Span::styled(" ›", style),
            ]))
            .render(
                Rect {
                    y,
                    height: 1,
                    ..inner
                },
                frame.buffer_mut(),
            );

            let right_x = left_x + 3 + value.chars().count() as u16;
            self.arrows
                .push((Rect::new(left_x, y, 1, 1), Rect::new(right_x, y, 1, 1)));
            y += 1;
        }
    }

    fn draw_footer(&self, frame: &mut Frame, area: Rect) {
        let keys = match self.pane {
            Pane::Games if self.options_len() > 0 => "↑↓ game   tab rules   enter play   q quit",
            Pane::Games => "↑↓ game   enter play   q quit",
            Pane::Rules => "↑↓ rule   ←→ change   tab games   enter play   q quit",
        };
        Paragraph::new(Line::from(Span::styled(keys, Style::new().fg(MUTED))))
            .alignment(Alignment::Center)
            .render(area, frame.buffer_mut());
    }
}

fn contains(rect: &Rect, x: u16, y: u16) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyEventState;

    fn screen(menu: &mut Menu, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| menu.draw(frame)).unwrap();
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

    fn press(menu: &mut Menu, code: KeyCode) -> Option<Choice> {
        menu.handle(Event::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }))
    }

    fn click(menu: &mut Menu, x: u16, y: u16) -> Option<Choice> {
        menu.handle(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }))
    }

    #[test]
    fn the_menu_lists_every_game_and_previews_the_highlighted_one() {
        let mut menu = Menu::new();
        let view = screen(&mut menu, 80, 24);
        println!("{view}");

        for game in rules::games() {
            assert!(view.contains(game.name()), "{} is missing", game.name());
        }
        assert!(view.contains("Games"));
        assert!(view.contains("enter play"));
    }

    #[test]
    fn moving_down_previews_a_different_game() {
        let mut menu = Menu::new();
        let first = menu.current().name();
        press(&mut menu, KeyCode::Down);
        assert_ne!(menu.current().name(), first);

        press(&mut menu, KeyCode::Up);
        assert_eq!(menu.current().name(), first, "and wraps back");
    }

    #[test]
    fn the_game_list_wraps_in_both_directions() {
        let mut menu = Menu::new();
        let count = menu.games.len();
        press(&mut menu, KeyCode::Up);
        assert_eq!(menu.selected, count - 1);
        press(&mut menu, KeyCode::Down);
        assert_eq!(menu.selected, 0);
    }

    #[test]
    fn enter_plays_and_q_quits() {
        let mut menu = Menu::new();
        assert_eq!(press(&mut menu, KeyCode::Enter), Some(Choice::Play));
        assert_eq!(press(&mut menu, KeyCode::Char('q')), Some(Choice::Quit));
        assert_eq!(press(&mut menu, KeyCode::Esc), Some(Choice::Quit));
        assert_eq!(press(&mut menu, KeyCode::Down), None);
    }

    #[test]
    fn tab_only_reaches_a_rules_pane_that_has_rules() {
        let mut menu = Menu::new();
        for index in 0..menu.games.len() {
            menu.select(index);
            menu.pane = Pane::Games;
            menu.switch_pane();
            if menu.options_len() == 0 {
                assert_eq!(
                    menu.pane,
                    Pane::Games,
                    "{} has no rules",
                    menu.current().name()
                );
            } else {
                assert_eq!(menu.pane, Pane::Rules);
                menu.switch_pane();
                assert_eq!(menu.pane, Pane::Games, "and tab goes back");
            }
        }
    }

    #[test]
    fn changing_a_rule_in_the_menu_is_carried_into_the_game() {
        let Some(index) =
            (0..Menu::new().games.len()).find(|&i| !rules::games()[i].options().is_empty())
        else {
            return; // no configurable game yet
        };

        let mut menu = Menu::new();
        menu.select(index);
        menu.switch_pane();
        let before = menu.current().selected()[0];
        press(&mut menu, KeyCode::Right);
        let after = menu.current().selected()[0];
        assert_ne!(before, after, "the rule moved on");

        assert_eq!(
            menu.take().selected()[0],
            after,
            "and the game handed over carries it"
        );
        assert_eq!(
            menu.current().selected()[0],
            after,
            "while the menu keeps its own setting for next time"
        );
    }

    #[test]
    fn rule_values_wrap_around_in_both_directions() {
        let Some(index) =
            (0..Menu::new().games.len()).find(|&i| !rules::games()[i].options().is_empty())
        else {
            return;
        };
        let mut menu = Menu::new();
        menu.select(index);
        menu.switch_pane();

        let count = menu.current().options()[0].values.len();
        let start = menu.current().selected()[0];
        for _ in 0..count {
            press(&mut menu, KeyCode::Right);
        }
        assert_eq!(menu.current().selected()[0], start, "a full cycle returns");

        press(&mut menu, KeyCode::Left);
        press(&mut menu, KeyCode::Right);
        assert_eq!(menu.current().selected()[0], start);
    }

    #[test]
    fn clicking_a_game_selects_it_and_clicking_it_again_plays() {
        let mut menu = Menu::new();
        screen(&mut menu, 80, 24);
        assert!(menu.games.len() > 1, "this test needs a second game");

        let row = menu.game_rows[1];
        assert_eq!(click(&mut menu, row.x + 2, row.y), None);
        assert_eq!(menu.selected, 1, "the click moved the highlight");
        assert_eq!(click(&mut menu, row.x + 2, row.y), Some(Choice::Play));
    }

    #[test]
    fn clicking_a_rule_arrow_changes_that_rule() {
        let mut menu = Menu::new();
        let Some(index) = (0..menu.games.len()).find(|&i| !rules::games()[i].options().is_empty())
        else {
            return;
        };
        menu.select(index);
        screen(&mut menu, 80, 24);
        assert!(!menu.arrows.is_empty(), "the rules were drawn");

        let before = menu.current().selected()[0];
        let (_, right) = menu.arrows[0];
        click(&mut menu, right.x, right.y);
        assert_ne!(menu.current().selected()[0], before);
    }

    #[test]
    fn a_tiny_terminal_says_so_instead_of_drawing_a_broken_menu() {
        let mut menu = Menu::new();
        let view = screen(&mut menu, 30, 8);
        assert!(view.contains("too small"), "got:\n{view}");
    }

    #[test]
    fn the_menu_survives_being_drawn_at_awkward_sizes() {
        let mut menu = Menu::new();
        for (width, height) in [(80, 24), (74, 12), (48, 12), (200, 60), (48, 40)] {
            screen(&mut menu, width, height);
        }
    }
}
