//! solitaire-tui's games on a graphical card table, in a browser tab.
//!
//! The terminal's engine runs the game: a `solitaire_tui_core::app::Session`
//! owns the board, the cursor, the held tail, hints and undo, and is fed the
//! same keys the terminal sends it. What this crate adds is everything a
//! terminal does not have — a pixel layout ([`layout`]), cards painted on a
//! canvas ([`cards`], [`render`]), cards that travel ([`anim`]), dragging,
//! touch, and HTML around the table ([`chrome`]).
//!
//! Pointer presses go to `Session::click` with the pile and card resolved
//! against this crate's own layout, so a press, a drag and a tap-then-tap all
//! become the same pick-up-and-drop the terminal's mouse does.
//!
//! Build it with Trunk from this directory: `trunk serve`, or `trunk build
//! --release` for the static site. The query string picks a deal as the
//! terminal build's does: `?game=klondike&rule=draw=1&seed=42`.

mod anim;
mod cards;
mod chrome;
mod layout;
mod render;

use std::cell::RefCell;
use std::rc::Rc;

use solitaire_tui_core::app::Session;
use solitaire_tui_core::board::{PileId, PileKind};
use solitaire_tui_core::card::Suit;
use solitaire_tui_core::input::{Event, KeyCode, KeyEvent};
use solitaire_tui_core::rules::{self, Rules};
use solitaire_tui_core::ui::layout::Hit;
use solitaire_tui_core::ui::render::Tone;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use web_sys::{
    CanvasRenderingContext2d, Document, Element, EventTarget, HtmlCanvasElement, HtmlElement,
    KeyboardEvent, PointerEvent, Window,
};

use crate::anim::Animator;
use crate::layout::{Layout, Rect};
use crate::render::{Particle, Scene};

/// How far a press must travel before it is a drag rather than a tap.
const DRAG_SLOP: f64 = 6.0;
/// How soon a second tap on the same card counts as a double-tap.
const DOUBLE_TAP_MS: f64 = 380.0;
/// Where the chosen game and its rules are remembered between visits.
const STORE_GAME: &str = "solitaire-gui.game";
const STORE_RULES: &str = "solitaire-gui.rules";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Overlay {
    None,
    Menu,
    Help,
    Win,
}

/// A press that picked a card up and may yet become a drag.
struct Press {
    id: i32,
    x0: f64,
    y0: f64,
    dx: f64,
    dy: f64,
    dragging: bool,
    /// Pressed on the card already held: a tap puts it back down.
    deselect_on_tap: bool,
}

struct Tap {
    at: f64,
    pile: PileId,
    card: Option<usize>,
}

struct App {
    window: Window,
    document: Document,
    canvas: HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    table: HtmlElement,
    overlay_el: HtmlElement,
    /// Every game, carrying the rules the player last chose for it.
    games: Vec<Box<dyn Rules>>,
    chosen: usize,
    session: Option<Session>,
    layout: Option<Layout>,
    anim: Animator,
    press: Option<Press>,
    last_tap: Option<Tap>,
    /// Shown once a key is used, hidden again by the pointer — a mouse or
    /// touch player has no use for a keyboard cursor.
    show_cursor: bool,
    overlay: Overlay,
    won: bool,
    particles: Vec<Particle>,
    last_frame: f64,
    width: f64,
    height: f64,
    dpr: f64,
    raf: Option<js_sys::Function>,
    frame_pending: bool,
}

fn main() {
    let window = web_sys::window().expect("running in a browser");
    let document = window.document().expect("a page");
    let by_id = |id: &str| -> HtmlElement {
        document
            .get_element_by_id(id)
            .unwrap_or_else(|| panic!("#{id} in index.html"))
            .unchecked_into()
    };
    let canvas: HtmlCanvasElement = by_id("felt").unchecked_into();
    let ctx: CanvasRenderingContext2d = canvas
        .get_context("2d")
        .ok()
        .flatten()
        .expect("a 2d canvas")
        .unchecked_into();

    let app = Rc::new(RefCell::new(App {
        table: by_id("table"),
        overlay_el: by_id("overlay"),
        window: window.clone(),
        document: document.clone(),
        canvas: canvas.clone(),
        ctx,
        games: rules::games(),
        chosen: 0,
        session: None,
        layout: None,
        anim: Animator::default(),
        press: None,
        last_tap: None,
        show_cursor: false,
        overlay: Overlay::None,
        won: false,
        particles: Vec::new(),
        last_frame: 0.0,
        width: 0.0,
        height: 0.0,
        dpr: 1.0,
        raf: None,
        frame_pending: false,
    }));

    let raf = Closure::<dyn FnMut(f64)>::new({
        let app = app.clone();
        move |time: f64| app.borrow_mut().frame(time)
    });
    app.borrow_mut().raf = Some(raf.as_ref().unchecked_ref::<js_sys::Function>().clone());
    raf.forget();

    let canvas_target: &EventTarget = canvas.as_ref();
    listen(canvas_target, "pointerdown", {
        let app = app.clone();
        move |event: PointerEvent| app.borrow_mut().pointer_down(&event)
    });
    listen(canvas_target, "pointermove", {
        let app = app.clone();
        move |event: PointerEvent| app.borrow_mut().pointer_move(&event)
    });
    listen(canvas_target, "pointerup", {
        let app = app.clone();
        move |event: PointerEvent| app.borrow_mut().pointer_up(&event, false)
    });
    listen(canvas_target, "pointercancel", {
        let app = app.clone();
        move |event: PointerEvent| app.borrow_mut().pointer_up(&event, true)
    });
    listen(
        canvas_target,
        "contextmenu",
        |event: web_sys::MouseEvent| {
            event.prevent_default();
        },
    );
    listen(&document, "keydown", {
        let app = app.clone();
        move |event: KeyboardEvent| app.borrow_mut().key_down(&event)
    });
    listen(&document, "click", {
        let app = app.clone();
        move |event: web_sys::MouseEvent| app.borrow_mut().button(&event)
    });

    let observer = Closure::<dyn FnMut(js_sys::Array)>::new({
        let app = app.clone();
        move |_entries| app.borrow_mut().resize()
    });
    let resize =
        web_sys::ResizeObserver::new(observer.as_ref().unchecked_ref()).expect("ResizeObserver");
    resize.observe(&by_id("table"));
    observer.forget();
    // Zooming changes the pixel density without always changing the size.
    listen(&window, "resize", {
        let app = app.clone();
        move |_: web_sys::Event| app.borrow_mut().resize()
    });

    app.borrow_mut().boot();
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
    closure.forget();
}

/// A fresh deal. Only needs to differ visit to visit.
fn random_seed() -> u64 {
    (js_sys::Math::random() * (1u64 << 53) as f64) as u64
}

/// The rules `game` is set to, as `label=value` settings `rules::configure`
/// reads back.
fn settings(game: &dyn Rules) -> Vec<String> {
    game.options()
        .iter()
        .zip(game.selected())
        .map(|(option, value)| format!("{}={}", option.label, option.values[value]))
        .collect()
}

/// Undo a query string's percent-encoding. Malformed escapes are kept as
/// they are rather than failing the whole link.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (b'+', _) => {
                out.push(b' ');
                i += 1;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The game, rules and seed in a query string.
fn from_query(search: &str) -> (Option<String>, Vec<String>, Option<u64>) {
    let mut game = None;
    let mut settings = Vec::new();
    let mut seed = None;
    for pair in search.trim_start_matches('?').split('&') {
        match pair.split_once('=') {
            Some(("game", name)) => game = Some(decode(name)),
            Some(("rule", setting)) => settings.push(decode(setting)),
            Some(("seed", value)) => seed = value.parse().ok(),
            _ => {}
        }
    }
    (game, settings, seed)
}

/// A key the game listens for, or `None` to leave it to the browser.
///
/// Ctrl-Z and Ctrl-Y are what a browser player reaches for to undo and redo,
/// so they are spelled the terminal's way (`u`, Ctrl-R) before the game sees
/// them. Every other Ctrl, Alt and Meta chord stays the browser's.
fn translate_key(event: &KeyboardEvent) -> Option<KeyEvent> {
    if event.meta_key() || event.alt_key() {
        return None;
    }
    let ctrl = event.ctrl_key();
    let code = match event.key().as_str() {
        "ArrowUp" => KeyCode::Up,
        "ArrowDown" => KeyCode::Down,
        "ArrowLeft" => KeyCode::Left,
        "ArrowRight" => KeyCode::Right,
        "Enter" => KeyCode::Enter,
        "Escape" => KeyCode::Esc,
        "Tab" if event.shift_key() => KeyCode::BackTab,
        "Tab" => KeyCode::Tab,
        "Backspace" => KeyCode::Char('u'),
        key => {
            let mut chars = key.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => KeyCode::Char(c),
                _ => return None,
            }
        }
    };
    if !ctrl {
        return Some(KeyEvent::new(code));
    }
    match code {
        KeyCode::Char('z') => Some(KeyEvent::new(KeyCode::Char('u'))),
        KeyCode::Char('Z' | 'y' | 'r') => Some(KeyEvent::ctrl(KeyCode::Char('r'))),
        _ => None,
    }
}

impl App {
    fn now(&self) -> f64 {
        self.window.performance().map_or(0.0, |p| p.now())
    }

    fn storage(&self) -> Option<web_sys::Storage> {
        self.window.local_storage().ok().flatten()
    }

    // -- starting games -----------------------------------------------------

    /// The game the URL names, else the one played last, else Klondike under
    /// the menu.
    fn boot(&mut self) {
        self.resize();
        let search = self.window.location().search().unwrap_or_default();
        let (game, query_rules, seed) = from_query(&search);
        let stored = self.storage().and_then(|s| {
            let game = s.get_item(STORE_GAME).ok().flatten()?;
            let rules = s.get_item(STORE_RULES).ok().flatten().unwrap_or_default();
            Some((
                game,
                rules
                    .split(';')
                    .filter(|r| !r.is_empty())
                    .map(String::from)
                    .collect(),
            ))
        });
        let (wanted, wanted_rules, from_url) = match (game, stored.clone()) {
            (Some(game), _) => (Some(game), query_rules, true),
            (None, Some((game, rules))) => (Some(game), rules, false),
            (None, None) => (None, Vec::new(), false),
        };
        if let Some(name) = wanted {
            match rules::configure(&name, &wanted_rules) {
                Ok(game) => {
                    let index = self
                        .games
                        .iter()
                        .position(|g| g.name() == game.name())
                        .unwrap_or(0);
                    self.games[index] = game;
                    self.start(index, if from_url { seed } else { None });
                    return;
                }
                Err(error) => web_sys::console::warn_1(&error.into()),
            }
        }
        self.start(0, None);
        self.open(Overlay::Menu);
    }

    /// Deal `games[index]` with its current rules.
    fn start(&mut self, index: usize, seed: Option<u64>) {
        self.chosen = index;
        let game = &self.games[index];
        let fresh = rules::configure(game.name(), &settings(game.as_ref()))
            .expect("a registered game takes its own settings");
        if let Some(store) = self.storage() {
            let _ = store.set_item(STORE_GAME, game.name());
            let _ = store.set_item(STORE_RULES, &settings(game.as_ref()).join(";"));
        }
        let seed = seed.unwrap_or_else(random_seed);
        self.session = Some(Session::new(fresh, seed).with_seeds(random_seed));
        self.dealt();
    }

    /// A fresh deal is on the board: fly it in from the stock.
    fn dealt(&mut self) {
        self.won = false;
        self.particles.clear();
        self.press = None;
        self.relayout();
        if let (Some(session), Some(layout)) = (&self.session, &self.layout) {
            let origin = layout
                .piles
                .iter()
                .find(|p| session.board().pile(p.pile).kind() == PileKind::Stock)
                .map(|p| p.base)
                .unwrap_or_else(|| {
                    Rect::new(
                        (layout.width - layout.card_w) / 2.0,
                        -layout.card_h * 1.2,
                        layout.card_w,
                        layout.card_h,
                    )
                });
            let now = self.now();
            self.anim
                .diff(&[], session.board(), layout, Some(origin), now);
        }
        self.write_url();
        if self.overlay == Overlay::Win {
            self.close();
        }
        self.changed();
    }

    /// Put the current deal in the address bar, so a reload or a shared link
    /// deals the same cards.
    fn write_url(&self) {
        let Some(session) = &self.session else { return };
        let rules = session.rules();
        let mut query = format!("?game={}", rules.name().to_lowercase());
        for setting in settings(rules) {
            query.push_str("&rule=");
            query.push_str(&String::from(js_sys::encode_uri_component(
                &setting.to_lowercase(),
            )));
        }
        query.push_str(&format!("&seed={}", session.board().seed()));
        if let Ok(history) = self.window.history() {
            let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&query));
        }
    }

    // -- geometry and drawing -----------------------------------------------

    fn resize(&mut self) {
        let width = f64::from(self.table.client_width());
        let height = f64::from(self.table.client_height());
        let dpr = self.window.device_pixel_ratio().max(1.0);
        if (width, height, dpr) == (self.width, self.height, self.dpr) {
            return;
        }
        (self.width, self.height, self.dpr) = (width, height, dpr);
        self.canvas.set_width((width * dpr).round() as u32);
        self.canvas.set_height((height * dpr).round() as u32);
        self.anim.clear();
        self.relayout();
        self.request_frame();
    }

    fn relayout(&mut self) {
        self.layout = self
            .session
            .as_ref()
            .and_then(|s| layout::compute(s.spec(), s.board(), self.width, self.height));
    }

    fn request_frame(&mut self) {
        if self.frame_pending {
            return;
        }
        if let Some(raf) = &self.raf {
            self.frame_pending = self.window.request_animation_frame(raf).is_ok();
        }
    }

    fn frame(&mut self, time: f64) {
        self.frame_pending = false;
        let dt = ((time - self.last_frame) / 1000.0).clamp(0.0, 0.05);
        self.last_frame = time;
        let height = self.height;
        self.particles.retain_mut(|p| p.step(dt, height));
        self.draw(self.now());
        if self.anim.is_busy(self.now()) || !self.particles.is_empty() {
            self.request_frame();
        }
    }

    fn draw(&mut self, now: f64) {
        let _ = self
            .ctx
            .set_transform(self.dpr, 0.0, 0.0, self.dpr, 0.0, 0.0);
        let (Some(session), Some(layout)) = (&self.session, &self.layout) else {
            render::too_small(&self.ctx, self.width, self.height);
            return;
        };
        let focus = session.focus();
        let hinting = session.hint().is_some();
        let scene = Scene {
            board: session.board(),
            layout,
            anim: &self.anim,
            cursor: (self.show_cursor || hinting).then_some((focus.pile, focus.card)),
            selection: focus.selection,
            hint_target: focus.hint_target,
            drag: self
                .press
                .as_ref()
                .filter(|p| p.dragging)
                .map(|p| (p.dx, p.dy)),
            particles: &self.particles,
            now,
        };
        render::draw(&self.ctx, &scene);
    }

    // -- changing the game --------------------------------------------------

    /// Run `change` against the session and animate whatever it did.
    fn act(&mut self, change: impl FnOnce(&mut Session)) {
        let now = self.now();
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let lift = self
            .press
            .as_ref()
            .filter(|p| p.dragging)
            .zip(session.focus().selection)
            .map(|(p, held)| (held, (p.dx, p.dy)));
        let before = self
            .layout
            .as_ref()
            .map(|layout| self.anim.snapshot(session.board(), layout, lift, now))
            .unwrap_or_default();
        change(session);
        self.relayout();
        if let (Some(session), Some(layout)) = (&self.session, &self.layout) {
            self.anim.diff(&before, session.board(), layout, None, now);
        }
        self.changed();
    }

    /// Hand the session a key, as if typed.
    fn send(&mut self, event: impl Into<Event>) {
        let event = event.into();
        self.act(|s| {
            s.handle(event);
        });
    }

    /// Bring the HTML in step with the session and schedule a frame.
    fn changed(&mut self) {
        let Some(won) = self.session.as_ref().map(Session::is_won) else {
            return;
        };
        if won && !self.won {
            self.won = true;
            self.celebrate();
            self.open(Overlay::Win);
        } else if !won {
            self.won = false;
        }

        let Some(session) = &self.session else { return };
        if let Some(status) = self.document.get_element_by_id("status") {
            status.set_inner_html(&chrome::status(session.rules(), session.board()));
        }
        if let Some(toast) = self.document.get_element_by_id("toast") {
            let (message, tone) = session.message();
            toast.set_text_content(Some(message));
            let class = match (message.is_empty(), tone) {
                (true, _) => "",
                (false, Tone::Warn) => "show warn",
                (false, Tone::Hint) => "show hint",
            };
            toast.set_class_name(class);
        }
        let board = session.board();
        let has_stock = board.first_of_kind(PileKind::Stock).is_some();
        let finishable = session.rules().autoplay(board).is_some();
        self.set_button("undo", session.can_undo(), "");
        self.set_button("redo", session.can_redo(), "");
        self.set_button("deal", has_stock, if has_stock { "" } else { "gone" });
        self.set_button("finish", finishable, if finishable { "ready" } else { "" });
        self.request_frame();
    }

    fn set_button(&self, action: &str, enabled: bool, class: &str) {
        let selector = format!("#tools [data-action=\"{action}\"]");
        if let Ok(Some(button)) = self.document.query_selector(&selector) {
            let _ = if enabled {
                button.remove_attribute("disabled")
            } else {
                button.set_attribute("disabled", "")
            };
            let _ = button.set_attribute("data-state", class);
        }
    }

    fn celebrate(&mut self) {
        let suits = Suit::ALL;
        let rand = js_sys::Math::random;
        self.particles = (0..150)
            .map(|i| Particle {
                x: rand() * self.width,
                y: -rand() * self.height * 0.8 - 20.0,
                vx: (rand() - 0.5) * 160.0,
                vy: rand() * 120.0,
                turn: rand() * 6.3,
                spin: (rand() - 0.5) * 8.0,
                size: 12.0 + rand() * 18.0,
                suit: suits[i % 4],
            })
            .collect();
        self.request_frame();
    }

    // -- overlays -----------------------------------------------------------

    fn open(&mut self, overlay: Overlay) {
        let Some(sheet) = self.document.get_element_by_id("sheet") else {
            return;
        };
        let html = match (overlay, &self.session) {
            (Overlay::Menu, session) => chrome::menu(&self.games, self.chosen, session.is_some()),
            (Overlay::Help, Some(session)) => chrome::help(session.rules()),
            (Overlay::Win, Some(session)) => chrome::win(
                session.rules(),
                session.board().moves(),
                session.board().seed(),
            ),
            _ => return,
        };
        sheet.set_inner_html(&html);
        sheet.set_class_name(match overlay {
            Overlay::Menu => "sheet menu-sheet",
            Overlay::Win => "sheet win-sheet",
            _ => "sheet",
        });
        self.overlay = overlay;
        self.overlay_el.set_hidden(false);
        // Put keyboard focus in the sheet so Tab walks its buttons.
        if let Ok(Some(first)) = sheet.query_selector(".primary") {
            let _ = first.unchecked_into::<HtmlElement>().focus();
        }
    }

    fn close(&mut self) {
        self.overlay = Overlay::None;
        self.overlay_el.set_hidden(true);
        if let Some(active) = self.document.active_element() {
            let _ = active.unchecked_into::<HtmlElement>().blur();
        }
    }

    /// A click on anything carrying a `data-action`.
    fn button(&mut self, event: &web_sys::MouseEvent) {
        let Some(target) = event
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .and_then(|t| t.closest("[data-action]").ok().flatten())
        else {
            // A click on the dimmed backdrop leaves help and the menu.
            let on_backdrop = event
                .target()
                .and_then(|t| t.dyn_into::<Element>().ok())
                .is_some_and(|t| t.id() == "overlay");
            if on_backdrop && self.overlay != Overlay::None && self.session.is_some() {
                self.close();
            }
            return;
        };
        let action = target.get_attribute("data-action").unwrap_or_default();
        let number = |name: &str| {
            target
                .get_attribute(name)
                .and_then(|v| v.parse::<usize>().ok())
        };
        match action.as_str() {
            "choose" => {
                if let Some(game) = number("data-game") {
                    self.chosen = game;
                    self.open(Overlay::Menu);
                }
            }
            "option" => {
                if let (Some(option), Some(value)) = (number("data-option"), number("data-value")) {
                    let game = &mut self.games[self.chosen];
                    let delta = value as i32 - game.selected()[option] as i32;
                    game.configure(option, delta);
                    self.open(Overlay::Menu);
                }
            }
            "play" => {
                self.close();
                self.start(self.chosen, None);
            }
            "close" => self.close(),
            "menu" => self.open_menu(),
            "help" => self.open(Overlay::Help),
            "new" => self.redeal(KeyCode::Char('n')),
            "restart" => {
                self.close();
                self.redeal(KeyCode::Char('r'));
            }
            "undo" => self.send(KeyCode::Char('u')),
            "redo" => self.send(KeyEvent::ctrl(KeyCode::Char('r'))),
            "hint" => self.send(KeyCode::Char('H')),
            "deal" => self.send(KeyCode::Char('d')),
            "finish" => self.send(KeyCode::Char('A')),
            _ => {}
        }
    }

    /// `n` or `r`: a new deal, or this one again, flown in from the stock.
    fn redeal(&mut self, key: KeyCode) {
        if let Some(session) = self.session.as_mut() {
            session.handle(key.into());
        }
        self.dealt();
    }

    // -- keyboard -----------------------------------------------------------

    fn key_down(&mut self, event: &KeyboardEvent) {
        let Some(key) = translate_key(event) else {
            return;
        };
        // Tab and Enter on a focused button belong to the button.
        let on_button = event
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .is_some_and(|t| t.tag_name() == "BUTTON" || t.tag_name() == "A");
        if self.overlay != Overlay::None {
            self.overlay_key(key, on_button, event);
            return;
        }
        event.prevent_default();
        self.show_cursor = true;
        match key.code {
            KeyCode::Char('?') => self.open(Overlay::Help),
            KeyCode::Char('m' | 'q') => self.open_menu(),
            KeyCode::Char('n' | 'r') if !key.ctrl => self.redeal(key.code),
            _ => self.send(Event::Key(key)),
        }
    }

    fn overlay_key(&mut self, key: KeyEvent, on_button: bool, event: &KeyboardEvent) {
        let playing = self.session.is_some();
        match (self.overlay, key.code) {
            (Overlay::Menu, KeyCode::Up | KeyCode::Left) => {
                self.chosen = (self.chosen + self.games.len() - 1) % self.games.len();
                self.open(Overlay::Menu);
            }
            (Overlay::Menu, KeyCode::Down | KeyCode::Right) => {
                self.chosen = (self.chosen + 1) % self.games.len();
                self.open(Overlay::Menu);
            }
            (Overlay::Menu, KeyCode::Enter) if !on_button => {
                self.close();
                self.start(self.chosen, None);
            }
            (Overlay::Win, KeyCode::Char('n')) | (Overlay::Win, KeyCode::Enter) if !on_button => {
                self.redeal(KeyCode::Char('n'))
            }
            (Overlay::Win, KeyCode::Char('u')) => {
                self.close();
                self.send(KeyCode::Char('u'));
            }
            (_, KeyCode::Esc | KeyCode::Char('?' | 'm' | 'q')) if playing => self.close(),
            (_, KeyCode::Tab | KeyCode::BackTab | KeyCode::Enter | KeyCode::Char(' ')) => return,
            _ => return,
        }
        event.prevent_default();
    }

    /// The menu, opened on the game being played.
    fn open_menu(&mut self) {
        if let Some(index) = self
            .session
            .as_ref()
            .and_then(|s| self.games.iter().position(|g| g.name() == s.rules().name()))
        {
            self.chosen = index;
        }
        self.open(Overlay::Menu);
    }

    // -- pointer ------------------------------------------------------------

    fn point(&self, event: &PointerEvent) -> (f64, f64) {
        let rect = self.canvas.get_bounding_client_rect();
        (
            f64::from(event.client_x()) - rect.left(),
            f64::from(event.client_y()) - rect.top(),
        )
    }

    fn pointer_down(&mut self, event: &PointerEvent) {
        event.prevent_default();
        if self.press.is_some() || !event.is_primary() {
            return;
        }
        self.show_cursor = false;
        if event.button() == 2 {
            self.act(Session::put_down);
            return;
        }
        if event.button() != 0 {
            return;
        }
        let (x, y) = self.point(event);
        let now = self.now();
        let Some(session) = &self.session else { return };
        let Some(hit) = self.layout.as_ref().and_then(|l| l.hit(x, y)) else {
            if session.focus().selection.is_some() {
                self.act(Session::put_down);
            }
            return;
        };
        let held = session.focus().selection;
        let stock_like = session
            .rules()
            .activate(session.board(), hit.pile)
            .is_some();

        // A second tap on the card in hand sends it where it goes.
        let double = self.last_tap.take().is_some_and(|tap| {
            now - tap.at < DOUBLE_TAP_MS && tap.pile == hit.pile && tap.card == hit.card
        });
        if double && !stock_like && held.is_some_and(|(p, i)| p == hit.pile && Some(i) == hit.card)
        {
            self.send(KeyCode::Char('f'));
            return;
        }

        let mut deselect_on_tap = false;
        match held {
            Some((pile, index)) if pile == hit.pile => {
                if hit.card == Some(index) {
                    deselect_on_tap = true;
                } else {
                    // Another card of the same pile: take that one instead.
                    self.act(|s| {
                        s.put_down();
                        s.click(hit);
                    });
                }
            }
            _ => self.act(|s| s.click(hit)),
        }

        self.last_tap = Some(Tap {
            at: now,
            pile: hit.pile,
            card: hit.card,
        });
        let picked = self
            .session
            .as_ref()
            .and_then(|s| s.focus().selection)
            .is_some_and(|(pile, _)| pile == hit.pile);
        if picked {
            let _ = self.canvas.set_pointer_capture(event.pointer_id());
            self.press = Some(Press {
                id: event.pointer_id(),
                x0: x,
                y0: y,
                dx: 0.0,
                dy: 0.0,
                dragging: false,
                deselect_on_tap,
            });
        }
    }

    fn pointer_move(&mut self, event: &PointerEvent) {
        let (x, y) = self.point(event);
        let Some(press) = self.press.as_mut().filter(|p| p.id == event.pointer_id()) else {
            return;
        };
        press.dx = x - press.x0;
        press.dy = y - press.y0;
        if !press.dragging && press.dx.hypot(press.dy) > DRAG_SLOP {
            press.dragging = true;
            self.last_tap = None;
        }
        if press.dragging {
            self.request_frame();
        }
    }

    fn pointer_up(&mut self, event: &PointerEvent, cancelled: bool) {
        let Some(press) = self.press.as_ref().filter(|p| p.id == event.pointer_id()) else {
            return;
        };
        let _ = self.canvas.release_pointer_capture(event.pointer_id());
        if !press.dragging {
            let deselect = press.deselect_on_tap;
            self.press = None;
            if deselect && !cancelled {
                self.last_tap = None;
                self.act(Session::put_down);
            }
            return;
        }

        let (dx, dy) = (press.dx, press.dy);
        let target = (!cancelled)
            .then(|| {
                let session = self.session.as_ref()?;
                let (pile, first) = session.focus().selection?;
                let layout = self.layout.as_ref()?;
                let held = layout.card(pile, first)?.offset(dx, dy);
                layout.drop_target(&held).filter(|&to| to != pile)
            })
            .flatten();
        // The snapshot inside `act` still sees the drag, so whatever happens
        // the cards travel on from where the pointer let go of them.
        self.act(|s| {
            if let Some(pile) = target {
                s.click(Hit { pile, card: None });
            }
            s.put_down();
        });
        self.press = None;
        self.request_frame();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_query_names_game_rules_and_seed() {
        let (game, rules, seed) = from_query("?game=klondike&rule=draw%3D1&rule=redeals=3&seed=42");
        assert_eq!(game.as_deref(), Some("klondike"));
        assert_eq!(rules, ["draw=1", "redeals=3"]);
        assert_eq!(seed, Some(42));
        assert_eq!(decode("a%2"), "a%2", "a broken escape is kept");
    }

    #[test]
    fn settings_round_trip_through_configure() {
        for game in rules::games() {
            let again = rules::configure(game.name(), &settings(game.as_ref())).unwrap();
            assert_eq!(again.selected(), game.selected());
        }
    }
}
