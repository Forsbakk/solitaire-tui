//! Scorpion solitaire for the terminal.

mod app;
mod card;
mod game;
mod ui;

use std::io::stdout;

use anyhow::Result;
use clap::Parser;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event;
use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;

use app::App;

#[derive(Parser)]
#[command(
    name = "scorpion",
    version,
    about = "Scorpion solitaire in the terminal"
)]
struct Cli {
    /// Deal a specific game. The same seed always deals the same cards.
    #[arg(long, value_name = "N")]
    seed: Option<u64>,

    /// Keyboard only; leave the terminal's own mouse handling alone.
    #[arg(long)]
    no_mouse: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let seed = cli.seed.unwrap_or_else(app::random_seed);
    let mouse = !cli.no_mouse;

    let mut terminal = ratatui::init();
    // Installed after `init`, so on a panic this runs first and mouse capture
    // is released before ratatui leaves the alternate screen.
    if mouse {
        release_mouse_on_panic();
        execute!(stdout(), EnableMouseCapture)?;
    }

    let result = run(&mut terminal, App::new(seed));

    if mouse {
        let _ = execute!(stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal, mut app: App) -> Result<()> {
    while !app.quit {
        terminal.draw(|frame| app.draw(frame))?;
        app.handle(event::read()?);
    }
    Ok(())
}

fn release_mouse_on_panic() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(stdout(), DisableMouseCapture);
        previous(info);
    }));
}
