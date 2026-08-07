# Future games

The crate plays Klondike and Scorpion, and a variant is now one file in `src/rules/` plus a
line in `rules::games()`. So the question is no longer "what would it take to support a
second game" but simply which games are worth adding and what each still needs from the
shared code. This file is a candidate list, not a commitment. Entries are grouped by cost.

## What exists to build on

| Piece | Where | Reusable as-is? |
| --- | --- | --- |
| `Card`, `Suit`, `ACE`/`KING`, `face()` | `src/card.rs` | Yes |
| `shuffled_deck(seed)` — splitmix64, Fisher–Yates | `src/card.rs` | One 52-card deck only |
| `Pile` — a kind, a face-down prefix, a face-up suffix | `src/board.rs` | Yes, for every pile kind |
| `Board` — a flat `Vec<Pile>` addressed by `PileId` | `src/board.rs` | Yes |
| `Move` = `(from, index, to)` over piles of any kind | `src/board.rs` | Yes |
| `Action`/`Step` and the undo/redo stacks | `src/board.rs`, `src/app.rs` | Yes — the step flags cover flips, reversal and recycles |
| `Rules` trait, with most methods defaulted | `src/rules/mod.rs` | Yes; a simple variant overrides seven methods |
| `BoardSpec` — rows of slots, fanned or flat | `src/board.rs`, `src/ui/layout.rs` | Yes for row-and-slot boards |
| Layout as the single source of truth for board shape | `src/ui/layout.rs` `compute()` and `hit()` | Yes, including multi-row boards |
| Cursor, selection, hint cycling, mouse, menu | `src/app.rs`, `src/ui/menu.rs` | Yes — pile counts and key mappings come from the spec |
| Hint ranking — default `suggestions()` + per-game `rank()` | `src/rules/mod.rs` | Yes; each game supplies only its heuristic |
| Density fallback — Roomy, Tight, elided `⋯N` | `src/ui/layout.rs` | Yes, and every long-pile game wants it |
| Foundations, stock, waste, draw/recycle, send-home, autoplay | `src/rules/klondike.rs` | As worked examples, not as shared code |
| Per-game rule options in the menu and on the CLI | `src/rules/mod.rs`, `src/ui/menu.rs` | Yes — declare `OptionSpec`s and read `selected()` |

What still does not exist: a multi-deck shuffle, `PileKind::FreeCell` and
`PileKind::Reserve` (deliberately not added until a game needs them), non-row board
geometry, and scoring. The digit keys run `1`…`9`, so a game with ten tableau piles needs
one more shortcut.

## Tier 1 — Scorpion's own rules, near-free

### Wasp

Scorpion's board and Scorpion's build rule — same suit, one rank down, lift a face-up card
with everything under it — but **any card may take an empty column**, not only a King, and
only three cards start face down instead of twelve.

*Why it is worth adding.* It is a much gentler game than Scorpion and shares its whole
vocabulary, so a player who knows one already knows the other.

**Cost against today's code**
- Reuses: everything — `Scorpion`'s `check`, `rank`, `is_won` and spec.
- Needs: a deal function, and the `NotKing` arm of `check` made conditional. Could even be
  a `Scorpion` rule option rather than its own file.
- Verdict: an afternoon. The cheapest game on this page.

### Three Blind Mice

Ten columns, two face-down cards in each, Scorpion's build rule, and the leftover cards
dealt as stock onto the leftmost columns.

*Why it is worth adding.* A wider board changes the feel of Scorpion considerably — more
places to put things, so less of the game is spent stuck.

**Cost against today's code**
- Reuses: all of it. Pile counts already come from the spec, and `board_width` sizes
  itself, so ten columns needs no layout work.
- Needs: a deal function and a spec with ten slots. A tenth digit shortcut, since the
  keys stop at `9`.
- Verdict: a deal function and a constant.

### Double Scorpion

Two decks, ten columns, Scorpion's rules, and eight complete runs to build instead of
four.

*Why it is worth adding.* A long game for people who want one, at almost no rules cost.

**Cost against today's code**
- Reuses: the rules unchanged.
- Needs: a multi-deck `shuffled_deck` — the one genuinely missing piece of shared code.
- Verdict: cheap, with one trap — `Card` derives `Eq` and `Hash`, and with two decks a
  card value no longer identifies a card. Anything that reasons about cards by value
  rather than by position has to be re-checked.

## Tier 2 — new rules on the shared machinery

### Spiderette

Single-deck Spider on seven columns. Build **down regardless of suit**, but only a
**same-suit sequence** may be lifted as a unit. The stock deals one card onto every column
at once, and can be dealt repeatedly. A complete King-to-Ace run in one suit leaves the
tableau for good.

*Why it is worth adding.* It is the first game here whose tension comes from the stock:
every deal buries the board a little further, so the question is always whether to spend
one now.

**Cost against today's code**
- Reuses: nearly everything. Klondike already shows the sequence check (`is_sequence`), a
  repeatable `activate` on the stock, and a `StatusItem` counter.
- Needs: a stock deal that puts one card on *every* pile — a multi-step `Action`, which
  Scorpion's stock deal already demonstrates; and removal of completed runs, the one
  genuinely new mechanic (cards leaving play rather than moving).
- Verdict: real rules work, but no new shared code.

### Spider

Spiderette with two decks and ten columns, and the conventional one-suit, two-suit and
four-suit settings.

*Why it is worth adding.* One-suit Spider is the most forgiving game on this page and the
best on-ramp for someone who has never played a patience game in a terminal.

**Cost against today's code**
- Reuses: everything Spiderette establishes.
- Needs: the multi-deck shuffle. The suit count is an `OptionSpec`, which the menu already
  knows how to present.
- Verdict: near-free once Spiderette lands.

### Yukon

Scorpion's signature move — lift any face-up card together with everything beneath it,
in any order — but building is **alternating colour**, and you win by moving all 52 cards
onto four **foundations** running Ace to King.

*Why it is worth adding.* It shows how much of Scorpion's character comes from the
free-tail move rather than from the same-suit rule, and it is a genuinely well-regarded
game in its own right.

**Cost against today's code**
- Reuses: everything. Klondike already pays for foundations, the top row, `send_home` and
  `autoplay`; Scorpion already has the free-tail lift.
- Needs: a deal function, an alternating-colour predicate, and a spec — essentially
  Klondike's spec minus the stock and waste.
- Verdict: now one of the cheapest games here, not one of the most expensive. It was
  Tier 3 work before Klondike existed.

### Russian Solitaire

Yukon with same-suit building instead of alternating colour.

*Why it is worth adding.* It is one predicate away from Yukon and dramatically harder —
worth listing because once Yukon exists this is close to free.

**Cost against today's code**
- Reuses: all of Yukon.
- Needs: the build predicate.
- Verdict: free companion; ship it alongside Yukon or not at all.

## Tier 3 — a new pile kind each

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
- Reuses: foundations, `send_home`, `autoplay` and the two-row spec from Klondike; the
  hint ranking is more useful here than anywhere else, since FreeCell positions are
  analysable.
- Needs: `PileKind::FreeCell` (removed from `board.rs` until a game wants it) plus its own
  `Fan::None` slots, and — if multi-card moves are supported rather than made one card at
  a time — the `(free cells + 1) × (empty piles + 1)` limit in `check`.
- Verdict: the best remaining game, and no longer an expensive one.

### Baker's Game and Seahaven Towers

Baker's Game is FreeCell with same-suit building. Seahaven Towers deals ten columns with
two cards already sitting in free cells, and builds down in suit.

*Why it is worth adding.* Two more games for a predicate and a deal function each.

**Cost against today's code**
- Reuses: all of FreeCell.
- Needs: build predicates and deals.
- Verdict: near-free once FreeCell lands.

### Canfield

A thirteen-card **reserve** feeds the tableau, the foundations start at whatever rank the
deal turns up rather than at Ace and wrap around through King to the rank below, and the
stock is turned three at a time with unlimited redeals.

*Why it is worth adding.* It is the most structurally different of the Klondike family and
the one that stresses a pile-kind model hardest — good insurance that the abstraction is
not just "Klondike with names".

**Cost against today's code**
- Reuses: Klondike's waste, foundations, draw and recycle almost wholesale — `Step.recycle`
  and `Board::recycles()` already exist to keep a redeal limit honest under undo.
- Needs: `PileKind::Reserve`, and foundations with a base rank plus wrapping comparisons —
  every rank comparison has to stop assuming Ace is the
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
  stacks — and a covered/uncovered model in place of `Pile`'s face-down prefix.
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

## What is left to build

Most of the shared work the earlier tiers called for is now done. What a future game might
still need from the common code, roughly in the order the games above want it:

1. **A multi-deck shuffle** — and the discipline of never identifying a card by value,
   since `Card` derives `Eq` and `Hash`. (Double Scorpion, Spider)
2. **Cards leaving play** — a completed run removed from the tableau rather than moved.
   `Step` can already express it as a move to a hidden pile; nothing does yet. (Spiderette)
3. **`PileKind::FreeCell` and `PileKind::Reserve`** — removed from `board.rs` on purpose,
   so they arrive with the game that needs them. (FreeCell, Canfield)
4. **Foundations that do not start at Ace** — a base rank and wrapping rank comparisons.
   (Canfield)
5. **Non-row geometry and non-drag interactions.** The layout models rows of slots; a
   triangle is not one, and Pyramid's "pick two cards" is not the pick-up-and-drop gesture
   `app.rs` implements. (TriPeaks, Pyramid, Accordion)

Two things run underneath all of it, and both are already load-bearing. `Action` is shared
rather than per-game — a `Vec<Step>` applied in order and undone in reverse — which is what
keeps `app.rs` non-generic over `Box<dyn Rules>`; a new mechanic should be expressible as
step flags rather than as a new action type. And `layout.rs` is the single source of truth,
so a click can never disagree with what is drawn: that property is what a triangular layout
would be most at risk of quietly breaking, and the round-trip tests in `ui/layout.rs` and
`app.rs` are what would catch it.

## A suggested order

**Yukon next.** It went from the most expensive game on this page to one of the cheapest
the moment Klondike paid for foundations and the top row — it is now a deal function, a
build predicate and a spec. Russian Solitaire comes free with it.

**Then FreeCell**, for the reason it was always worth having: nothing is face down, so a
deal is a pure puzzle and `--seed` becomes a shareable puzzle number rather than just a
replay. The determinism guarantee in `card.rs` is exactly the property such a number needs.
Baker's Game and Seahaven follow it almost for free.

**Spiderette and Spider** whenever the multi-deck shuffle gets written; they are the only
games here whose tension comes from the stock rather than the tableau.

**Wasp and Three Blind Mice** any time — they are afternoon projects, and good first
exercises for anyone learning the `Rules` trait.

**Tier 4 last, or never.** TriPeaks and Pyramid need a layout that nothing else reuses, and
the density fallback that makes this app work in a small terminal would have to be
reinvented for a triangle.
