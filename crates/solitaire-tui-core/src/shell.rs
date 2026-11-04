//! The menu and a game taking turns, as state rather than as a loop.
//!
//! A terminal can block on the next key, so `main.rs` could once nest a game's
//! loop inside the menu's. A browser cannot: it calls back with each event and
//! each frame. Keeping the whole program as one value that is fed events and
//! asked to draw is what lets both frontends share it — the terminal wraps it
//! in a loop, the browser wraps it in callbacks.

use ratatui::Frame;

use crate::app::{Exit, Session};
use crate::input::Event;
use crate::rules::Rules;
use crate::ui::menu::{Choice, Menu};

/// Whether the program should keep going after an event.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flow {
    Continue,
    Quit,
}

enum Screen {
    Menu,
    Playing(Box<Session>),
}

pub struct Shell {
    screen: Screen,
    /// Kept across games so the rules chosen in it survive a round trip.
    menu: Menu,
    /// Every deal uses this seed when set — `--seed` or `?seed=`.
    seed: Option<u64>,
    /// Where fresh deals come from: the frontend's clock.
    seeds: fn() -> u64,
    /// Whether the program was started straight into a game. Leaving that game
    /// leaves the program rather than dropping into a menu nobody asked for.
    direct: bool,
    /// Whether there is anywhere to quit *to*. A terminal has its shell; a
    /// browser tab has nothing, so quitting there returns to the menu instead.
    can_quit: bool,
}

impl Shell {
    /// Open the menu, or go straight into `chosen` when one is given.
    pub fn new(chosen: Option<Box<dyn Rules>>, seed: Option<u64>, seeds: fn() -> u64) -> Self {
        let mut shell = Self {
            screen: Screen::Menu,
            menu: Menu::new(),
            seed,
            seeds,
            direct: chosen.is_some(),
            can_quit: true,
        };
        if let Some(game) = chosen {
            shell.start(game);
        }
        shell
    }

    /// Never end the program: quitting goes back to the menu, and a game
    /// started directly still leaves to the menu. For the browser.
    pub fn without_quit(mut self) -> Self {
        self.can_quit = false;
        self.direct = false;
        self
    }

    fn start(&mut self, game: Box<dyn Rules>) {
        let seed = self.seed.unwrap_or_else(self.seeds);
        self.screen = Screen::Playing(Box::new(Session::new(game, seed).with_seeds(self.seeds)));
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        match &mut self.screen {
            Screen::Menu => self.menu.draw(frame),
            Screen::Playing(session) => session.draw(frame),
        }
    }

    pub fn handle(&mut self, event: Event) -> Flow {
        match &mut self.screen {
            Screen::Menu => match self.menu.handle(event) {
                Some(Choice::Play) => {
                    let game = self.menu.take();
                    self.start(game);
                }
                Some(Choice::Quit) if self.can_quit => return Flow::Quit,
                Some(Choice::Quit) | None => {}
            },
            Screen::Playing(session) => match session.handle(event) {
                Some(Exit::Quit) if self.can_quit => return Flow::Quit,
                Some(Exit::Menu) if self.direct => return Flow::Quit,
                Some(_) => self.screen = Screen::Menu,
                None => {}
            },
        }
        Flow::Continue
    }

    /// Whether a game is on screen rather than the menu.
    pub fn is_playing(&self) -> bool {
        matches!(self.screen, Screen::Playing(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{KeyCode, KeyEvent};
    use crate::rules;

    fn seeds() -> u64 {
        7
    }

    #[test]
    fn the_menu_and_a_game_take_turns_until_the_player_quits() {
        let mut shell = Shell::new(None, None, seeds);
        assert!(!shell.is_playing());

        assert_eq!(shell.handle(KeyCode::Enter.into()), Flow::Continue);
        assert!(shell.is_playing());

        assert_eq!(shell.handle(KeyCode::Char('m').into()), Flow::Continue);
        assert!(!shell.is_playing(), "m goes back to the menu");

        assert_eq!(shell.handle(KeyCode::Enter.into()), Flow::Continue);
        assert_eq!(shell.handle(KeyCode::Char('q').into()), Flow::Quit);
    }

    #[test]
    fn a_game_started_directly_leaves_the_program_rather_than_opening_the_menu() {
        let game = rules::find("scorpion").unwrap();
        let mut shell = Shell::new(Some(game), Some(42), seeds);
        assert!(shell.is_playing());
        assert_eq!(shell.handle(KeyCode::Char('m').into()), Flow::Quit);
    }

    #[test]
    fn without_quit_nothing_ends_the_program() {
        let game = rules::find("scorpion").unwrap();
        let mut shell = Shell::new(Some(game), None, seeds).without_quit();

        assert_eq!(shell.handle(KeyCode::Char('q').into()), Flow::Continue);
        assert!(!shell.is_playing(), "quitting a game returns to the menu");

        assert_eq!(shell.handle(KeyCode::Char('q').into()), Flow::Continue);
        let ctrl_c = KeyEvent::ctrl(KeyCode::Char('c'));
        assert_eq!(shell.handle(ctrl_c.into()), Flow::Continue);
        assert!(!shell.is_playing());
    }
}
