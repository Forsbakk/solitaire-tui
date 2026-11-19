//! Everything around the table that is text: the game menu, help, the win
//! dialog and the status line.
//!
//! These are DOM rather than canvas so text is crisp and selectable, buttons
//! are real buttons with real focus and touch targets, and the page's CSS can
//! rearrange them for a phone. This module only writes HTML; `main.rs` puts it
//! on the page and routes the clicks, which name what they do with
//! `data-action` (and `data-game`, `data-option`, `data-value`).

use std::fmt::Write;

use solitaire_tui_core::rules::{Rules, StatusTone};

pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Paragraphs of a blurb, which wraps its lines with `\n\n` between them.
fn paragraphs(text: &str) -> String {
    text.split("\n\n")
        .map(|p| {
            format!(
                "<p>{}</p>",
                escape(&p.split_whitespace().collect::<Vec<_>>().join(" "))
            )
        })
        .collect()
}

/// The first sentence of a blurb, for a game's tile.
fn tagline(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.find(". ") {
        Some(end) => flat[..=end].to_string(),
        None => flat,
    }
}

/// The game selector. `chosen` is the highlighted game; `playing` adds a way
/// back to the game in progress.
pub fn menu(games: &[Box<dyn Rules>], chosen: usize, playing: bool) -> String {
    let mut html = String::from("<header><h2>Choose a game</h2>");
    if playing {
        html.push_str(
            r#"<button class="close" data-action="close" aria-label="Back to the game">✕</button>"#,
        );
    }
    html.push_str(r#"</header><div class="menu"><div class="games" role="listbox">"#);
    for (i, game) in games.iter().enumerate() {
        let _ = write!(
            html,
            r#"<button class="game{}" data-action="choose" data-game="{i}" role="option" aria-selected="{}"><span class="name">{}</span><span class="tag">{}</span></button>"#,
            if i == chosen { " on" } else { "" },
            i == chosen,
            escape(game.name()),
            escape(&tagline(game.blurb())),
        );
    }
    html.push_str("</div>");

    let game = &games[chosen];
    let _ = write!(
        html,
        r#"<div class="detail"><h3>{}</h3>"#,
        escape(game.name())
    );
    html.push_str(&paragraphs(game.blurb()));
    let selected = game.selected();
    for (o, option) in game.options().iter().enumerate() {
        let _ = write!(
            html,
            r#"<div class="option"><span>{}</span><div class="seg">"#,
            escape(option.label)
        );
        for (v, value) in option.values.iter().enumerate() {
            let _ = write!(
                html,
                r#"<button data-action="option" data-option="{o}" data-value="{v}" class="{}">{}</button>"#,
                if selected.get(o) == Some(&v) {
                    "on"
                } else {
                    ""
                },
                escape(value),
            );
        }
        html.push_str("</div></div>");
    }
    html.push_str(
        r#"<div class="actions"><button class="primary" data-action="play">Deal</button><a class="link" href="tui/">Terminal version</a></div></div></div>"#,
    );
    html
}

pub fn help(rules: &dyn Rules) -> String {
    let mut html = format!(
        r#"<header><h2>{}</h2><button class="close" data-action="close" aria-label="Close">✕</button></header><div class="help">"#,
        escape(rules.name())
    );
    html.push_str(&paragraphs(rules.blurb()));
    html.push_str(
        r#"<h3>Mouse and touch</h3><dl>
<dt>Drag</dt><dd>move a card, or a run from the card you grab</dd>
<dt>Tap, then tap</dt><dd>pick a card up, then put it down where you tap</dd>
<dt>Double-tap</dt><dd>send a card home, or to the one place it fits</dd>
<dt>Tap the stock</dt><dd>deal</dd>
<dt>Right-click</dt><dd>put down what you are holding</dd>
</dl>
<h3>Keyboard</h3><dl>
<dt>← → ↑ ↓</dt><dd>move between piles and cards (also h l k j)</dd>
<dt>Space / Enter</dt><dd>pick up, or put down on the cursor's pile</dd>
<dt>1 – 9, 0</dt><dd>drop on (or jump to) that column</dd>
<dt>f</dt><dd>send the card home, or to its only column</dd>
<dt>d</dt><dd>deal from the stock</dd>
<dt>H</dt><dd>hint; again for the next one</dd>
<dt>A</dt><dd>finish once every card is free</dd>
<dt>u, Ctrl-Z</dt><dd>undo</dd>
<dt>Ctrl-R, Ctrl-Y</dt><dd>redo</dd>
<dt>n / r</dt><dd>new deal / replay this deal</dd>
<dt>Esc</dt><dd>put down what you are holding</dd>
<dt>m</dt><dd>choose another game</dd>
<dt>?</dt><dd>this help</dd>
</dl>"#,
    );
    html.push_str(
        r#"<div class="actions"><button data-action="restart">Replay this deal</button><button class="primary" data-action="close">Back to the game</button></div></div>"#,
    );
    html
}

pub fn win(rules: &dyn Rules, moves: u32, seed: u64) -> String {
    format!(
        r#"<div class="win"><div class="trophy">♛</div><h2>You won!</h2><p>{} in {moves} moves.</p><p class="muted">Deal #{seed}</p><div class="actions"><button data-action="menu">Games</button><button class="primary" data-action="new">New deal</button></div></div>"#,
        escape(rules.name())
    )
}

/// The status line: the game, its deal and move count, and whatever the game
/// itself counts.
pub fn status(rules: &dyn Rules, board: &solitaire_tui_core::board::Board) -> String {
    let mut html = format!(
        r#"<span class="title">{}</span><span class="stat"><span>moves</span> {}</span><span class="stat wide"><span>deal</span> #{}</span>"#,
        escape(rules.name()),
        board.moves(),
        board.seed()
    );
    for item in rules.status(board) {
        let _ = write!(
            html,
            r#"<span class="stat {}"><span>{}</span> {}</span>"#,
            match item.tone {
                StatusTone::Good => "good",
                StatusTone::Muted => "muted",
            },
            escape(item.label),
            escape(&item.value)
        );
    }
    html
}

#[cfg(test)]
mod tests {
    use super::*;
    use solitaire_tui_core::rules;

    #[test]
    fn every_game_offers_its_options_in_the_menu() {
        let games = rules::games();
        for (i, game) in games.iter().enumerate() {
            let html = menu(&games, i, false);
            for option in game.options() {
                assert!(
                    html.contains(option.label),
                    "{}: {}",
                    game.name(),
                    option.label
                );
            }
            assert!(html.contains(r#"data-action="play""#));
        }
    }

    #[test]
    fn text_from_the_rules_is_escaped() {
        assert_eq!(escape("<b>&\""), "&lt;b&gt;&amp;&quot;");
    }

    #[test]
    fn a_tagline_is_the_first_sentence() {
        assert_eq!(tagline("One thing.  Two\nthings."), "One thing.");
    }
}
