# Future games

The crate plays Klondike, FreeCell, Scorpion, Yukon, Russian Solitaire, Spiderette,
Spider and Mod3, and a variant is now one file in `src/rules/` plus a line in `rules::games()`. So the question is
no longer "what would it take to support a second game" but simply which games are worth
adding and what each still needs from the shared code. This file is a candidate list, not a
commitment. Entries are grouped by cost.

## What exists to build on

| Piece | Where | Reusable as-is? |
| --- | --- | --- |
| `Card`, `Suit`, `ACE`/`KING`, `face()` | `src/card.rs` | Yes |
| `shuffled_deck(seed)` / `shuffled_decks(seed, decks)` — splitmix64, Fisher–Yates | `src/card.rs` | Yes — one deck or many |
| `Pile` — a kind, a face-down prefix, a face-up suffix | `src/board.rs` | Yes, for every pile kind |
| `Board` — a flat `Vec<Pile>` addressed by `PileId` | `src/board.rs` | Yes |
| `Move` = `(from, index, to)` over piles of any kind | `src/board.rs` | Yes |
| `Action`/`Step` and the undo/redo stacks | `src/board.rs`, `src/app.rs` | Yes — the step flags cover flips, reversal and recycles |
| `Rules` trait, with most methods defaulted | `src/rules/mod.rs` | Yes; a simple variant overrides seven methods |
| `BoardSpec` — rows of slots, fanned or flat | `src/board.rs`, `src/ui/layout.rs` | Yes for row-and-slot boards |
| Layout as the single source of truth for board shape | `src/ui/layout.rs` `compute()` and `hit()` | Yes, including multi-row boards |
| Cursor, selection, hint cycling, mouse, menu | `src/app.rs`, `src/ui/menu.rs` | Yes — pile counts and key mappings come from the spec |
| Hint ranking — default `suggestions()` + per-game `rank()` | `src/rules/mod.rs` | Yes; each game supplies only its heuristic |
| One-key play of a forced move — default `only_move()` behind `f` | `src/rules/mod.rs`, `src/app.rs` | Yes, from `check` alone; a game with no foundations still gets the key |
| Density fallback — Roomy, Tight, elided `⋯N` | `src/ui/layout.rs` | Yes, and every long-pile game wants it |
| Foundations, stock, waste, draw/recycle, send-home, autoplay | `src/rules/klondike.rs` | As worked examples, not as shared code |
| Per-game rule options in the menu and on the CLI | `src/rules/mod.rs`, `src/ui/menu.rs` | Yes — declare `OptionSpec`s and read `selected()` |
| A free-tail lift onto foundations, with no stock at all | `src/rules/yukon.rs` | Yes — `pub(super)` functions, already shared with `russian.rs` |
| `PileKind::FreeCell` — a pile holding exactly one card | `src/board.rs`, `src/rules/freecell.rs` | Yes, and as the pattern for any new pile kind |
| A move-size limit the app cannot know about, enforced in `check` | `src/rules/freecell.rs` `capacity` | Yes — selection is never capped by `app.rs` |
| A board with nothing face down, and an `autoplay` gate to match | `src/rules/freecell.rs` `is_decided` | Yes, and required: `face_down_count` says nothing there |
| Two games differing by one predicate, in two files | `src/rules/yukon.rs`, `src/rules/russian.rs` | Yes, as the pattern to copy; `Build` is shared by three games now |
| Two games differing by three parameters, via one `Setup` struct | `src/rules/spider.rs`, `src/rules/spiderette.rs` | Yes, as the pattern to copy when it is more than a predicate |
| Cards leaving play — steps appended to the move's own `Action` | `src/rules/spider.rs` `with_filing` | Yes, as the pattern to copy |
| A stock that deals onto every column at once, repeatedly | `src/rules/spider.rs` | Yes |
| A stock that auto-refills emptied tableau piles mid-move — steps appended to the player's `Action` | `src/rules/mod3.rs` `with_autofill` | Yes, as the pattern to copy |
| Foundations whose base rank is not Ace (2, 3, or 4) and that build in steps larger than one | `src/rules/mod3.rs` `foundation_check` | Yes, as a worked example |
| A board ten columns wide, with `0` as the tenth shortcut | `src/rules/spider.rs`, `src/app.rs` | Yes — `board_width` sizes it at 69 cells |

What still does not exist: `PileKind::Reserve` (deliberately not added until a game needs
it), non-row board geometry, and scoring.

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
- Needs: a deal function and a spec with ten slots. Spider already paid for the tenth
  digit shortcut and the wider board.
- Verdict: a deal function and a constant.

### Double Scorpion

Two decks, ten columns, Scorpion's rules, and eight complete runs to build instead of
four.

*Why it is worth adding.* A long game for people who want one, at almost no rules cost.

**Cost against today's code**
- Reuses: the rules unchanged.
- Needs: nothing. `shuffled_decks(seed, 2)` arrived with Spider.
- Verdict: cheap, and the one trap is now documented rather than theoretical — `Card`
  derives `Eq` and `Hash`, and with two decks a card value no longer identifies a card.
  Anything that reasons about cards by value rather than by position has to be
  re-checked, and a card-conservation test needs a multiset rather than a `HashSet`.

## Tier 2 — a new pile kind each

### FreeCell — shipped

`src/rules/freecell.rs`. It cost what this file predicted: `PileKind::FreeCell`, four
`Fan::None` slots beside the foundations, and the `(free cells + 1) × (empty columns + 1)`
limit in `check`. Three things were worth more than they looked:

- The limit could only live in `check`. `app.rs` never caps how many cards are picked up,
  so a game with a move-size rule refuses it where the player's drop and `suggestions` both
  arrive — and the move stays one `Action::moving`, so a supermove is still one undo.
- `autoplay` had to be re-derived. Every other game gates on `board.face_down_count() > 0`,
  which here is permanently zero and would call the opening deal decided. The gate is
  `is_decided`: every column running down in rank, which makes the lowest card still out
  reachable and playable. `autoplay` also has to read the cells, or a finish strands cards
  in them and never wins.
- A new pile kind is four places, not one: the variant, `check`'s target match,
  `app.rs::suggest`'s wording for an empty target, and `render::draw_flat`'s marker for an
  empty pile. Skipping the last two gives a board of identical empty boxes and a hint that
  calls a cell "an empty pile".

`Build` moved from `yukon.rs` to `rules/mod.rs` on the way, since three games now share it.

### Mod3 — shipped

`src/rules/mod3.rs`. Two decks, a 4×8 grid, foundations building in steps of
three. Two things were worth more than they looked:

- **Auto-fill used the same `with_filing` shape as Spider**, not a separate
  mechanism. When a player move empties a working-row pile, the action that
  caused it gets extra `Step`s appended (computed against a scratch clone, each
  step's flags against the state it will actually meet). One `u` undoes the
  move and the refill together, and `app.rs` never learns what a refill is.
- **The `WrongTarget` guard for stock and aces sources had to come before the
  face-up guard.** Stock cards are face down; without that ordering, picking up
  from the stock returned `FaceDown` instead of `WrongTarget`, which is a
  misleading error. Lesson: guards on the source pile's *kind* go first.

`with_autofill` is the pattern to copy for any game where a player move
triggers an automatic deal step.

### Baker's Game and Seahaven Towers

Baker's Game is FreeCell with same-suit building. Seahaven Towers deals ten columns with
two cards already sitting in free cells, and builds down in suit.

*Why it is worth adding.* Two more games for a predicate and a deal function each.

**Cost against today's code**
- Reuses: all of FreeCell, whose half is already `pub(super)` and already takes a `Build`.
- Needs: Baker's Game is `russian.rs` with `Build::SameSuit` — a file of forwarding methods.
  Seahaven diverges in the deal and the spec as well as the predicate, so it wants a `Copy`
  `Setup` in the `spiderette.rs` mould rather than a second argument threaded everywhere.
- Verdict: Baker's Game is an afternoon; Seahaven is a day.

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

## Tier 3 — different board geometry

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
- Verdict: cheap after Tier 2, and the only quick game on the list.

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

1. **`PileKind::Reserve`** — removed from `board.rs` on purpose, so it arrives with the
   game that needs it. `PileKind::FreeCell` arrived with FreeCell exactly that way, and the
   four places it touched are listed under that entry. (Canfield)
2. **Foundations that do not start at Ace** — a base rank and wrapping rank comparisons.
   (Canfield)
3. **Non-row geometry and non-drag interactions.** The layout models rows of slots; a
   triangle is not one, and Pyramid's "pick two cards" is not the pick-up-and-drop gesture
   `app.rs` implements. (TriPeaks, Pyramid, Accordion)

A pile kind of its own was the first entry here until FreeCell shipped it. The multi-deck
shuffle and cards leaving play were the two before that, until Spider shipped both. What that cost is worth knowing before the next such mechanic: the shuffle
was six lines and one assertion (`shuffled_decks(seed, 1) == shuffled_deck(seed)`, which is
what let two decks arrive without moving a single existing deal), and cards leaving play
turned out to need no new shared code at all — a finished run is filed by extra `Step`s
appended to the *same* `Action` as the move that finished it.

Two things run underneath all of it, and both are already load-bearing. `Action` is shared
rather than per-game — a `Vec<Step>` applied in order and undone in reverse — which is what
keeps `app.rs` non-generic over `Box<dyn Rules>`; a new mechanic should be expressible as
step flags rather than as a new action type, and Spider's filing is the proof that a
genuinely new mechanic can be. And `layout.rs` is the single source of truth,
so a click can never disagree with what is drawn: that property is what a triangular layout
would be most at risk of quietly breaking, and the round-trip tests in `ui/layout.rs` and
`app.rs` are what would catch it.

## A suggested order

**Baker's Game next**, now that FreeCell has shipped and left its half `pub(super)` behind
a `Build` argument. Seahaven Towers after it, for one `Setup` more.

**Wasp and Three Blind Mice** any time — they are afternoon projects, and good first
exercises for anyone learning the `Rules` trait.

**Tier 3 last, or never.** TriPeaks and Pyramid need a layout that nothing else reuses, and
the density fallback that makes this app work in a small terminal would have to be
reinvented for a triangle.
