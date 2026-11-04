//! What the player did, independent of where they did it.
//!
//! The game and the menu read these rather than a terminal library's events,
//! so the same `Session` runs under crossterm in a terminal and under Ratzilla
//! in a browser. Each frontend translates its own events into these and drops
//! anything that is not a press — key releases and repeats, mouse moves — so
//! nothing here has to ask what kind of key event it was given.
//!
//! The names deliberately mirror crossterm's, so the matches in `app.rs` and
//! `ui/menu.rs` read the same as they always did.

/// One thing the player did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    Key(KeyEvent),
    Mouse(MouseEvent),
}

/// A key press.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KeyEvent {
    pub code: KeyCode,
    /// Only Ctrl is ever read: `ctrl-c` quits and `ctrl-r` redoes. Shift is
    /// already folded into the character (`H`, `A`, `?`).
    pub ctrl: bool,
}

impl KeyEvent {
    pub fn new(code: KeyCode) -> Self {
        Self { code, ctrl: false }
    }

    pub fn ctrl(code: KeyCode) -> Self {
        Self { code, ctrl: true }
    }
}

/// The keys anything listens for. Everything else is dropped by the frontend.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyCode {
    Char(char),
    Up,
    Down,
    Left,
    Right,
    Enter,
    Esc,
    Tab,
    BackTab,
}

/// A mouse press or scroll, at a cell of the drawn screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MouseEvent {
    pub kind: MouseEventKind,
    pub column: u16,
    pub row: u16,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MouseEventKind {
    Down(MouseButton),
    ScrollUp,
    ScrollDown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

impl From<KeyEvent> for Event {
    fn from(key: KeyEvent) -> Self {
        Self::Key(key)
    }
}

impl From<KeyCode> for Event {
    fn from(code: KeyCode) -> Self {
        Self::Key(KeyEvent::new(code))
    }
}
