//! solitaire-tui in a browser tab.
//!
//! The whole game is `solitaire_tui_core::shell::Shell`; this file only hands it
//! browser events and lets Ratzilla draw its frames into the DOM. Build it with
//! Trunk from this directory: `trunk serve` to play locally, `trunk build
//! --release` for the static site.
//!
//! The query string picks a deal the way the command line does:
//! `?game=klondike&rule=draw=1&seed=42`. A seed makes a link shareable — the
//! same seed deals the same cards here as in the terminal.
//!
//! Input is wired to the document by hand rather than through Ratzilla's
//! `on_key_event`/`on_mouse_event`. Those attach to the grid element, which the
//! DOM backend throws away and rebuilds on every window resize — taking the
//! listeners with it — and they cannot stop the browser acting on the same key
//! (Tab leaving the page, arrows and space scrolling it).

use std::cell::RefCell;
use std::rc::Rc;

use ratzilla::ratatui::Terminal;
use ratzilla::{DomBackend, WebRenderer};
use solitaire_tui_core::input::{
    Event, KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind,
};
use solitaire_tui_core::rules::{self, Rules};
use solitaire_tui_core::shell::Shell;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use web_sys::{Element, EventTarget};

fn main() {
    let window = web_sys::window().expect("running in a browser");
    let (chosen, seed) = from_query(&window.location().search().unwrap_or_default());
    let shell = Rc::new(RefCell::new(
        Shell::new(chosen, seed, random_seed).without_quit(),
    ));

    let document = window.document().expect("a page to draw into");
    listen(&document, "keydown", {
        let shell = shell.clone();
        move |event: web_sys::KeyboardEvent| {
            if let Some(key) = translate_key(&event) {
                event.prevent_default();
                shell.borrow_mut().handle(Event::Key(key));
            }
        }
    });
    listen(&document, "mousedown", {
        let shell = shell.clone();
        move |event: web_sys::MouseEvent| {
            if let Some(mouse) = translate_click(&event) {
                shell.borrow_mut().handle(Event::Mouse(mouse));
            }
        }
    });
    listen(&document, "wheel", {
        let shell = shell.clone();
        move |event: web_sys::WheelEvent| {
            // Scrolling only ever steps the cursor, never aims, so where the
            // pointer is does not matter.
            let kind = if event.delta_y() < 0.0 {
                MouseEventKind::ScrollUp
            } else {
                MouseEventKind::ScrollDown
            };
            shell.borrow_mut().handle(Event::Mouse(MouseEvent {
                kind,
                column: 0,
                row: 0,
            }));
        }
    });
    // Right-click drops what is held, as in the terminal — not a menu.
    listen(&document, "contextmenu", |event: web_sys::MouseEvent| {
        event.prevent_default();
    });

    let backend = DomBackend::new().expect("a DOM to draw into");
    let terminal = Terminal::new(backend).expect("a terminal over the DOM");
    terminal.draw_web(move |frame| shell.borrow_mut().draw(frame));
}

/// Call `handler` for every `kind` event on `target`, for the life of the page.
fn listen<E: JsCast + wasm_bindgen::convert::FromWasmAbi + 'static>(
    target: &EventTarget,
    kind: &str,
    handler: impl FnMut(E) + 'static,
) {
    let closure = Closure::<dyn FnMut(E)>::new(handler);
    target
        .add_event_listener_with_callback(kind, closure.as_ref().unchecked_ref())
        .expect("listening for input");
    // The page never removes these, so the closure lives as long as it does.
    closure.forget();
}

/// The game and seed named in the query string, if any.
///
/// A name that is not a game, or a rule that does not fit it, falls back to the
/// menu rather than leaving a blank page — and says why in the console.
fn from_query(search: &str) -> (Option<Box<dyn Rules>>, Option<u64>) {
    let mut game = None;
    let mut settings = Vec::new();
    let mut seed = None;
    for pair in search.trim_start_matches('?').split('&') {
        match pair.split_once('=') {
            Some(("game", name)) => game = Some(name.to_owned()),
            Some(("rule", setting)) => settings.push(setting.replace("%3D", "=")),
            Some(("seed", value)) => seed = value.parse().ok(),
            _ => {}
        }
    }
    let chosen = game.and_then(|name| {
        rules::configure(&name, &settings)
            .inspect_err(|error| web_sys::console::warn_1(&error.into()))
            .ok()
    });
    (chosen, seed)
}

/// A fresh deal. Only needs to differ visit to visit.
fn random_seed() -> u64 {
    (js_sys::Math::random() * (1u64 << 53) as f64) as u64
}

/// A key the game listens for, or `None` to leave it to the browser.
fn translate_key(event: &web_sys::KeyboardEvent) -> Option<KeyEvent> {
    // Browser shortcuts stay the browser's — except Ctrl-R, which is redo here
    // just as in the terminal. Ctrl-C is copy, not quit: a tab has nothing to
    // quit to.
    let ctrl = event.ctrl_key();
    if event.meta_key() || event.alt_key() {
        return None;
    }
    let code = match event.key().as_str() {
        "ArrowUp" => KeyCode::Up,
        "ArrowDown" => KeyCode::Down,
        "ArrowLeft" => KeyCode::Left,
        "ArrowRight" => KeyCode::Right,
        "Enter" => KeyCode::Enter,
        "Escape" => KeyCode::Esc,
        "Tab" if event.shift_key() => KeyCode::BackTab,
        "Tab" => KeyCode::Tab,
        key => {
            let mut chars = key.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => KeyCode::Char(c),
                _ => return None,
            }
        }
    };
    if ctrl && code != KeyCode::Char('r') {
        return None;
    }
    Some(KeyEvent { code, ctrl })
}

/// The cell under a click, read off the DOM itself.
///
/// The DOM backend draws one `<pre>` per row and one `<span>` per cell, so the
/// clicked span's position among its siblings *is* the column, and its row's
/// position the row. That is exact whatever the font, zoom or resize history —
/// no pixel arithmetic to drift out of step with what was drawn, which is the
/// same promise `layout::hit` makes in the terminal.
fn translate_click(event: &web_sys::MouseEvent) -> Option<MouseEvent> {
    let button = match event.button() {
        0 => MouseButton::Left,
        1 => MouseButton::Middle,
        2 => MouseButton::Right,
        _ => return None,
    };
    let cell: Element = event.target()?.dyn_into().ok()?;
    let line = cell.parent_element()?;
    if cell.tag_name() != "SPAN" || line.tag_name() != "PRE" {
        return None;
    }
    let grid = line.parent_element()?;
    Some(MouseEvent {
        kind: MouseEventKind::Down(button),
        column: index_in(&line, &cell)?,
        row: index_in(&grid, &line)?,
    })
}

fn index_in(parent: &Element, child: &Element) -> Option<u16> {
    let children = parent.children();
    (0..children.length())
        .find(|&index| children.item(index).as_ref() == Some(child))
        .and_then(|index| u16::try_from(index).ok())
}
