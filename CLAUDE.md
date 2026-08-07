# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```sh
cargo run --release                 # a fresh deal
cargo run --release -- --seed 42    # a specific deal, repeatable
cargo run --release -- --no-mouse   # keyboard only

cargo test                          # all 58 tests
cargo test the_cursor_starts        # one test, by substring of its name
cargo test -- --nocapture           # several render tests print the board they asserted on

cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

The directory is `solitaire-tui` but the crate is `scorpion` and plays one game. There is
no integration test directory — every test is a `#[cfg(test)] mod tests` inside the module
it covers.

## Architecture

Four modules, and the dependency arrows all point one way: `main` → `app` → {`game`, `ui`},
with `card` underneath everything.

- `card.rs` — `Card`, `Suit`, and the shuffle. No game knowledge.
- `game.rs` — the rules. Pure, terminal-free, and fully testable on its own.
- `ui/layout.rs` — where cards sit. Purely geometric; knows column *lengths*, not cards.
- `ui/render.rs` — drawing. Reads a layout and a `Game`, writes to a `Buffer`.
- `app.rs` — cursor, selection, undo stacks, key and mouse handling.

### The layout is the single source of truth for board shape

`ui/layout.rs::compute()` returns a `BoardLayout`. The renderer draws *from* it, and mouse
clicks are resolved *against* it via `BoardLayout::hit()` — so a click can never disagree
with what is on screen. `App` stores the layout from the last frame in `self.layout`
(`src/app.rs`) precisely so clicks resolve against what was actually drawn rather than
against a freshly recomputed guess.

**This invariant is the thing most worth protecting.** Two tests guard it, and both are
deliberately indirect:

- `assert_slots_round_trip` in `ui/layout.rs` — every row of every visible card must
  resolve back to that card through `hit()`, and no two cards may claim the same row.
- `clicking_a_card_picks_up_the_card_that_was_drawn_there` in `app.rs` — finds `K♦` by
  scanning the *rendered text*, not by asking the layout, then clicks those coordinates.

If you change how cards are positioned, change `layout.rs` and let render and hit-testing
follow. Never compute a position in `render.rs`.

### Slot ownership and shared border rows

Cards overlap, so a border row is shared between the card above and the card below. Each
card owns its own top border row plus its face row; the last card in a column also owns the
closing border (see `Slot` and `column_layout` in `ui/layout.rs`). `Accent::weight()` in
`render.rs` decides which of two cards wins a shared row. That is what lets a highlighted
card mid-column be closed on all four sides instead of open-bottomed — `a_highlighted_card_is_boxed_in_even_mid_column` asserts exactly this.

Three accents, each with both a colour *and* a distinct border weight so they survive
without colour: cursor (cyan, `║`), selection (yellow, `┃`), hint target (green, `║`).

### Density is chosen per frame, for the whole board

`compute()` picks one `Density` for all seven columns so they stay aligned: `Roomy` (two
rows per card), `Tight` (one row per card), and for columns still too tall, `Tight` plus an
elision that collapses the middle to a `⋯N` marker. The exposed card always survives
elision — it is the only legal drop target. `pins` (built by `Focus::pins()` in
`render.rs`) names one more card per column that must also survive: the cursor's card, or
the head of a held tail.

### Undo lives in `app`, not `game`

`Game` holds no history. `Action` (`game.rs`) is a compact record of a completed change,
and `App` owns the `history` and `future` `Vec<Action>` stacks. `Game::undo(action)` and
`Game::redo(action)` are inverse operations that **assume the board is in the state that
action left it in / preceded it** — actions carry only enough information to be reversed in
exact order (`Action::Move` records `count` and whether it `flipped` a card, not the cards
themselves). Applying them out of order corrupts the board silently. A test plays a few
hundred legal moves and asserts the board returns to the exact opening deal.

### Column invariant

Face-down cards are always a *prefix* of a `Column`, tracked by a single `face_down` count,
because cards are only ever added face up. Every "is this face up" question goes through
`Column::is_face_up`. Preserve this if you touch dealing or moving.

### Determinism is a product guarantee, not an implementation detail

`shuffled_deck` uses a hand-rolled splitmix64 rather than the `rand` crate so that a given
`--seed` deals the same game permanently, instead of changing under a dependency upgrade.
Do not swap it for `rand`.

Several tests depend on the concrete deal for a seed — seed 42 puts `K♦`, `9♣`, `10♣` and
`Q♦` where those tests expect them. Changing the shuffle will break them, and that breakage
is the guarantee working as intended, not a flaky test.

### Hints are a heuristic, not a solver

`Game::suggestions()` returns every legal move that changes the position, sorted by
`value_of()`: prefer flipping a card face up, then emptying a column, penalise spending an
empty column, then prefer disturbing fewer cards. Moves that slide a whole column into an
empty one are excluded — legal but useless. The sort is stable over an already deterministic
list so repeated calls in the same position offer hints in the same order, which is what
makes `H`-to-cycle work.

## Adding a second game

There is no `Game` trait, no variant enum and no registry — the crate is single-game
throughout. `COLUMNS` is a `const` consumed as an *array length* in three places
(`Game::columns`, `compute()`'s `&[usize; COLUMNS]` and `&[Option<usize>; COLUMNS]`
parameters, and `Focus::pins`), and `'1'..='7'` is hardcoded at `src/app.rs:180`. Candidate
variants and what each would cost are worked out in `docs/future-games.md`.

## Conventions

Every module opens with a `//!` header explaining what it is for; `game.rs`'s enumerates
the rules. Comments explain *why*, not what — the splitmix64 choice, why `pins` exists, why
the redo path recomputes an index. Tests are named as sentences
(`only_a_king_may_take_an_empty_column`, `a_run_with_a_face_down_card_is_not_a_win`).
Player-facing strings live next to the code that raises them, as `MoveError::message()` and
the `HELP` constant in `render.rs`. Keep README.md in step with any rule, key binding or
layout change.
