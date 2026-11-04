//! The solitaire engine: cards, boards, every game's rules, and the drawing —
//! everything except a terminal.
//!
//! Two frontends sit on top. `solitaire-tui` feeds [`shell::Shell`] crossterm
//! events in a loop; `solitaire-web` feeds it Ratzilla's from browser
//! callbacks. Both translate into [`input::Event`] first, so nothing in here
//! depends on where the player is sitting.

pub mod app;
pub mod board;
pub mod card;
pub mod input;
pub mod rules;
pub mod shell;
pub mod ui;
