# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```sh
cargo run --release                                   # menu, then a game
cargo run --release -- --game scorpion --seed 42      # skip the menu, fixed deal
cargo run --release -- --game klondike --rule draw=1  # set a game's rules
cargo run --release -- --game spider --rule suits=1   # the ten-column, two-deck board
cargo run --release -- --list                         # games and their rules

cargo test                          # all 213 tests
cargo test undo_round_trips         # one test, by substring of its name
cargo test -- --nocapture           # many render tests print the board they asserted on

cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

The directory is `solitaire-tui`; the crate and binary are `solitaire`. There is
no integration test directory — every test is a `#[cfg(test)] mod tests` inside the
module it covers.

## Architecture

```
main.rs      CLI, terminal, and the menu ⇄ game loop
card.rs      Card, Suit, shuffled_deck/shuffled_decks — no game knowledge
board.rs     Pile, Board, Move, Action/Step, BoardSpec — state, no rules
rules/       one file per game + the Rules trait and registry
ui/layout.rs where cards sit; purely geometric
ui/render.rs drawing; reads a layout and a Board
ui/menu.rs   the game selector
app.rs       Session: cursor, selection, undo stacks, input
```

Dependencies point one way: `app` → {`rules`, `ui`} → `board` → `card`. `rules`
never imports `ui`, which is why `BoardSpec` lives in `board.rs` even though only
the layout consumes it.

### Adding a game is one file plus a registry line

**Read [`docs/adding-a-game.md`](docs/adding-a-game.md) before adding or changing a
game mode.** It is the detailed guide: a full walkthrough building Yukon, the
`Rules` trait method by method, the board and `Action`/`Step` API, how `BoardSpec`
placement works, rule options, what to test, and the traps. Keep it in step with
any change to the `Rules` contract.

The short version: implement `Rules` (`src/rules/mod.rs`) and add it to
`rules::games()`. Required: `name`, `blurb`, `help`, `deal`, `spec`, `check`,
`is_won`. Everything else is defaulted — notably `suggestions`, which enumerates
every legal move and sorts by the game's `rank`. Override `activate` (what pressing
the stock does), `stock_refusal` (why it would not), `send_home`, `autoplay`,
`is_useful`, `options`/`configure`/`selected` only as needed.

`Scorpion` is the minimal example; `Klondike` uses nearly every hook; `Yukon` is a
foundation game with no stock, and `Russian` is Yukon with one predicate swapped.
`Spider` is the only game whose actions do more than move cards — a finished
King-to-Ace run is filed by extra `Step`s appended to the same `Action` — and
`Spiderette` is Spider with a different `Setup`.

`russian.rs` and `spiderette.rs` are the two modules that reach into another game.
`yukon.rs` exposes its deal, spec, `check`, foundations and heuristic as
`pub(super)` functions taking a `Build`, and `spider.rs` exposes its half taking a
`Setup` — decks, suits, and the shape of the deal. In both cases the divergence is
one named `Copy` type passed as one argument; anything more divergent gets its own
file. That `spider::check` needs no `Setup` at all is what says the second split is
honest.

### `Action` is universal, and only reversible in exact order

`Board` holds no history. An `Action` is a `Vec<Step>` applied in order and undone
in reverse; `Session` (`app.rs`) owns the `history`/`future` stacks. Keeping the
action type shared rather than per-game is what lets `app.rs` be non-generic over
`Box<dyn Rules>`.

A `Step` records what a transfer did to the cards' *orientation and order* —
`was_face_up`, `face_up`, `reverse`, `flip_source`, `recycle` — so `Board::undo`
needs no rules knowledge. `Action::moving` computes `flip_source` from the board
and is the right constructor for almost every move; build `Step`s by hand only for
stock deals, draws and recycles.

These assume the board is in the state the action left behind. Applying them out of
order corrupts the board silently. Each game has an `undo_round_trips_a_played_game`
test that plays a few hundred moves and asserts the board equals the opening deal —
if you add a new kind of `Step`, make sure that walk exercises it.

`Board::recycles()` exists solely so a redeal limit survives undo; it moves with
the `Step.recycle` flag rather than being recomputed.

The rules may also *append* steps to an action. `spider.rs` files a finished
King-to-Ace run as part of the move that finished it, so the run leaving play is
one undo and `app.rs` never learns what a run is. Detection needs the post-move
board, so the extra steps are computed against a scratch clone — each one's flags
against the state it will actually meet, which is what `undo` assumes. Both
`action_for` and `activate` have to go through it; dealing a card finishes runs
too.

### The layout is the single source of truth for board shape

A game declares a `BoardSpec` of `Spot`s — a pile, a row, a slot, and a `Fan`:
`Down` (cards overlap downwards), `None` (only the exposed card shows), or
`Right(n)` (the last `n` overlap rightwards, Klondike's waste).
`layout::compute` places fixed rows at `CARD_H` each and gives the remaining
height to the fanned row. `render.rs` draws *from* the resulting `BoardLayout`,
and mouse clicks resolve *against* it via `hit()`. `Session` stores the layout
from the last frame so clicks match what was actually drawn.

**This invariant is the thing most worth protecting.** Two tests guard it, both
deliberately indirect:

- `assert_slots_round_trip` in `ui/layout.rs` — every row of every visible card
  must resolve back to that card through `hit()`, across all piles.
- `clicking_a_card_picks_up_the_card_that_was_drawn_there` in `app.rs` — finds
  `K♦` by scanning the *rendered text*, not by asking the layout, then clicks it.

Never compute a position in `render.rs`. Two piles can share an x range in
different rows (Klondike's foundations sit above tableau piles), so `hit()`
resolves on both axes via `claim_bottom`; a fanned pile claims every row down to
the bottom of the board so clicking below a short pile still targets it.

A `Fan::None`/`Fan::Right` pile's single slot carries the *exposed* card's index,
not zero — otherwise clicking the waste would grab the bottom card. Cards that are
drawn but not addressable live in `PileLayout::spread`; only `slots` decides what
`hit()` can name, which is how a fanned waste shows three cards but plays one.

The caret (`▶`) is passed into `draw_face` rather than derived from the accent:
picking a card up turns its accent from `Cursor` to `Selected`, and deriving the
caret would make the cursor vanish at the moment the player most needs it.
`the_cursor_stays_findable_once_a_card_is_picked_up` guards this.

**The cursor can sit on a face-down card.** `Session::cursor_card` names the card
the cursor is on, not a card that can be picked up — the stock is entirely face
down and still has to look focused. `clamp_cursor` therefore falls back to the
exposed card when a pile has nothing face up; blanking it to `None` makes the
accent fall through to `Plain` and the pile look unfocused right after a draw.
Guarded by `the_stock_stays_marked_after_it_is_drawn_from`. Whether a card can
actually be *taken* is `Pile::is_face_up`, checked separately in `activate`.

### Slot ownership and shared border rows

Cards overlap, so a border row is shared. Each card owns its top border row plus
its face row; the last card also owns the closing border. `Accent::weight()` in
`render.rs` decides which of two cards wins a shared row — that is what lets a
highlighted card mid-pile be closed on all four sides, asserted by
`a_highlighted_card_is_boxed_in_even_mid_pile`.

Three accents, each with a colour *and* a distinct border weight so they survive
without colour: cursor (cyan `║`), selection (yellow `┃`), hint target (green `║`).

### Density is chosen per frame, for the whole board

`Roomy` (two rows per card), `Tight` (one row), or `Tight` plus an elision that
collapses the middle to `⋯N`. The exposed card always survives elision — it is the
only legal drop target. `pins` (from `Focus::pins`) names one more card per pile
that must also survive: the cursor's card, or the head of a held run.

### Pile invariant

Face-down cards are always a *prefix* of a `Pile`, tracked by one `face_down`
count. `Board::apply_step` clamps that count when cards leave and only lets
face-down cards land on an already-entirely-face-down pile (debug-asserted).
Every "is this face up" question goes through `Pile::is_face_up`.

### Determinism is a product guarantee

`shuffled_deck` uses a hand-rolled splitmix64 rather than `rand` so a given
`--seed` deals the same game permanently. Do not swap it for `rand`.

`shuffled_decks(seed, decks)` is written separately rather than as a
generalisation, so that `shuffled_decks(seed, 1) == shuffled_deck(seed)` is a real
assertion between two pieces of code that could disagree. That test is what says
two-deck games arrived without moving a single existing deal; keep both functions
rather than folding one into the other.

Tests depend on the concrete deal for a seed — `the_deal_is_pinned_to_its_seed` in
each rules module, plus app tests that pick `K♦`, `9♣`, `10♣`, `Q♦` out of
Scorpion's seed 42 (which is also the README's sample board). Changing the shuffle
or a deal order breaks them, and that breakage is the guarantee working.

## Conventions

Every module opens with a `//!` header; each rules module's enumerates that game's
rules. Comments explain *why* — why `pins` exists, why a recycle reverses, why the
labels anchor to the tableau row. Tests are named as sentences
(`only_a_king_may_take_an_empty_pile`, `a_recycled_stock_deals_exactly_the_same_cards_again`).
Player-facing strings live next to the code that raises them: `MoveError::message()`,
each game's `help`/`blurb`, and the shared help constants in `render.rs`.

Keep README.md in step with any rule, key binding or layout change;
`docs/adding-a-game.md` in step with the `Rules` contract; and
`docs/future-games.md` in step with what the shared code now provides.
