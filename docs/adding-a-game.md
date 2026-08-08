# Adding a game

A solitaire variant is one file in `src/rules/` plus a line in `rules::games()`.
Everything else — the board, undo, the cursor, the mouse, the layout, the menu,
hints, the help popup — is shared and needs no changes.

This guide walks through building **Yukon** from nothing, then documents each
piece of the contract in more detail. For complete examples in the tree,
`src/rules/scorpion.rs` is the smallest game and `src/rules/klondike.rs` uses
nearly every hook there is.

Yukon has since shipped, so [`src/rules/yukon.rs`](../src/rules/yukon.rs) is this
walkthrough as it actually landed — worth reading alongside. It differs in two
ways, both noted where they come up: its shared half is factored out as
`pub(super)` functions so [`src/rules/russian.rs`](../src/rules/russian.rs) can
be Russian Solitaire in forty lines, and its `is_useful` is a little more careful
than the version below.

## Contents

- [What you supply, and what you get](#what-you-supply-and-what-you-get)
- [Walkthrough: Yukon](#walkthrough-yukon)
- [The `Rules` trait, method by method](#the-rules-trait-method-by-method)
- [The board API](#the-board-api)
- [Actions and undo](#actions-and-undo)
- [Laying out the board](#laying-out-the-board)
- [Rule options](#rule-options)
- [Testing a new game](#testing-a-new-game)
- [Checklist](#checklist)
- [Traps](#traps)

## What you supply, and what you get

You implement seven methods. Everything else on `Rules` has a default that is
correct for a plain tableau game.

| You supply | You get for free |
| --- | --- |
| `name`, `blurb`, `help` | the menu entry, preview pane, and `?` popup |
| `deal` | seeded, repeatable deals via `--seed` |
| `spec` | placement, density fallback, elision, hit-testing |
| `check` | move validation wherever a move can come from |
| `is_won` | the win banner |
| — | undo/redo, cursor, selection, mouse, digit keys |
| — | hints, via a default `suggestions` that ranks with your `rank` |
| — | `--game`, `--rule`, `--list`, and the menu's rules pane |

The shared code never asks *what kind of game* it is driving. It asks the rules
whether a move is legal, what an action does, and whether the game is over.

## Walkthrough: Yukon

Yukon is Scorpion's free-tail lift with Klondike's alternating colours and
foundations, and no stock at all. It is a good first game because it needs a
second board row but no new mechanics.

### 1. The file and the pile layout

Decide the piles and their ids first — everything else is written in terms of
them. Ids are just indices into `Board`'s flat `Vec<Pile>`, so fix them as
constants:

```rust
use crate::board::{
    Action, Board, BoardSpec, Fan, Move, MoveError, Pile, PileId, PileKind, Spot,
};
use crate::card::{ACE, Card, KING, shuffled_deck};
use crate::rules::{Rules, StatusItem, StatusTone};

const TABLEAU: usize = 7;
const FOUNDATIONS: usize = 4;
const FACE_UP_PER_PILE: usize = 5;
/// The first foundation; the rest follow it.
const FOUNDATION: PileId = TABLEAU;

#[derive(Default)]
pub struct Yukon;
```

The struct holds no board state — only rule *settings*, if the game has any.
The board lives in `Session`.

### 2. Dealing

`deal` builds the `Vec<Pile>` in id order and hands it to `Board::new`. Use
`shuffled_deck(seed)` and consume it with `by_ref().take(n)` so the deal stays a
single pass:

```rust
fn deal(&self, seed: u64) -> Board {
    let mut deck = shuffled_deck(seed).into_iter();
    let mut piles: Vec<Pile> = (0..TABLEAU)
        .map(|i| {
            // The first pile is a lone face-up card; the rest bury `i`
            // cards under five face-up ones.
            let total = if i == 0 { 1 } else { i + FACE_UP_PER_PILE };
            let cards: Vec<_> = deck.by_ref().take(total).collect();
            Pile::tableau(i, &cards)
        })
        .collect();

    piles.extend((0..FOUNDATIONS).map(|_| Pile::new(PileKind::Foundation)));
    Board::new(seed, piles)
}
```

`Pile::tableau(face_down, cards)` is shorthand for
`Pile::holding(PileKind::Tableau, face_down, cards)`. Face-down cards are always
a *prefix*, so the count is all you pass.

### 3. Where the piles sit

`spec` returns a `BoardSpec` of `Spot`s: a pile, a row, a slot along that row,
and whether it fans. Yukon puts four foundations in a fixed top row and seven
fanned tableau piles below:

```rust
fn spec(&self, _board: &Board) -> BoardSpec {
    let mut spots: Vec<Spot> = Self::foundations()
        .enumerate()
        .map(|(i, pile)| Spot::new(pile, 0, 3 + i as u16, Fan::None))
        .collect();
    spots.extend((0..TABLEAU).map(|i| Spot::new(i, 1, i as u16, Fan::Down)));
    BoardSpec::new(spots)
}
```

Slots 0–2 of row 0 are simply left empty — that is how you get a gap. A pile
with no spot is not drawn at all, which is how Scorpion keeps a stock that only
ever appears as a status-bar count.

### 4. Legality

`check` is the heart of it. It is called for every move the player attempts
*and* by the default `suggestions` for every move it enumerates, so it must
reject nonsense cheaply and completely — including moves involving piles that
should never be sources or targets.

```rust
fn check(&self, board: &Board, mv: Move) -> Result<(), MoveError> {
    let Move { from, index, to } = mv;
    if from == to {
        return Err(MoveError::SamePile);
    }
    let source = board.pile(from);
    let target = board.pile(to);
    if index >= source.len() {
        return Err(MoveError::NoSuchCard);
    }
    if !source.is_face_up(index) {
        return Err(MoveError::FaceDown);
    }

    let last = source.len() - 1;
    if source.kind() == PileKind::Foundation && index != last {
        return Err(MoveError::NotExposed);
    }

    let head = source.cards()[index];
    if target.kind() == PileKind::Foundation {
        if source.kind() == PileKind::Foundation {
            return Err(MoveError::WrongTarget);
        }
        if index != last {
            return Err(MoveError::NotExposed);
        }
        return if Self::foundation_takes(board, to, head) {
            Ok(())
        } else {
            Err(MoveError::Mismatch)
        };
    }

    // Landing on the tableau. Unlike Klondike, the tail need not be in
    // order — that free lift is what makes this Yukon.
    match target.exposed() {
        Some(card)
            if card.rank == head.rank + 1 && card.suit.is_red() != head.suit.is_red() =>
        {
            Ok(())
        }
        Some(_) => Err(MoveError::Mismatch),
        None if head.rank == KING => Ok(()),
        None => Err(MoveError::NotKing),
    }
}
```

Order matters: bounds before face-up before rules, so the message the player
sees names the first thing actually wrong.

### 5. Winning, and the helpers

```rust
impl Yukon {
    fn foundations() -> impl Iterator<Item = PileId> {
        FOUNDATION..FOUNDATION + FOUNDATIONS
    }

    fn foundation_takes(board: &Board, pile: PileId, card: Card) -> bool {
        match board.pile(pile).exposed() {
            Some(top) => top.suit == card.suit && card.rank == top.rank + 1,
            None => card.rank == ACE,
        }
    }

    fn home_for(board: &Board, card: Card) -> Option<PileId> {
        Self::foundations().find(|&pile| Self::foundation_takes(board, pile, card))
    }
}

fn is_won(&self, board: &Board) -> bool {
    Self::foundations().all(|pile| board.pile(pile).is_complete_suit())
}
```

`Pile::is_complete_suit` (Ace up to King, one suit) and `Pile::is_complete_run`
(King down to Ace, one suit, nothing hidden) are both on `Pile` already.
`is_complete_suit` is also what certifies a Spider foundation: a finished run is
turned over as it is filed, so it arrives Ace up to King like everyone else's.

### 6. Convenience moves

`f` is two halves. `send_home` is the first — the foundation, where there is
never anything to choose between. The second is the defaulted `Rules::only_move`,
which plays the card onto the one column that takes it, and your game gets that
half for free from `check`: no method to write, and a game with no foundations at
all still has a working `f`. `autoplay` backs `A` and is drained one action at a
time so the finish stays undoable:

```rust
fn send_home(&self, board: &Board, from: PileId) -> Option<Action> {
    let source = board.pile(from);
    if source.kind() != PileKind::Tableau {
        return None;
    }
    let card = source.exposed()?;
    let home = Self::home_for(board, card)?;
    Some(Action::moving(board, from, source.len() - 1, home))
}

fn autoplay(&self, board: &Board) -> Option<Action> {
    if board.face_down_count() > 0 || self.is_won(board) {
        return None;
    }
    (0..TABLEAU)
        .filter_map(|pile| {
            let card = board.pile(pile).exposed()?;
            Some((card.rank, pile, Self::home_for(board, card)?))
        })
        .min()
        .map(|(_, pile, home)| Action::moving(board, pile, board.pile(pile).len() - 1, home))
}
```

`autoplay` must eventually return `None`, or `Session::finish` will spin. Gate
it on a condition that the actions themselves make false — here, the game being
won.

`only_move` filters with `check` *and* `is_useful`, the same pair `suggestions`
uses, so a move your hints refuse to offer is one the shortcut will not play
either. It answers `None` the moment two occupied columns would both take the
card: that is a decision the player has to make. Empty columns are its last
resort and count collectively as one destination, since they are
interchangeable — override it only if that is untrue in your game.

### 7. Hints

The default `suggestions` enumerates every legal move and sorts by `rank`,
highest first. You supply the taste:

```rust
fn is_useful(&self, board: &Board, mv: Move) -> bool {
    let to = board.pile(mv.to);
    board.pile(mv.from).kind() != PileKind::Foundation
        && !(mv.index == 0 && to.kind() == PileKind::Tableau && to.is_empty())
}

fn rank(&self, board: &Board, mv: Move) -> i32 {
    let Move { from, index, to } = mv;
    let source = board.pile(from);
    let mut value = 0;

    if index > 0 && index == source.face_down() {
        value += 100;   // turns a card over
    }
    if board.pile(to).kind() == PileKind::Foundation {
        value += 60;
    }
    if index == 0 && source.kind() == PileKind::Tableau {
        value += 30;    // empties a pile
    }
    if board.pile(to).kind() == PileKind::Tableau && board.pile(to).is_empty() {
        value -= 25;    // spends a scarce empty pile
    }
    value - (source.len() - index) as i32
}
```

`is_useful` filters out moves that are legal but pointless, so `H` never offers
them. Taking a card back off a foundation and sliding a whole pile into an empty
one are the usual two.

Note the `PileKind::Tableau` in that second clause. Without it, "index 0 into an
empty pile" also describes sending a lone Ace to a bare foundation — and Yukon's
first pile is exactly one card, so `H` would refuse to suggest the best opening
move on the board. Qualify anything you say about *empty* piles with what kind of
pile you meant.

### 8. Status, text, and registering

```rust
fn status(&self, board: &Board) -> Vec<StatusItem> {
    vec![StatusItem::new(
        "home",
        format!("{}/52", Self::foundations().map(|p| board.pile(p).len()).sum::<usize>()),
        StatusTone::Good,
    )]
}
```

Seed and move count are shown for every game; `status` is only for the extras.

Then two lines in `src/rules/mod.rs`:

```rust
pub mod yukon;

pub fn games() -> Vec<Box<dyn Rules>> {
    vec![
        Box::new(klondike::Klondike::default()),
        Box::new(scorpion::Scorpion),
        Box::new(yukon::Yukon),
    ]
}
```

`games()` fixes the order the menu lists them in, so put a new game where you want
a player to meet it, not necessarily last.

That is the whole job. The menu lists it, `--game yukon` plays it, `?` shows its
help, and the generic tests in `rules/mod.rs` start covering it immediately.

## The `Rules` trait, method by method

Defined in `src/rules/mod.rs`.

### Required

| Method | Notes |
| --- | --- |
| `name(&self) -> &'static str` | Menu label and `--game` key, matched case-insensitively. Must be unique. |
| `blurb(&self) -> &'static str` | A few sentences for the preview pane. Wrapped for you; `\n\n` makes a paragraph. |
| `help(&self) -> &'static str` | *Only* the keys this game adds. The shared keys are wrapped around it by `render::help_text`. Return `""` if there are none. Two leading spaces, then the key, padded to column 16. |
| `deal(&self, seed) -> Board` | Must be pure in `seed`. Build piles in id order. |
| `spec(&self, board) -> BoardSpec` | Usually ignores `board`; take it if the shape depends on the deal. |
| `check(&self, board, mv) -> Result<(), MoveError>` | Called for player moves *and* by `suggestions`. Reject bad pile kinds explicitly. |
| `is_won(&self, board) -> bool` | Checked after every action. |

### Defaulted

| Method | Default | Override when |
| --- | --- | --- |
| `action_for(board, mv)` | `Action::moving(...)` — lift a tail onto another pile | the move does more than move cards — `spider.rs` appends the steps that file a finished run, so the whole thing stays one undo |
| `activate(board, pile)` | `None` | a pile *does* something when pressed: deal, draw, recycle |
| `stock_refusal(board)` | `"The stock has nothing left to give."` | a *full* stock can still refuse, and saying it is spent would be a lie — Spider will not deal onto an empty column |
| `send_home(board, from)` | `None` | the game has foundations, so `f` should try one before it looks at the columns. The app passes the pile of the card in hand when one is held, so it must work for a lone card as well as for the pile under the cursor |
| `only_move(board, from, index)` | the one column `check` allows, empty ones last | never, in practice — the default is the whole of the `f` key for a game without foundations, and it is derived from `check` |
| `autoplay(board)` | `None` | a decided position should be playable out with `A` |
| `is_useful(board, mv)` | `true` | some legal moves are pointless |
| `rank(board, mv)` | `0` | you want hints in a sensible order (you do) |
| `suggestions(board)` | enumerate + filter + sort by `rank` | the enumeration is wrong for your game, not just slow |
| `has_progress_move(board)` | `!suggestions().is_empty()` | a stock that can still be turned counts as progress |
| `status(board)` | empty | there is a counter worth showing |
| `options` / `selected` / `configure` | none | the game has settable rules |

Two overrides are easy to forget:

- **`has_progress_move`** — the default only looks at board moves. If your game
  has a stock, an exhausted board is not a dead game. Both Scorpion and Klondike
  override this.
- **`is_useful`** — without it, `H` will cheerfully suggest undoing your own
  progress.

## The board API

From `src/board.rs`. None of it knows any rules.

### `Pile`

```rust
Pile::new(kind)                          // empty
Pile::holding(kind, face_down, cards)    // with cards
Pile::tableau(face_down, cards)          // the common case

pile.kind() -> PileKind
pile.cards() -> &[Card]                  // index 0 is the bottom
pile.len() / pile.is_empty()
pile.face_down() -> usize                // always a prefix
pile.is_face_up(index) -> bool
pile.exposed() -> Option<Card>           // the last card — what others land on
pile.first_face_up() -> Option<usize>
pile.is_complete_run() -> bool           // K..A, one suit, nothing hidden
pile.is_complete_suit() -> bool          // A..K, one suit
```

`PileKind` is `Tableau | Foundation | Stock | Waste`. `FreeCell` and `Reserve`
were deliberately left out until a game needs them — add the variant with the
game.

### `Board`

```rust
board.pile(id) -> &Pile
board.piles() -> &[Pile]                 // `board.piles().len()` for the count
board.seed() / board.moves() / board.recycles()
board.of_kind(kind) -> impl Iterator<Item = PileId>
board.first_of_kind(kind) -> Option<PileId>
board.face_down_count() -> usize         // handy for autoplay conditions
board.apply(&action) / board.undo(&action) / board.redo(&action)
```

`Board` derives `PartialEq`, which is what makes the undo round-trip test a
one-liner.

`first_of_kind(PileKind::Stock)` is how the `d` key finds your stock — so if
your game has one, give it `PileKind::Stock` even if it behaves unusually.

## Actions and undo

`Board` holds no history. An `Action` is a `Vec<Step>` applied in order and
undone in **reverse**; `Session` owns the undo/redo stacks. Because the action
type is shared rather than per-game, `app.rs` never needs to know which game it
is driving.

A `Step` records what a transfer did to the cards' orientation and order, so
`Board::undo` can reverse it without consulting any rules:

```rust
pub struct Step {
    pub from: PileId,
    pub to: PileId,
    pub count: usize,
    pub reverse: bool,       // cards arrive in the opposite order
    pub was_face_up: bool,   // were they face up where they came from?
    pub face_up: bool,       // are they face up where they land?
    pub flip_source: bool,   // did taking them turn the source's new last card up?
    pub recycle: bool,       // does this spend a redeal?
}
```

**Use `Action::moving(board, from, index, to)` for anything that is just moving
cards.** It computes `count`, `was_face_up` and `flip_source` from the board.
That covers every tableau move, every foundation send, and every play off the
waste. Hand-build `Step`s only for the unusual:

```rust
// Scorpion: one card onto each of three piles, as a single undoable action.
Action::new(
    (0..3)
        .map(|to| Step {
            from: STOCK, to, count: 1,
            reverse: false, was_face_up: false, face_up: true,
            flip_source: false, recycle: false,
        })
        .collect(),
)

// Klondike: turn the waste back into the stock, face down and reversed,
// spending a redeal.
Action::single(Step {
    from: WASTE, to: STOCK,
    count: board.pile(WASTE).len(),
    reverse: true, was_face_up: true, face_up: false,
    flip_source: false, recycle: true,
})
```

Rules for hand-built steps:

- Cards always come off the **end** of `from` and land on the **end** of `to`.
- `face_up: false` is only legal onto a pile that is *already entirely face
  down*. There is a `debug_assert` for this.
- `reverse` models flipping a packet over. Klondike's draw and recycle are both
  `reverse: true`, which is exactly what makes a recycled stock deal the same
  cards again.
- `recycle: true` increments `Board::recycles()`, and undoing decrements it —
  that is how a redeal limit survives undo.
- Several steps in one `Action` = one undo.

**An action is only reversible against the state it left behind.** Undo out of
order and the board corrupts silently. The undo round-trip test is what protects
this; if you invent a new kind of step, make sure that test exercises it.

## Laying out the board

The layout is the single source of truth for the board's shape: `render.rs`
draws *from* it and mouse clicks resolve *against* it, so a click can never
disagree with what is on screen. You never compute positions yourself — you
describe the board and let `ui/layout.rs` place it.

```rust
Spot::new(pile, row, slot, fan)
```

- **row** — `0` is the top. Rows stack downward with one blank row between.
- **slot** — column along the row, `0`-based. Skipped slots are gaps.
- **fan** — how the pile shows its cards:
  - `Fan::Down` — cards overlap downward; every one is drawn and clickable.
  - `Fan::None` — only the exposed card is drawn.
  - `Fan::Right(n)` — the last `n` cards overlap rightwards. All are drawn, but
    only the exposed one is addressable: a click anywhere in the fan picks up the
    card on top. Klondike's waste uses `Fan::Right(draw_count)` so a draw of
    three can be read at a glance. A covered card keeps its border and three
    cells of its face, which is every rank and suit including a ten's.

A rightward fan spreads into whatever lies to its right, so put it beside a
skipped slot. It costs the board no extra width unless it runs past the widest
row — `board_width` measures from each spot's own left edge, not from the slot
count. A three-card fan is 14 cells wide, so it can end flush against the next
pile rather than a column clear of it; that is a squeeze, not a bug, and the
layout will never let it overlap.

A row containing any `Fan::Down` pile is a *fanned row* and grows to fill the
height left over; every other row is exactly one card tall. Have at most one
fanned row unless you want the height split evenly between them.

What you get automatically:

- **Density** — Roomy (two rows per card), Tight (one), or Tight plus an
  elision that collapses the middle to `⋯N`. Chosen per frame for the whole
  board so piles stay aligned. The exposed card always survives elision.
- **Hit-testing** — resolved on both axes, so a foundation above a tableau pile
  in the same slot is unambiguous. A fanned pile claims every row down to the
  bottom of the board, so clicking below a short pile still targets it.
- **Sizing** — `board_width` and `min_board_height` derive from the spec, so the
  "terminal too small" threshold is correct without being told.
- **Digit keys** — `1`…`9` map to the `Fan::Down` piles in reading order.
- **Labels** — the numbered row is drawn directly above the tableau.

A pile with no `Spot` is not drawn. That is legitimate: Scorpion's stock holds
cards but appears only as a status-bar count.

Note that a `Fan::None` or `Fan::Right` pile's single slot carries the **exposed**
card's index, not zero — so a click on the waste picks up the card you can see.
Cards drawn but not addressable live in `PileLayout::spread`, which the renderer
walks; only `slots` decides what a click can name.

## Rule options

Options appear in the menu's preview pane and as `--rule name=value`. Declare
them as a `const`, store the selected indices on your struct, and read them when
dealing:

```rust
const OPTIONS: [OptionSpec; 2] = [
    OptionSpec { label: "Draw",    values: &["3", "1"] },
    OptionSpec { label: "Redeals", values: &["unlimited", "3", "none"] },
];

const DRAW_COUNTS: [usize; 2] = [3, 1];

/// The first value of every option is its default.
#[derive(Default)]
pub struct Klondike {
    selected: [usize; OPTIONS.len()],
}

impl Rules for Klondike {
    fn options(&self) -> &[OptionSpec] { &OPTIONS }
    fn selected(&self) -> Vec<usize> { self.selected.to_vec() }

    fn configure(&mut self, index: usize, delta: i32) {
        if let Some(slot) = self.selected.get_mut(index) {
            let count = OPTIONS[index].values.len() as i32;
            *slot = (*slot as i32 + delta).rem_euclid(count) as usize;
        }
    }
}
```

`configure` must wrap in both directions — `rem_euclid` handles negative
`delta`, which is what `←` sends. Ordering the first value as the default keeps
`#[derive(Default)]` honest.

`rules::set_option` drives `configure` by name for `--rule`, so you never parse
strings yourself.

## A second game that is nearly the first

Several variants differ from an existing game by a single predicate. You have two
ways to ship one, and they are not equivalent:

- **A rule option**, if the two are settings of one game — Klondike's draw of one
  and draw of three.
- **A second file**, if the two have their own names and their own reputations.
  Russian Solitaire is Yukon built in suit; a player looking for it will look for
  it in the menu, not under a flag on something else.

For the second, put the shared half in the first game's module as `pub(super)`
functions and let the new file supply only what differs.
`src/rules/russian.rs` is the whole pattern: a `//!` header, a struct, its own
`name`/`blurb`, and a `Rules` impl whose methods forward to `yukon::…` — with
`check` passing `Build::SameSuit` where Yukon passes `Build::AlternatingColour`.

Keep the divergence to one named type. An `enum` with a method that both games
call, rather than a `bool` threaded through several functions, is what stops the
second game slowly acquiring a second copy of the first one's rules.

`src/rules/spiderette.rs` is the second instance, and shows the shape scales
past a single predicate: Spiderette differs from Spider in deck count, column
count and suit count, and all three live in one `Copy` `Setup` struct passed as
one first argument. The test that the split is honest is that `spider::check`
takes no `Setup` at all — the build rule, the lift rule and the empty-column
rule really are one game's.

Its tests should cover only what differs — the build rule, and an assertion that
the deals really are identical — and lean on the first game's suite for the rest.

## Testing a new game

Registering the game immediately subjects it to the generic tests in
`src/rules/mod.rs`, which check every registered game:

- names are unique and findable
- `deal` is seed-stable, and different seeds differ
- a deal is a whole number of full decks, is not already won, and has an opening
  move
- every suggestion is a legal move
- suggestions are stable for the same position
- every spot names a real pile and no two share a place
- every declared option can be set by name, and bad values are rejected

Write on top of that, in your own module:

1. **`deal_has_the_right_shape`** — pile sizes, face-down counts, and 52 (or
   104) distinct cards.
2. **`the_deal_is_pinned_to_its_seed`** — assert a few concrete cards. This is
   what stops a refactor silently changing what `--seed 42` deals.
3. **One test per rule** — what lands where, what does not, and the error each
   refusal returns. Build positions by hand with a local helper:

   ```rust
   fn board(tableau: &[Pile], foundations: &[Pile]) -> Board {
       let mut piles: Vec<Pile> = tableau.to_vec();
       piles.resize(TABLEAU, Pile::tableau(0, &[]));
       piles.extend_from_slice(foundations);
       piles.resize(TABLEAU + FOUNDATIONS, Pile::new(PileKind::Foundation));
       Board::new(0, piles)
   }
   ```

4. **`undo_round_trips_a_played_game`** — the most valuable test you will write.
   Play a few hundred moves deterministically, undo everything, assert the board
   equals the opening deal:

   ```rust
   #[test]
   fn undo_round_trips_a_played_game() {
       let start = Yukon.deal(2024);
       let mut board = start.clone();
       let mut history = Vec::new();

       for step in 0..400 {
           let moves = Yukon.suggestions(&board);
           if moves.is_empty() {
               break;
           }
           let action = Yukon.action_for(&board, moves[(step * 7 + 3) % moves.len()]);
           board.apply(&action);
           history.push(action);
       }

       assert!(history.len() > 20, "played {} moves", history.len());
       while let Some(action) = history.pop() {
           board.undo(&action);
       }
       assert_eq!(board, start);
   }
   ```

   The `(step * 7 + 3) % len` choice is deterministic but varied, so the walk
   explores flips, empties and multi-card tails rather than the same move every
   time. If your game has a stock, deal from it when the board runs dry so the
   walk covers those steps too.

5. **A card-conservation test** if the game moves cards in unusual ways —
   collect every card on the board after each move and assert it never changes.
   A `HashSet` is right for a single-deck game; a two-deck game needs a sorted
   multiset keyed on `(suit.index(), rank)`, because a set collapses the
   duplicate and would pass while cards went missing.

Tests are named as sentences, and live in a `#[cfg(test)] mod tests` in the same
file. Run one with `cargo test only_a_king`, and print a board mid-test with
`cargo test <name> -- --nocapture`.

## Checklist

- [ ] `src/rules/<game>.rs` with a `//!` header listing the rules
- [ ] Pile ids as constants, tableau first
- [ ] `deal` pure in `seed`, piles built in id order
- [ ] `spec` covering every pile you want drawn
- [ ] `check` rejecting wrong pile kinds, not just wrong cards
- [ ] `is_won`
- [ ] `rank` and `is_useful` so hints are sensible
- [ ] `has_progress_move` if the game has a stock
- [ ] `activate` / `send_home` / `autoplay` if the game wants those keys (`f`'s
      column half comes free from `check` — nothing to write)
- [ ] `help` listing only the keys this game adds
- [ ] `status` for any counter worth showing
- [ ] `pub mod` + registry line in `src/rules/mod.rs`
- [ ] Tests: deal shape, pinned seed, one per rule, undo round-trip
- [ ] `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check`
- [ ] README's game list and key table updated

## Traps

**Forgetting to reject pile kinds in `check`.** `suggestions` tries every
`(from, index, to)` triple on the board. If you only compare ranks and suits,
you will find the hint system proposing moves onto the stock. Every shipped game
rejects bad kinds explicitly, first thing — and in Spider's case that one guard
is also what keeps the player from filing a run by hand.

**A `check` that is expensive.** It runs `O(piles² × cards)` times per hint
press. Keep it to comparisons.

**`autoplay` that never returns `None`.** `Session::finish` loops until it does.

**Face-down cards landing on a face-up pile.** `face_up: false` is only valid
onto an entirely face-down pile. If you need to hide cards some other way,
rethink the step rather than reaching past the invariant.

**Changing an existing deal.** Seeds are a product guarantee: `--seed 42` deals
what it has always dealt. The pinned-seed tests exist to catch this, and the
splitmix64 shuffle in `card.rs` must not be swapped for `rand`.

**Two decks.** `shuffled_decks(seed, decks)` in `card.rs` deals any number of
decks, and `shuffled_decks(seed, 1)` is asserted equal to `shuffled_deck(seed)`,
so two-deck games arrived without moving a single existing deal. What survives
is the discipline: `Card` derives `Eq` and `Hash`, and with two decks a value no
longer names a single card. A `HashSet<Card>` card-conservation test will
happily pass while half the cards go missing — count a sorted multiset keyed on
`(suit.index(), rank)` instead, as `spider.rs`'s `census` helper does. Nothing
in `app.rs`, `render.rs`, `layout.rs` or `board.rs` identifies a card by value;
keep it that way.

**Ten tableau piles.** `0` is the tenth digit shortcut, and `draw_labels`
numbers the tenth pile `0` so the label *is* the key. `board_width` already
sizes a ten-column board at 69 cells, which fits an 80-column terminal.

**Cards that leave play.** If a move can cause something else to happen — a
finished Spider run leaving the tableau — the extra `Step`s must be appended to
the *same* `Action`, or the player gets two undos for one move and `app.rs` has
to learn what a run is. Detection needs the board *after* the move, so build
those steps against a scratch clone: each step's flags are then computed against
the state it will actually meet, which is exactly what `Board::undo` assumes.
`spider.rs`'s `with_filing` is the pattern, and the entry points to cover are
`action_for` *and* `activate` — dealing a card can finish a run too.

**A new board geometry.** Rows of slots cover every game currently planned
except the triangular ones. Pyramid also needs a different *gesture* — pick two
cards rather than pick up and drop — which is `app.rs` work, not rules work. See
[`future-games.md`](future-games.md) for what each candidate would cost.
