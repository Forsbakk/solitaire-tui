# solitaire

Solitaire in the terminal, built with [ratatui](https://ratatui.rs). Two games so
far — Klondike and Scorpion — chosen from a menu at startup.

```
╭ Games ─────────────╮╭ Klondike ────────────────────────────────────────╮
│ ▸ Klondike         ││ Build the tableau down in alternating colour,    │
│   Scorpion         ││ moving ordered runs as a unit. Only a King may   │
│                    ││ take an empty pile.                              │
│                    ││                                                  │
│                    ││ Turn the stock for more cards, and build the     │
│                    ││ four foundations up in suit from the Ace. Win by │
│                    ││ sending all 52 cards home.                       │
│                    ││                                                  │
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

## Playing

```sh
cargo run --release                                   # pick a game from the menu
cargo run --release -- --game klondike                # skip the menu
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
* The four **foundations** build up in suit from the Ace. `f` sends the card
  under the cursor to one if it fits.
* Win by sending all 52 cards home. Once nothing is face down and the stock is
  spent, `A` plays out the rest.

Rules you can change: **Draw** (3 or 1) and **Redeals** (unlimited, 3, or none).

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
* You win when all four suits sit in the tableau as complete King-to-Ace runs.

Not every deal is winnable. `u` undoes as far back as you like, and `r` deals the
same cards again from the start.

## Keys

| Key | |
| --- | --- |
| `← →` `h l` | move between piles |
| `↑ ↓` `k j` | move up and down a pile, and between rows |
| `1`…`9` | jump to a tableau pile, or drop a held card there |
| `space` `enter` | pick up the card under the cursor, or drop what you hold |
| `esc` | put a held card back |
| `d` | turn or deal the stock |
| `f` | send a card to a foundation (Klondike) |
| `A` | play out a decided game (Klondike) |
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

## Hints

`H` suggests a move, puts the cursor on the card to lift, and frames the card it
should land on. Accepting it is then just `space` and the target pile. Press `H`
again to cycle through the alternatives, best first — the count in the status
line (`hint 2/5`) tells you how many there are.

The ranking is a heuristic, not a solver. Each game supplies its own: both prefer
moves that turn a card face up, Klondike then favours sending cards home and
clearing the waste, and neither will offer the legal-but-useless move of sliding
a whole pile into an empty one. Neither game can be played perfectly without
searching, so a hint is a reasonable move rather than a winning one.

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
test. Variants that could come next, and what each would cost, are sketched in
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
