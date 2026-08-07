# Future games

Scorpion is the only game this crate plays, and nothing in the code anticipates a second
one — there is no `Game` trait, no variant enum, no registry. So the first new variant is
a refactor rather than an addition, and the interesting question is not "which games are
fun" but "which game forces the least new machinery for the most new play". This file is a
candidate list, not a commitment. Entries are grouped by what they would cost against the
code as it stands.

## What exists to build on

| Piece | Where | Reusable as-is? |
| --- | --- | --- |
| `Card`, `Suit`, `ACE`/`KING`, `face()` | `src/card.rs` | Yes |
| `shuffled_deck(seed)` — splitmix64, Fisher–Yates | `src/card.rs` | One 52-card deck only |
| `Column` — a face-down prefix and a face-up suffix | `src/game.rs` | Yes, for any game with stacked tableau columns |
| Move address `(from, index, to)` | `src/game.rs` `check_move` / `apply_move` | Column-to-column only |
| `Action` plus the undo/redo stacks | `src/game.rs`, `src/app.rs` | The mechanism, yes; the enum is Scorpion's |
| Layout as the single source of truth for board shape | `src/ui/layout.rs` `compute()` and `hit()` | One row of vertical stacks only |
| Cursor, selection, hint cycling, mouse | `src/app.rs` | Yes, but seven columns is baked in — `'1'..='7'` at `src/app.rs:180`, and `step_column` wraps modulo `COLUMNS` |
| Hint ranking — `suggestions()` and `value_of()` | `src/game.rs` | The pattern transfers; the heuristic is Scorpion's |
| Density fallback — Roomy, Tight, elided `⋯N` | `src/ui/layout.rs` | Yes, and every long-column game wants it |

`COLUMNS` is a `const`, not data. It is consumed as an *array length* in three places —
`Game::columns`, `layout::compute`'s `&[usize; COLUMNS]` and `&[Option<usize>; COLUMNS]`
parameters, and `Focus::pins` in `src/ui/render.rs` — so any game with a different column
count turns those arrays into slices. That is the smallest real refactor on this page and
several games below need it.

What does not exist at all: foundation piles, a waste pile, free cells, a reserve, a
multi-deck shuffle, repeatable stock deals, autocomplete to the foundations, and scoring.
Scorpion needs none of them; it builds its runs in the tableau and counts only moves.

## Tier 1 — Scorpion's own engine, near-free

### Wasp

Scorpion's board and Scorpion's build rule — same suit, one rank down, lift a face-up card
with everything under it — but **any card may take an empty column**, not only a King, and
only three cards start face down instead of twelve.

*Why it is worth adding.* It is a much gentler game than Scorpion and shares its whole
vocabulary, so a player who knows one already knows the other.

**Cost against today's code**
- Reuses: everything — board shape, layout, cursor, undo, hints.
- Needs: a second deal function, and the `NotKing` branch of `check_move` made
  conditional.
- Verdict: effectively a configuration of the existing `Game`. Its value is as the
  variant that forces the `Game` trait into existence without being able to break it.

### Three Blind Mice

Ten columns, two face-down cards in each, Scorpion's build rule, and the leftover cards
dealt as stock onto the leftmost columns.

*Why it is worth adding.* A wider board changes the feel of Scorpion considerably — more
places to put things, so less of the game is spent stuck.

**Cost against today's code**
- Reuses: all the rules code.
- Needs: `COLUMNS` as runtime data rather than a `const`, which ripples into the
  `[T; COLUMNS]` parameters in `layout.rs` and `Focus::pins`; also a wider `BOARD_W`
  minimum, and digit keys past `7`.
- Verdict: the cheapest game that pays for the runtime-column-count refactor.

### Double Scorpion

Two decks, ten columns, Scorpion's rules, and eight complete runs to build instead of
four.

*Why it is worth adding.* A long game for people who want one, at almost no rules cost.

**Cost against today's code**
- Reuses: the rules unchanged.
- Needs: a multi-deck `shuffled_deck`, plus the runtime column count above.
- Verdict: cheap, with one trap — `Card` derives `Eq` and `Hash`, and with two decks a
  card value no longer identifies a card. Anything that reasons about cards by value
  rather than by position has to be re-checked.

## Tier 2 — same board, new rules, a completed-suit counter

### Spiderette

Single-deck Spider on seven columns. Build **down regardless of suit**, but only a
**same-suit sequence** may be lifted as a unit. The stock deals one card onto every column
at once, and can be dealt repeatedly. A complete King-to-Ace run in one suit leaves the
tableau for good.

*Why it is worth adding.* It is the first game here whose tension comes from the stock:
every deal buries the board a little further, so the question is always whether to spend
one now.

**Cost against today's code**
- Reuses: `Column`, layout, cursor, mouse, undo mechanism.
- Needs: a validity check on the *lifted tail* — Scorpion lifts any tail, Spider only an
  ordered one; a repeatable stock deal with a remaining-deals count; removal of completed
  runs; and a small "suits done 2/4" indicator in the status line rather than a full
  foundation row.
- Verdict: real new rules work, but no new pile kind and no layout change.

### Spider

Spiderette with two decks and ten columns, and the conventional one-suit, two-suit and
four-suit settings.

*Why it is worth adding.* One-suit Spider is the most forgiving game on this page and the
best on-ramp for someone who has never played a patience game in a terminal.

**Cost against today's code**
- Reuses: everything Spiderette establishes.
- Needs: nothing new once Spiderette, multi-deck and runtime columns exist — the suit
  count is a deal-time parameter.
- Verdict: near-free, third in line behind two other games.

### Yukon

Scorpion's signature move — lift any face-up card together with everything beneath it,
in any order — but building is **alternating colour**, and you win by moving all 52 cards
onto four **foundations** running Ace to King.

*Why it is worth adding.* It shows how much of Scorpion's character comes from the
free-tail move rather than from the same-suit rule, and it is a genuinely well-regarded
game in its own right.

**Cost against today's code**
- Reuses: the tail-lifting model, `Column`, the hint pattern.
- Needs: a foundation pile kind, a top row in the layout to draw it, a move address that
  can name a foundation, and autocomplete for the endgame.
- Verdict: the first entry that forces `layout.rs` to model something other than a row of
  columns. That is the expensive part, and Tier 3 shares the bill.

### Russian Solitaire

Yukon with same-suit building instead of alternating colour.

*Why it is worth adding.* It is one predicate away from Yukon and dramatically harder —
worth listing because once Yukon exists this is close to free.

**Cost against today's code**
- Reuses: all of Yukon.
- Needs: the build predicate.
- Verdict: free companion; ship it alongside Yukon or not at all.

## Tier 3 — new pile kinds and a generalized move address

### Klondike

The game most people mean when they say "solitaire". Seven columns dealt in a triangle,
build down in alternating colour, Kings to empty columns, four foundations Ace up, and a
stock turned to a **waste pile** one or three cards at a time.

*Why it is worth adding.* Familiarity. It is the one variant a new player will not have to
read the rules for.

**Cost against today's code**
- Reuses: `Column`, the density fallback, cursor and mouse.
- Needs: a waste pile with cycling and a redeal limit, foundations with autocomplete,
  and a move address richer than `(column, index, column)` — moves now run between piles
  of *different kinds*, which is the change that most affects `Action`, `check_move` and
  `hit()`.
- Verdict: the largest single step, and the one that leaves the most behind for later
  games.

### FreeCell

Eight columns, all face up from the start, four **free cells** each holding one card, four
foundations. Build down in alternating colour. Nothing is hidden and nearly every deal is
winnable.

*Why it is worth adding.* It fits this project's determinism story better than anything
else here. Because no card is face down, a deal is a pure puzzle — `--seed` stops being a
way to replay a game and becomes a shareable puzzle number, and a loss is the player's
fault rather than the deal's. The existing splitmix64 guarantee, that a seed deals the same
game permanently, is exactly the property such a number needs.

**Cost against today's code**
- Reuses: `Column` (with a face-down count that is always zero), layout's column row,
  the hint pattern — which is more useful here than in Scorpion, since FreeCell positions
  are analysable.
- Needs: free cells as a pile kind, foundations, the generalized move address, and — if
  multi-card moves are supported rather than made one card at a time — the
  `(free cells + 1) × (empty columns + 1)` limit.
- Verdict: comparable in cost to Klondike, and the better game.

### Baker's Game and Seahaven Towers

Baker's Game is FreeCell with same-suit building. Seahaven Towers deals ten columns with
two cards already sitting in free cells, and builds down in suit.

*Why it is worth adding.* Two more games for a predicate and a deal function each.

**Cost against today's code**
- Reuses: all of FreeCell.
- Needs: build predicates and deals; Seahaven also wants the runtime column count.
- Verdict: near-free once FreeCell lands.

### Canfield

A thirteen-card **reserve** feeds the tableau, the foundations start at whatever rank the
deal turns up rather than at Ace and wrap around through King to the rank below, and the
stock is turned three at a time with unlimited redeals.

*Why it is worth adding.* It is the most structurally different of the Klondike family and
the one that stresses a pile-kind model hardest — good insurance that the abstraction is
not just "Klondike with names".

**Cost against today's code**
- Reuses: waste, foundations and the move address from Klondike.
- Needs: a reserve pile kind, and foundations with a base rank plus wrapping comparisons —
  every rank comparison in the shared rules helpers has to stop assuming Ace is the
  bottom.
- Verdict: worthwhile, but only after Klondike or FreeCell has paid for the groundwork.

## Tier 4 — different board geometry

### Golf

Seven columns of five fanned cards and a waste pile. Play any exposed card that is one rank
above or below the top of the waste, with no wrapping between King and Ace — or with
wrapping, as an option, since it makes the game much easier.

*Why it is worth adding.* Two-minute games with a one-sentence rule set, and a useful
contrast to the hour-long ones above.

**Cost against today's code**
- Reuses: the column row, the waste pile once Klondike or Canfield has built one.
- Needs: columns that fan without a face-down prefix and are only five deep — a layout
  case, not a new geometry.
- Verdict: cheap after Tier 3, and the only quick game on the list.

### TriPeaks

Golf's matching rule played over three overlapping peaks of face-down cards, each card
unlocked when both cards covering it are gone.

*Why it is worth adding.* Chaining a long run across the peaks is the most satisfying
single moment in any of these games.

**Cost against today's code**
- Reuses: the waste pile and the matching rule from Golf.
- Needs: a genuinely new layout — overlapping positions on a triangle rather than a row of
  stacks — and a covered/uncovered model in place of `Column`'s face-down prefix.
- Verdict: the layout work is most of the cost and nothing else reuses it except Pyramid.

### Pyramid

A twenty-eight-card triangle. Remove pairs of exposed cards whose ranks sum to thirteen;
Kings go alone.

*Why it is worth adding.* It is the one game here that is not about stacking at all.

**Cost against today's code**
- Reuses: the triangular layout, if TriPeaks built it.
- Needs: that layout, and a **pair selection** interaction — this is the first game where
  `app.rs`'s whole gesture, pick up a tail and drop it on a target, does not describe what
  the player is doing. Selection stops being "held cards" and becomes "first of two".
- Verdict: pair it with TriPeaks or skip it; alone it does not justify the layout.

### Aces Up and Accordion

Aces Up deals four piles and discards any card outranked by another of its suit. Accordion
lays the whole deck in one row and lets a card jump onto its neighbour or the card three
back, aiming to end with a single pile.

*Why it is worth adding.* Tiny rule sets, and they stress the layout in the opposite
direction from Scorpion — very wide and very short rather than narrow and deep.

**Cost against today's code**
- Reuses: cards, shuffle, undo.
- Needs: a horizontal-flow layout for Accordion; almost nothing for Aces Up.
- Verdict: filler, worth doing only when the infrastructure is already paid for.

## What each tier unlocks

The costs above are mostly shared, and the tiers happen to force the shared work in a
sensible order:

1. **A runtime column count** — `COLUMNS` becomes data, the `[T; COLUMNS]` arrays become
   slices. (Three Blind Mice)
2. **A multi-deck shuffle** — and the discipline of never identifying a card by value.
   (Double Scorpion)
3. **Repeatable stock deals and run removal.** (Spiderette)
4. **A foundation pile kind and a top row in the layout.** (Yukon)
5. **A generalized move address** — moves named by pile *kind* and index, not by column
   index, which is what lets `Action`, `check_move` and `hit()` describe games with mixed
   pile types. (Klondike, FreeCell)
6. **Free cells and reserves** as further pile kinds. (FreeCell, Canfield)
7. **Non-column geometry and non-drag interactions.** (TriPeaks, Pyramid)

Two things run underneath all of it. `Game` and `Action` would become a trait with an
associated `Action` type, so each variant keeps its own undo vocabulary while `app.rs`
drives them all — the current `Vec<Action>` history works unchanged under that shape. And
the invariant that `layout.rs` is the single source of truth, so a click can never
disagree with what is drawn, has to survive every step: it is the property most at risk
the moment the board stops being one row of vertical stacks, and it is worth keeping the
render-and-hit-test agreement tests that guard it in `src/app.rs`.

## A suggested order

**Wasp first.** It cannot break, it needs no new machinery, and its only real job is to be
the second implementation that the `Game` trait is extracted against. A trait designed
against one game is a guess; a trait designed against two is at least a hypothesis.

**Spiderette second.** It brings repeatable stock deals and completed-run removal — the
two systems Scorpion happens not to need — while staying on the seven-column board, so
none of the layout risk is in play yet.

**Then FreeCell or Klondike**, whichever answers the more useful question. FreeCell is the
better game and the better fit here: no hidden cards, near-universally winnable deals, and
a seed that becomes a puzzle number worth sharing. Klondike is the one people already know
how to play, which matters if the goal is that someone can open the app and start without
reading anything. Either one pays for foundations, the generalized move address and the
top-row layout, and once one is done Baker's Game, Seahaven, Russian Solitaire, Spider and
Golf are all small.

**Tier 4 last, or never.** TriPeaks and Pyramid need a layout that nothing else reuses,
and the density fallback in `layout.rs` — which is what makes this app work in a small
terminal — would have to be reinvented for a triangle.
