# scorpion

Scorpion solitaire in the terminal, built with [ratatui](https://ratatui.rs).

```
  1      2      3      4     [5]     6      7
┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐ ┌────┐
│▞▞▞▞│ │▞▞▞▞│ │▞▞▞▞│ │▞▞▞▞│ │ 4♦ │ │ 3♣ │ │ Q♣ │
├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤
│▞▞▞▞│ │▞▞▞▞│ │▞▞▞▞│ │▞▞▞▞│ │ 3♥ │ │ 8♥ │ │ 9♥ │
├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤ ├────┤
│▞▞▞▞│ │▞▞▞▞│ │▞▞▞▞│ │▞▞▞▞│ │10♦ │ │ Q♥ │ │ J♥ │
├────┤ ├────┤ ├────┤ ├────┤ ┏━━━━┓ ├────┤ ├────┤
│ A♣ │ │ 8♣ │ │ 5♣ │ │ 4♥ │ ┃ J♣ ┃ │ 9♣ │ │ Q♠ │
└────┘ └────┘ └────┘ └────┘ ┗━━━━┛ └────┘ └────┘
```

## Playing

```sh
cargo run --release              # a fresh deal
cargo run --release -- --seed 42 # a specific deal, repeatable
cargo run --release -- --no-mouse
```

## Rules

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

Not every deal is winnable. `u` undoes as far back as you like, and `r` deals
the same cards again from the start.

## Keys

| Key | |
| --- | --- |
| `← →` `h l` | move between columns |
| `↑ ↓` `k j` | move up and down a column |
| `1`…`7` | jump to a column, or drop a held tail there |
| `space` `enter` | pick up the card under the cursor, or drop what you hold |
| `esc` | put a held tail back |
| `d` | deal the three stock cards |
| `H` | suggest a move; press again for the next one |
| `u` / `ctrl-r` | undo / redo |
| `n` / `r` | new game / replay this deal |
| `?` | help |
| `q` | quit |

Mouse: click a card to pick it up, click a column to drop. Right-click cancels;
the scroll wheel walks up and down a column.

## Finding your place

Cards overlap, so each border row is shared between the card above it and the
card below it. Whichever of the two is highlighted claims that row, which is
what lets a marked card be closed on all four sides even when it sits in the
middle of a pile.

Three things mark the board, and each uses both a colour and a distinct border
weight, so they stay legible where colour does not survive:

| | |
| --- | --- |
| `[5]` above a column, `▶` in the gutter, cyan `║` frame | where the cursor is |
| yellow `┃` frame around a run of cards | the tail you are carrying |
| green `║` frame | where the current hint says to drop |

## Hints

`H` suggests a move, puts the cursor on the card to lift, and frames the card it
should land on. Accepting it is then just `space` and the target column. Press
`H` again to cycle through the alternatives, best first — the count in the status
line (`hint 2/5`) tells you how many there are.

The ranking is a heuristic, not a solver. It prefers moves that turn a card face
up, then moves that empty a column, and it is reluctant to spend an empty column;
it never offers the legal-but-useless move of sliding a whole column into an
empty one. Scorpion cannot be played perfectly without searching, so a hint is a
reasonable move rather than a winning one.

## Layout on small terminals

Scorpion columns get long, so the board picks a density each frame:

* **Roomy** — two rows per card, the usual look above.
* **Tight** — one row per card under a single border, when the tallest column
  will not otherwise fit.
* **Elided** — for columns still too tall, the middle collapses to a `⋯N`
  marker. The exposed card is always shown, since it is the only drop target,
  and so is the card under the cursor.

Below 48×8 the board is replaced by a "terminal too small" note.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

The rules live in `src/game.rs` and are fully testable without a terminal; the
suite includes an undo round-trip that plays a few hundred legal moves and
asserts the board returns to the exact opening deal.

`src/ui/layout.rs` decides where every card sits and is the single source of
truth for the board's shape — the renderer draws from it and mouse clicks are
resolved against it, so a click can never disagree with what is on screen.

The shuffle is a small built-in splitmix64 rather than the `rand` crate, so a
given `--seed` deals the same game permanently instead of changing under a
dependency upgrade.

Other solitaire variants that could be added later, and what each would cost
against the code as it stands, are sketched in
[`docs/future-games.md`](docs/future-games.md).
