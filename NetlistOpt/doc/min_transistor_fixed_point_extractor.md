# MinTransistor Fixed-Point Extractor

## Scope
This document describes the implementation in
`src/extractors/min_transistor_fixed_point_extractor.rs`.

The two public entrypoints exported by `extractor` are:
- `extract_min_transistor_expr`
- `extract_min_transistor_expr_any_root`

Both return `Result<MinTransistorResult, OptimizationError>`.

## Current Architecture
The file uses a single strategy:
- Fixed-point solver (`AnyRootFixedPointSolver`) with context-aware memoization.

`extract_min_transistor_expr` starts in `JoinlikeRoot` context.
`extract_min_transistor_expr_any_root` starts in `AnyRoot` context.

## Core Data Model

### Context Domain
Each eclass is evaluated under one of these contexts:
- `AnyRoot`
- `JoinlikeRoot`
- `Cell`
- `Network`
- `ConcatRoot`

The solver key is `(Id, AnyRootContext)`.

### Candidate
A candidate carries:
- `expr: RecExpr<TransLog>`
- `cost: u64`
- `dedup: HashMap<(DedupOp, Id, EnodeId), u64>`

`DedupOp` is one of `Join` or `Inv`.

### Evaluation Result
`EvalResult`:
- `candidate: Option<Candidate>`
- `exact: bool`

`exact = false` is used for cycle-breaking fallback states.

### Memo Tables (Clear Semantics)
All memo tables are keyed by `(Id, AnyRootContext)`, but they serve different purposes:

- `round_memo`
  - stores full `EvalResult` for the current round;
  - can contain `candidate=None` and/or `exact=false`;
  - prevents repeated recursion and repeated cycle handling in the same round.

- `exact_memo`
  - stores only exact `Candidate` values for the current round;
  - never stores inexact states;
  - used as an exact-only semantic cache for exact reuse.

- `fallback_memo`
  - persists across rounds;
  - stores best known candidate so far plus exactness flag;
  - is the convergence carrier of the fixed-point loop.

Round lifetime:
- `round_memo` and `exact_memo` are cleared at the beginning of each round.
- `fallback_memo` is not cleared between rounds.

Lookup priority in `eval`:
1. `round_memo`
2. `exact_memo`
3. exact entry in `fallback_memo`
4. cycle handling (`visiting` + fallback/inexact miss path)

## Fixed-Point Algorithm

### Round-Level Solver
The solver iterates rounds until no fallback entry improves, or max rounds reached.

```text
solve(root):
  root = find(root)
  repeat at most ANYROOT_MAX_ROUNDS:
    clear round_memo
    clear exact_memo
    clear visiting
    changed = false
    eval(root, root_ctx)
    if changed == false:
      break
  return fallback_memo[(root, root_ctx)]?.candidate
```

`changed` is set only when `update_fallback` inserts/improves/promotes an entry.

### Recursive Evaluation (`eval`)

```text
eval(id, ctx):
  key = (find(id), ctx)

  if key in round_memo: return round_memo[key]
  if key in exact_memo: return {candidate: exact_memo[key], exact: true}
  if key in fallback_memo and fallback_memo[key].exact:
    return {candidate: fallback_memo[key].candidate, exact: true}

  if key in visiting:
    if key in fallback_memo:
      return {candidate: fallback_memo[key].candidate, exact: fallback_memo[key].exact}
    else:
      cache and return {candidate: None, exact: false}

  visiting.insert(key)
  result = dispatch by ctx
  visiting.remove(key)

  if result.candidate exists:
    update_fallback(key, result.candidate, result.exact)
    if result.exact: exact_memo[key] = result.candidate

  round_memo[key] = result
  return result
```

Cycle policy:
- Prefer fallback answer (exact or inexact) if present.
- Otherwise return `(None, inexact)` for this round.

Why both `round_memo` and `exact_memo` exist:
- They are not equivalent caches.
- `round_memo` is the authoritative per-round cache for all outcomes, including temporary inexact states caused by cycle breaking.
- `exact_memo` is a strict exact-only cache used for explicit exact semantics and exact reuse.
- Keeping both makes exactness intent explicit while preserving correct per-round cycle behavior.

Final-result exactness boundary:
- The public API returns only `MinTransistorResult { expr, cost }`, without exposing `exact`.
- `solve` returns the best entry from `fallback_memo`, so the returned result can be inexact when no exact fixed point is found within rounds.

## Context Transfer Functions

### `AnyRoot`
- If root eclass contains a `Concat`, delegate to `ConcatRoot`.
- Otherwise evaluate both `Cell` and `Network`, choose lower cost.
- If only one side exists, choose that side.

### `JoinlikeRoot`
- If root eclass contains a `Concat`, delegate to `ConcatRoot`.
- Otherwise evaluate as `Cell` only.

### `ConcatRoot`
For each `Concat([left, right])` enode in the eclass:
1. Evaluate `left` in `Cell`.
2. Evaluate `right` in `Cell`.
3. Prune by `lower_bound_concat_cost(left, right)` against incumbent.
4. Build candidate with `build_binary_candidate`.
5. Merge into best via `pick_better_with_exact`.

### `Cell`
For each enode in eclass:
- `Var` / `Bool`: literal candidate, cost `0`.
- `Size([child, size_id])`:
  - evaluate `child` in `Cell`;
  - pick best size literal expression from size eclass;
  - candidate cost follows `child` cost.
- `Join([left, right])`:
  - choose each child by `choose_cell_or_network`;
  - prune with `lower_bound_join_partial` and `lower_bound_join_cost`;
  - compose binary candidate;
  - apply join dedup policy (instance-level in `JoinlikeRoot`, standard in `AnyRoot`).
- `Inv(child)`:
  - evaluate child in `Cell`;
  - prune by `lower_bound_inv_cost`;
  - compose unary candidate;
  - apply inv dedup policy (instance-level in `JoinlikeRoot`, standard in `AnyRoot`).
- `Concat([left, right])`:
  - evaluate both children in `Cell`;
  - prune by `lower_bound_concat_cost`;
  - compose binary candidate.

### `Network`
Skip `Size` enodes.

For each remaining enode:
- `And([left, right])` / `Or([left, right])`:
  - choose children by `choose_cell_or_network`;
  - prune with `lower_bound_gate_partial` and `lower_bound_gate_cost`;
  - compose binary candidate;
  - add gate contribution from chosen child kinds.
- `Bridge([a, b, c, d, e])`:
  - choose each child by `choose_cell_or_network`;
  - prune by `lower_bound_bridge_cost`;
  - compose bridge candidate;
  - add gate contribution from all 5 chosen child kinds.

### Child Selection: `choose_cell_or_network`
For one child id:
1. Evaluate `Cell` and `Network`.
2. If both exist:
   - `cell_total = cell_candidate.cost + cell_weight`
   - `network_total = network_candidate.cost`
   - choose cell on tie.
3. If only one exists, use it.

`cell_weight` is derived from the root node of candidate expression:
- `Join` / `Inv` / `Concat` / `Var` / `Bool`: `1`
- `Size([_, size])`: parsed decimal `size` (must be > 0)
- Others: `0`

## Cost and Dedup Semantics

### Base Operator Costs
- `Var` / `Bool`: `0`
- `Join` (Cell): child costs + gate contribution, then dedup policy
- `Inv` (Cell): child cost + `2`, then dedup policy
- `And` / `Or` (Network): child costs + gate contribution
- `Bridge` (Network): five child costs + gate contribution
- `Concat`: structural composition only
- `Size([child, size])`: preserves expression, cost follows `child`

### Candidate Merge
When combining subtrees, `merge_dedup_cost_maps` does:
1. Start from `base_cost + other_cost`.
2. For each overlapping dedup key:
   - subtract the larger of the two shared costs,
   - keep the smaller cost in map.

This preserves cross-subtree sharing for identical dedup keys.

### Dedup Update Rules
Standard mode (`AnyRoot`):
- `add_dedup_entry(candidate, key, value)`:
  - if key absent: insert and add `value` to `candidate.cost`;
  - if key present and `value` is smaller: replace and adjust cost down.

Instance-level mode (`JoinlikeRoot`):
- First add raw operator increment into `candidate.cost`.
- Then `add_instance_dedup_entry(candidate, key)` stores shareable floor marker:
  - `dedup[key] = candidate_floor_cost(candidate)`.

## Pruning Lower Bounds
Define:
- `dedup_total_cost(c) = sum(c.dedup.values())`
- `candidate_floor_cost(c) = c.cost - dedup_total_cost(c)`

Bounds used in pruning:
- `lower_bound_join_partial(left, wl) = floor(left) + wl`
- `lower_bound_join_cost(left, wl, right, wr) = floor(left) + floor(right) + wl + wr`
- `lower_bound_gate_partial(left, wl) = floor(left) + wl`
- `lower_bound_gate_cost(left, wl, right, wr) = lower_bound_join_cost(...)`
- `lower_bound_bridge_cost([(ci, wi)] x 5) = sum_i (floor(ci) + wi)`
- `lower_bound_inv_cost(child) = floor(child) + 2`
- `lower_bound_concat_cost(left, right) = floor(left) + floor(right)`

A candidate path is skipped when lower bound is not better than current incumbent.

## Candidate Ordering and Fallback Update

### Ordering
`is_strictly_better_candidate(next, current)`:
1. Lower cost wins.
2. If equal cost, shorter expression wins.

`pick_better_with_exact` adds one more tie-break:
3. If cost and expression length are equal, prefer `exact = true`.

### Fallback Table Update
`update_fallback(key, candidate, exact)` updates on:
- strictly better candidate (`is_strictly_better_candidate`), or
- exactness promotion for same key (`existing.exact == false && exact == true`).

Each successful update sets `changed = true` for fixed-point convergence control.

## Minimal Cycle Example
Assume one cyclic key `K = (id, Cell)` where `K` recursively depends on itself:

Round 1:
1. `eval(K)` enters `visiting`.
2. A recursive call reaches `K` again.
3. `K` is in `visiting` and `fallback_memo[K]` is absent.
4. Return and cache `(candidate=None, exact=false)` for this round branch.
5. Another non-cyclic branch may still produce a concrete candidate `C` for `K`.
6. `update_fallback(K, C, exact=false or true)` stores the best known fallback.

Round 2:
1. `eval(K)` may encounter the same cycle again.
2. Now `fallback_memo[K]` exists, so cycle handling can return that fallback candidate instead of `None`.
3. This can unlock deeper compositions and possibly produce a better and/or exact candidate.
4. If an exact promotion or better candidate occurs, `changed=true` and rounds continue; otherwise fixed point is reached.

## Optional Runtime Stats
Set environment variable `MIN_TRANS_REFACTOR_STATS` to print:
- rounds, eval calls, memo hit counters,
- fallback hit/miss/update counters,
- candidate considered/pruned counters,
- final root cost.

## Error Conditions
The extractor may return:
- `OptimizationError::NoJoinLikeRootFound` when no valid root candidate exists.
- `OptimizationError::InvalidSizeLiteral` when a size literal is non-decimal or zero.

## Complexity Notes
Let `K = #eclass x #contexts` keys and `R <= ANYROOT_MAX_ROUNDS`.
- Worst-case evaluation work is bounded by repeated key evaluation across rounds.
- Per-key work depends on eclass arity and branching (`Bridge` is highest fan-in).
- Memoization and pruning reduce practical cost significantly on cyclic/shared egraphs.

## Limitations
- `_expected_outputs` is currently ignored by both entrypoints.
- Dedup is keyed by `(DedupOp, eclass id, enode id)`.
  Equivalent structures in different enodes do not deduplicate unless keys match.
