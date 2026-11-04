//! Solitaire for the terminal.

use std::io::stdout;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use clap::Parser;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode as CKeyCode,
    KeyEventKind, KeyModifiers, MouseButton as CButton, MouseEventKind as CMouseKind,
};
use ratatui::crossterm::execute;

use solitaire_tui_core::input::{
    Event, KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind,
};
use solitaire_tui_core::rules;
use solitaire_tui_core::shell::{Flow, Shell};

#[derive(Parser)]
#[command(
    name = "solitaire-tui",
    version,
    about = "Solitaire in the terminal",
    after_help = "With no --game, the menu opens so a game can be picked there."
)]
struct Cli {
    /// Play this game straight away, skipping the menu.
    #[arg(long, value_name = "NAME")]
    game: Option<String>,

    /// Set one of the chosen game's rules, e.g. --rule draw=1.
    #[arg(long = "rule", value_name = "KEY=VALUE")]
    rules: Vec<String>,

    /// Deal a specific game. The same seed always deals the same cards.
    #[arg(long, value_name = "N")]
    seed: Option<u64>,

    /// List the games that can be played and their rules.
    #[arg(long)]
    list: bool,

    /// Keyboard only; leave the terminal's own mouse handling alone.
    #[arg(long)]
    no_mouse: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.list {
        list_games();
        return Ok(());
    }

    // Resolve the game and its rules before touching the terminal, so a
    // mistyped name is an ordinary error rather than a flash of alternate
    // screen.
    let chosen = match &cli.game {
        Some(name) => Some(rules::configure(name, &cli.rules).map_err(|error| anyhow!(error))?),
        None if !cli.rules.is_empty() => {
            return Err(anyhow!("--rule needs a --game to apply to"));
        }
        None => None,
    };

    let mouse = !cli.no_mouse;
    let mut terminal = ratatui::init();
    // Installed after `init`, so on a panic this runs first and mouse capture
    // is released before ratatui leaves the alternate screen.
    if mouse {
        release_mouse_on_panic();
        execute!(stdout(), EnableMouseCapture)?;
    }

    let result = run(&mut terminal, Shell::new(chosen, cli.seed, clock_seed));

    if mouse {
        let _ = execute!(stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    result
}

fn list_games() {
    for game in rules::games() {
        println!("{}", game.name());
        for option in game.options() {
            println!(
                "    --rule {}={}",
                option.label.to_lowercase(),
                option.values.join("|")
            );
        }
    }
}

/// The menu and a game take turns until the player quits.
fn run(terminal: &mut DefaultTerminal, mut shell: Shell) -> Result<()> {
    loop {
        terminal.draw(|frame| shell.draw(frame))?;
        if let Some(event) = translate(event::read()?)
            && shell.handle(event) == Flow::Quit
        {
            return Ok(());
        }
    }
}

/// A seed for a fresh deal. Only needs to differ run to run.
fn clock_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos() as u64)
}

/// A crossterm event as the game reads it, or `None` for anything it ignores:
/// key releases and repeats (which Windows reports), mouse moves and drags,
/// focus changes. A resize needs no event of its own — the next frame is
/// drawn at the new size.
fn translate(event: CEvent) -> Option<Event> {
    match event {
        CEvent::Key(key) if key.kind == KeyEventKind::Press => {
            let code = match key.code {
                CKeyCode::Char(c) => KeyCode::Char(c),
                CKeyCode::Up => KeyCode::Up,
                CKeyCode::Down => KeyCode::Down,
                CKeyCode::Left => KeyCode::Left,
                CKeyCode::Right => KeyCode::Right,
                CKeyCode::Enter => KeyCode::Enter,
                CKeyCode::Esc => KeyCode::Esc,
                CKeyCode::Tab => KeyCode::Tab,
                CKeyCode::BackTab => KeyCode::BackTab,
                _ => return None,
            };
            Some(Event::Key(KeyEvent {
                code,
                ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
            }))
        }
        CEvent::Mouse(mouse) => {
            let kind = match mouse.kind {
                CMouseKind::Down(CButton::Left) => MouseEventKind::Down(MouseButton::Left),
                CMouseKind::Down(CButton::Right) => MouseEventKind::Down(MouseButton::Right),
                CMouseKind::Down(CButton::Middle) => MouseEventKind::Down(MouseButton::Middle),
                CMouseKind::ScrollUp => MouseEventKind::ScrollUp,
                CMouseKind::ScrollDown => MouseEventKind::ScrollDown,
                _ => return None,
            };
            Some(Event::Mouse(MouseEvent {
                kind,
                column: mouse.column,
                row: mouse.row,
            }))
        }
        _ => None,
    }
}

fn release_mouse_on_panic() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(stdout(), DisableMouseCapture);
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyEvent as CKeyEvent, KeyEventState, MouseEvent as CMouse};

    fn key(code: CKeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> CEvent {
        CEvent::Key(CKeyEvent {
            code,
            modifiers,
            kind,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn only_presses_reach_the_game() {
        let press = key(CKeyCode::Char('d'), KeyModifiers::NONE, KeyEventKind::Press);
        assert_eq!(translate(press), Some(KeyCode::Char('d').into()));
        // Windows reports releases too; acting on them would deal twice.
        let release = key(
            CKeyCode::Char('d'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        );
        assert_eq!(translate(release), None);
    }

    #[test]
    fn ctrl_survives_translation_and_shift_is_already_in_the_character() {
        let redo = key(
            CKeyCode::Char('r'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        );
        assert_eq!(
            translate(redo),
            Some(KeyEvent::ctrl(KeyCode::Char('r')).into())
        );
        let hint = key(
            CKeyCode::Char('H'),
            KeyModifiers::SHIFT,
            KeyEventKind::Press,
        );
        assert_eq!(translate(hint), Some(KeyCode::Char('H').into()));
    }

    #[test]
    fn clicks_keep_their_cell_and_moves_are_dropped() {
        let at = |kind| {
            CEvent::Mouse(CMouse {
                kind,
                column: 12,
                row: 7,
                modifiers: KeyModifiers::NONE,
            })
        };
        assert_eq!(
            translate(at(CMouseKind::Down(CButton::Left))),
            Some(Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 12,
                row: 7,
            }))
        );
        assert_eq!(translate(at(CMouseKind::Moved)), None);
        assert_eq!(translate(at(CMouseKind::Up(CButton::Left))), None);
    }
}
