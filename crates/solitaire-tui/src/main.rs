//! Solitaire for the terminal.

mod app;
mod board;
mod card;
mod rules;
mod ui;

use std::io::stdout;

use anyhow::{Result, anyhow};
use clap::Parser;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event;
use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;

use app::{Exit, Session};
use rules::Rules;

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
        Some(name) => Some(configure(name, &cli.rules)?),
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

    let result = run(&mut terminal, chosen, cli.seed);

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

/// Look up `name` and apply every `key=value` in `settings` to it.
fn configure(name: &str, settings: &[String]) -> Result<Box<dyn Rules>> {
    let mut game = rules::find(name).ok_or_else(|| {
        let known: Vec<_> = rules::games().iter().map(|game| game.name()).collect();
        anyhow!("no game called {name:?}; try {}", known.join(", "))
    })?;

    for setting in settings {
        let (key, value) = setting
            .split_once('=')
            .ok_or_else(|| anyhow!("--rule wants KEY=VALUE, got {setting:?}"))?;
        rules::set_option(&mut game, key, value).map_err(|error| anyhow!(error))?;
    }
    Ok(game)
}

/// The menu and a game take turns until the player quits.
fn run(
    terminal: &mut DefaultTerminal,
    chosen: Option<Box<dyn Rules>>,
    seed: Option<u64>,
) -> Result<()> {
    // `--game` skips the menu, and leaving that game leaves the app rather
    // than dropping into a menu the player never asked for.
    if let Some(game) = chosen {
        play(terminal, game, seed.unwrap_or_else(app::random_seed))?;
        return Ok(());
    }

    let mut menu = ui::menu::Menu::new();
    loop {
        let Some(game) = menu.run(terminal)? else {
            return Ok(());
        };
        if play(terminal, game, seed.unwrap_or_else(app::random_seed))? == Exit::Quit {
            return Ok(());
        }
    }
}

fn play(terminal: &mut DefaultTerminal, game: Box<dyn Rules>, seed: u64) -> Result<Exit> {
    let mut session = Session::new(game, seed);
    loop {
        terminal.draw(|frame| session.draw(frame))?;
        if let Some(exit) = session.handle(event::read()?) {
            return Ok(exit);
        }
    }
}

fn release_mouse_on_panic() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(stdout(), DisableMouseCapture);
        previous(info);
    }));
}
