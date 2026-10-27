# solitaire

[![ci](https://github.com/Forsbakk/solitaire-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/Forsbakk/solitaire-tui/actions/workflows/ci.yml)

Solitaire in the terminal, built with [ratatui](https://ratatui.rs). Eight games
so far — Klondike, FreeCell, Scorpion, Yukon, Russian Solitaire, Spiderette,
Spider and Mod3 — chosen from a menu at startup.

```
╭ Games ─────────────╮╭ Klondike ────────────────────────────────────────╮
│ ▸ Klondike         ││ Build the tableau down in alternating colour,    │
│   FreeCell         ││ moving ordered runs as a unit. Only a King may   │
│   Scorpion         ││ take an empty pile.                              │
│   Yukon            ││                                                  │
│   Russian          ││ Turn the stock for more cards, and build the     │
│   Spiderette       ││ four foundations up in suit from the Ace. Win by │
│   Spider           ││ sending all 52 cards home.                       │
│   Mod3             ││                                                  │
│                    ││ Rules                                            │
│                    ││   Draw      ‹ 3 ›                                │
│                    ││   Redeals   ‹ unlimited ›                        │
╰────────────────────╯╰──────────────────────────────────────────────────╯
                 ↑↓ game   tab rules   enter play   q quit
```

Klondike after one draw of three — the stock on the left, the waste fanned out
beside it, and the four foundations waiting:

```
┌────┐ ┌───┌───┌────┐┌────┐ ┌────┐ ┌────┐ ┌────┐
│▞▞▞▞│ │10♠│ J♠│ 9♠ ││    │ │    │ │    │ │    │
└────┘ └───└───└────┘└────┘ └────┘ └────┘ └────┘
 [1]     2      3      4      5      6      7
╔════╗ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐
║ 7♠ ║ │▞▞▞▞│ │▞▞▞▞│ │▞▞▞▞│ │▞▞▞▞│ │▞▞▞▞│ │▞▞▞▞│
╚════╝ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤
       │ K♠ │ │▞▞▞▞│ │▞▞▞▞│ │▞▞▞▞│ │▞▞▞▞│ │▞▞▞▞│
       └────┘ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤
              │ A♠ │ │▞▞▞▞│ │▞▞▞▞│ │▞▞▞▞│ │▞▞▞▞│
              └────┘ ├────┤ ├────┤ ├────┤ ├────┤
                     │ 5♠ │ │▞▞▞▞│ │▞▞▞▞│ │▞▞▞▞│
                     └────┘ ├────┤ ├────┤ ├────┤
                            │ 6♦ │ │▞▞▞▞│ │▞▞▞▞│
                            └────┘ ├────┤ ├────┤
                                   │ 7♥ │ │▞▞▞▞│
                                   └────┘ ├────┤
                                          │10♣ │
                                          └────┘
```

## Installing

Prebuilt binaries for Linux, Windows and macOS are on the
[releases page](https://github.com/Forsbakk/solitaire-tui/releases). There is
nothing to install alongside them — download, extract, run. The Linux builds are
statically linked, so they do not care which distro they land on.

On Arch, from the AUR — `solitaire-tui` builds from source, `solitaire-tui-bin`
drops in the prebuilt binary:

```sh
paru -S solitaire-tui-bin
```

On macOS or Linux, with Homebrew:

```sh
brew install Forsbakk/tap/solitaire-tui
```

Or with Rust 1.88 or newer, from crates.io:

```sh
cargo install solitaire-tui
```

Any of them installs a binary called `solitaire-tui`. Note that `cargo install`
puts it in `~/.cargo/bin`, which is only on your `PATH` if you put it there. To
work on the game instead, clone the repository and use `cargo run` as below.

Any terminal with 256 colours and a font covering `♠ ♥ ♦ ♣` will do. On Windows
use Windows Terminal rather than the legacy console host, which cannot render
the box-drawing and suit characters the board is made of.

## Playing

```sh
cargo run --release                                   # pick a game from the menu
cargo run --release -- --game yukon                   # skip the menu
cargo run --release -- --game klondike --rule draw=1  # and set its rules
cargo run --release -- --seed 42                      # a specific deal, repeatable
cargo run --release -- --list                         # what can be played
cargo run --release -- --no-mouse
```

The menu's right-hand pane shows the highlighted game's rules and any of them you
can change before dealing. `tab` moves into that pane, `←` and `→` change the
setting under the cursor, and `enter` deals. The same settings are available from
the command line as `--rule name=value`; `--list` prints them.

`m` during a game takes you back to the menu.

## The games

### Klondike

Seven piles dealt 1 to 7 cards, only the last of each face up; the other 24 cards
are the stock.

* Build the tableau **down in alternating colour** — a red 9 onto a black 10.
* A run of cards moves as a unit only if it is **already in order**. Any single
  face-up card moves on its own.
* Only a **King** may move to an empty pile.
* Press `d` (or `space` on the stock) to turn cards onto the waste. On a draw of
  three all three are shown, fanned out so every rank and suit stays readable,
  though only the one on top is in play — clicking any of them picks up that
  top card. When the stock runs out, the same key puts the waste back, as often
  as the **Redeals** rule allows.
* The four **foundations** build up in suit from the Ace. `f` sends a card to
  the one that will take it — the card you are holding, or the one under the
  cursor. There is never a choice to make: at most one foundation accepts any
  given card, and an Ace simply takes the first free one. Where no foundation
  will have it, `f` falls through to the tableau and plays it onto the only
  column that takes it.
* Win by sending all 52 cards home. Once nothing is face down and the stock is
  spent, `A` plays out the rest.

Rules you can change: **Draw** (3 or 1) and **Redeals** (unlimited, 3, or none).

### FreeCell

Eight columns — four of seven cards and four of six — every one of them face up.
There is no stock, and nothing is ever hidden, so `--seed 42` names a puzzle
rather than a gamble, and nearly every deal can be won.

```
┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐
│free│ │free│ │free│ │free│ │    │ │    │ │    │ │    │
└────┘ └────┘ └────┘ └────┘ └────┘ └────┘ └────┘ └────┘
 [1]     2      3      4      5      6      7      8
┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐
│ 7♠ │ │ K♥ │ │ 6♦ │ │ 2♠ │ │ 4♦ │ │ 7♣ │ │ 5♦ │ │ 3♠ │
├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤
│ 3♦ │ │10♥ │ │ 5♥ │ │ 8♦ │ │ 3♥ │ │ 3♣ │ │ 6♠ │ │ 9♦ │
├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤
│ K♠ │ │ 5♠ │ │ 2♥ │ │ 8♠ │ │10♦ │ │ 8♥ │ │ Q♣ │ │ Q♦ │
├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤
│ A♣ │ │ 8♣ │ │ 5♣ │ │ 4♥ │ │ J♣ │ │ Q♥ │ │ 9♥ │ │ 9♠ │
├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤
│ 2♣ │ │ J♦ │ │ 6♥ │ │ 7♦ │ │ 4♠ │ │ 9♣ │ │ J♥ │ │ J♠ │
├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤
│ A♠ │ │ 2♦ │ │ A♥ │ │ K♣ │ │ 6♣ │ │ K♦ │ │ Q♠ │ │10♠ │
╔════╗ ├────┤ ├────┤ ├────┤ └────┘ └────┘ └────┘ └────┘
║ A♦ ║ │ 4♣ │ │ 7♥ │ │10♣ │
╚════╝ └────┘ └────┘ └────┘
```

* Build the tableau **down in alternating colour** — a red 9 onto a black 10.
* **Any** card may take an empty column. This is the one place the game is
  kinder than Klondike, and empty columns are the thing worth playing for.
* The four boxes marked `free` are the **free cells**. Each holds a single card,
  which stays in play and can be brought back out at any time.
* A run in order **moves as a unit**, as far as there is room to shuffle it
  across: one card, doubled for every free cell and every empty column, not
  counting the column it is landing on. Anything longer is refused — but it can
  still be moved a card at a time, which is all the refusal really means.
* The four **foundations** build up in suit from the Ace, and a card may be
  taken back off one. `f` plays the card you are holding, or the one under the
  cursor, wherever it has no choice but to go — home if a foundation will take
  it, otherwise onto the only column that will. It works on a card sitting in a
  cell too, which is how a parked card gets back out; what it will never do is
  put one *into* a cell.
* Win by sending all 52 cards home. Once every column runs down in rank, the
  rest is bookkeeping and `A` plays it out.

### Scorpion

Seven columns of seven cards. The first four columns start with three cards face
down; the rest are face up. Three cards are held back as the stock.

* Pick up any **face-up card together with every card below it**. Those cards
  travel as a unit and do **not** need to be in order.
* Drop them on a card of the **same suit, exactly one rank higher** — a 9♥ onto
  a 10♥.
* Only a **King** (with whatever it is carrying) may move to an **empty column**.
* A face-down card left exposed by a move turns face up.
* Press `d` once per game to deal the three stock cards, one onto each of the
  first three columns.
* Scorpion builds in suit, so a card that can move at all can only move to one
  place. `f` plays it there, which is most of the game's bookkeeping in one key.
* You win when all four suits sit in the tableau as complete King-to-Ace runs.

Not every deal is winnable. `u` undoes as far back as you like, and `r` deals the
same cards again from the start.

### Yukon

Scorpion's lift played onto Klondike's foundations. Seven piles holding 1, 6, 7,
8, 9, 10 and 11 cards, with five face up on each and the rest buried — pile *n*
hides *n* cards. There is **no stock**: every card is on the table from the
first move.

* Pick up any **face-up card together with every card below it**, in any order —
  the tail does not have to be a run, which is what separates this from
  Klondike.
* Drop it on the **opposite colour, one rank higher** — a red 9 onto a black 10.
* Only a **King** may move to an empty pile.
* A face-down card left exposed by a move turns face up.
* The four **foundations** build up in suit from the Ace. `f` sends a card home
  — or, when no foundation will have it, onto the only column that takes it —
  and a card may be taken back off a foundation when you need it. `A` plays out
  the rest once nothing is face down.
* Win by sending all 52 cards home.

### Russian Solitaire

Yukon in every respect but one: build on the **same suit** rather than the
opposite colour. The deal, the free-tail lift, the foundations and the empty-pile
rule are all Yukon's.

That one word takes most of your legal moves away, so expect to lose far more
often than you win — which is the point of the game.

### Spiderette

Single-deck Spider. Seven columns dealt 1 to 7 cards, only the last of each face
up; the other 24 cards are the stock. This is the first game here whose tension
comes from the stock rather than the tableau.

* Build the tableau **down regardless of suit** — any 8 goes on any 9.
* Only a **run of one suit** may be lifted as a unit. Everything else moves one
  card at a time, which is why building in suit is worth the trouble.
* **Any** card may take an empty column.
* `d` deals **one card onto every column at once**, and refuses while any column
  stands empty. So an empty column is both the room you need to work and the
  thing you must give up to deal — that is the whole game.
* The last deal is short: 24 cards over seven columns is three full rounds and a
  round of three, which covers the leftmost three columns.
* A complete **King-to-Ace run in one suit files itself**, leaving the tableau
  for the foundations above the board. It happens as part of the move that
  finished it, so one `u` takes the whole thing back.
* Win by filing all four runs.

There is no `A`: a Spider board is never decided until it is won. `f` needs no
foundation to be useful here — it plays a card or a suited run onto the only
column that takes it, and prefers a card to an empty column, which is the one
thing you cannot get back.

### Spider

Spiderette on two decks and ten columns — four columns of six and six of five,
with 50 cards left over for five deals. Eight runs to file instead of four.
Every other rule is Spiderette's.

Rules you can change: **Suits** (1, 2 or 4), defaulting to 1. One-suit Spider is
the most forgiving game here and a good first patience in a terminal. The suit
count repaints one deal rather than dealing a different one, so `--seed 42` is
the same board at all three difficulties.

### Mod3

Two decks, 32 piles in a 4×8 grid — three rows of foundations above a working
row — and a talon. The rules follow KDE's KPatience.

* The three **foundation rows** each build in suit in steps of three:
  * **Row 1** — 2 → 5 → 8 → J, same suit per pile
  * **Row 2** — 3 → 6 → 9 → Q, same suit per pile
  * **Row 3** — 4 → 7 → 10 → K, same suit per pile
* An empty foundation spot takes only its row's base card (2, 3 or 4). A
  card dealt into the wrong spot is **misplaced**: nothing can be built on it,
  and it is there to be moved off — to its own row, or into the working row.
  `H` suggests those moves first.
* Each foundation pile fans downwards and its row grows with its tallest stack, so a whole stack can be read at a glance.
  As in KPatience, the Aces sit beside the first row and the talon beside the
  working row; the board needs a 62×29 terminal.
* **Aces** go to a separate pile and sit there for good. All eight must be
  placed to win.
* The **working row** only accepts a card onto an **empty pile**. A pile that
  empties is refilled from the talon straight away; once the talon is spent,
  it stays open as a free cell.
* Press `d` to deal one card from the talon onto **all eight** working-row
  piles at once, even non-empty ones.
* The digits `1`…`8` name columns: holding a card, a digit drops it on
  whichever pile of that column takes it.
* `f` sends a card to its foundation or, failing that, onto the first empty
  working-row pile. Once only foundation moves remain, `A` plays out the rest.
* Win by completing all 24 foundation sequences and placing all 8 Aces.

## Keys

| Key | |
| --- | --- |
| `← →` `h l` | move between piles |
| `↑ ↓` `k j` | move up and down a pile, and between rows |
| `1`…`9` `0` | jump to a tableau column, or drop a held card there — `0` is the tenth, on Spider's board |
| `space` `enter` | pick up the card under the cursor, or drop what you hold |
| `esc` | put a held card back |
| `d` | turn or deal the stock (Klondike, Scorpion, Spiderette, Spider) |
| `f` | play a card where it has no choice but to go — its foundation, or the only column that takes it |
| `A` | play out a decided game (Klondike, FreeCell, Yukon, Russian) |
| `H` | suggest a move; press again for the next one |
| `u` / `ctrl-r` | undo / redo |
| `n` / `r` | new game / replay this deal |
| `m` | back to the menu |
| `?` | help |
| `q` | quit |

Mouse: click a card to pick it up, click a pile to drop. Clicking the stock turns
it. Right-click cancels; the scroll wheel walks up and down a pile.

## Finding your place

Cards overlap, so each border row is shared between the card above it and the
card below it. Whichever of the two is highlighted claims that row, which is
what lets a marked card be closed on all four sides even when it sits in the
middle of a pile.

Three things mark the board, and each uses both a colour and a distinct border
weight, so they stay legible where colour does not survive:

| | |
| --- | --- |
| `[5]` above a pile, `▶` in the gutter, cyan `║` frame | where the cursor is |
| yellow `┃` frame around a run of cards | the cards you are carrying |
| green `║` frame | where the current hint says to drop |

The `▶` stays beside the cursor even once you have picked a card up, so the frame
tells you *what* you are holding and the caret still tells you *where* you are.

## Forced moves

A card often has exactly one place it can go, and finding which column that is
costs more attention than the move is worth. `f` plays it there — the card you
are holding, or the one under the cursor, run and all.

Foundations first, since at most one can take a given card, then the columns. Two
columns that would both take it is a real choice, so `f` says so and leaves the
card alone. An empty column is the last resort, and several of them count as one:
they are interchangeable, so `f` takes the leftmost. The status line offers the
key whenever it would work.

FreeCell's free cells are not among the places it will go. Parking a card is a
choice rather than a forced move, and it is the one the game is really about.

## Hints

`H` suggests a move, puts the cursor on the card to lift, and frames the card it
should land on. Accepting it is then just `space` and the target pile. Press `H`
again to cycle through the alternatives, best first — the count in the status
line (`hint 2/5`) tells you how many there are.

The ranking is a heuristic, not a solver. Each game supplies its own: they all
prefer moves that turn a card face up, the foundation games then favour sending
cards home, Klondike also favours clearing the waste, FreeCell favours emptying
a cell or a column and dislikes filling either, the Spider games favour
finishing a run and landing in suit and dislike splitting a suited run apart, and
none of them will offer
the legal-but-useless move of sliding a whole pile into an empty one. No game
here can be played perfectly without searching, so a hint is a reasonable move
rather than a winning one.

## Layout on small terminals

Piles get long, so the board picks a density each frame:

* **Roomy** — two rows per card, the usual look above.
* **Tight** — one row per card under a single border, when the tallest pile
  will not otherwise fit.
* **Elided** — for piles still too tall, the middle collapses to a `⋯N`
  marker. The exposed card is always shown, since it is the only drop target,
  and so is the card under the cursor.

Below the size a game's board needs, the board is replaced by a "terminal too
small" note naming that size.

## Adding a game

A variant is one file in `src/rules/` plus a line in `rules::games()`. It
implements `Rules`: how to deal, where its piles sit, what a legal move is, and
when the game is won. Most of the trait is defaulted — the default `suggestions`
walks every legal move and ranks it with the game's own `rank`.

Everything else is shared. `src/board.rs` holds the piles and the undoable
actions, `src/ui/layout.rs` places any board the spec describes, and `src/app.rs`
drives the cursor, selection and undo for whatever game it is handed.

[`docs/adding-a-game.md`](docs/adding-a-game.md) is the full guide — a
walkthrough building Yukon from nothing, the trait method by method, and what to
test; `src/rules/yukon.rs` is that walkthrough as it actually shipped. Variants
that could come next, and what each would cost, are sketched in
[`docs/future-games.md`](docs/future-games.md).

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

The rules live in `src/rules/` and are fully testable without a terminal; each
game's suite includes an undo round-trip that plays a few hundred legal moves and
asserts the board returns to the exact opening deal.

`src/ui/layout.rs` decides where every card sits and is the single source of
truth for the board's shape — the renderer draws from it and mouse clicks are
resolved against it, so a click can never disagree with what is on screen.

The shuffle is a small built-in splitmix64 rather than the `rand` crate, so a
given `--seed` deals the same game permanently instead of changing under a
dependency upgrade.

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you state otherwise, any contribution you intentionally
submit for inclusion in this crate shall be dual-licensed as above, without any
additional terms or conditions.
