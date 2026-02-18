# MinTransistor Context-DP Extractor

## Scope
This document describes the implementation in
`src/extractors/min_transistor_context_dp_extractor.rs`.

It covers the two entrypoints exported by `extractor`:
- `extract_min_transistor_expr_joinlike_context_dp`
- `extract_min_transistor_expr_any_root_context_dp`

Both return `Result<MinTransistorResult, OptimizationError>`.

## High-Level Design
The context-DP extractor uses dynamic programming on `(eclass, context)` states.
Each state stores the best local decision as:

```text
BestState {
  cost: u64,
  size: usize,
  choice: { node_idx, child_contexts }
}
```

State key:

```text
BestKey = (Id, ExtractContext)
```

Contexts:
- `Cell`
- `Network`
- `Concat`

## Analysis Mode Switch
`analyze_best` selects strategy by env var:
- `CONTEXT_DP_ANALYZE_MODE=worklist` -> `analyze_best_worklist`
- otherwise -> `analyze_best_fullscan`

Both modes compute the same `BestMap` target, but with different update order.

## Cost Model in Candidate Construction
`Size` nodes are skipped in all contexts.

### Cell context
- `Var` / `Bool`: `cost = 0`, `size = 1`
- `Join(left, right)`:
  - each child can be `Cell` or `Network`
  - child picked by lower effective cost
  - effective child cost adds `+1` when child is `Cell`
  - join node itself does not add extra fixed cost here
- `Inv(child)`:
  - child must be `Cell`
  - adds `+2`
- `Concat(left, right)`:
  - both children must be `Cell`
  - no extra operator cost

### Network context
- `And` / `Or`:
  - each child can be `Cell` or `Network`
  - `Cell` child contributes effective `+1`
- `Bridge(a,b,c,d,e)`:
  - each operand can be `Cell` or `Network`
  - total cost is the sum of child effective costs

### Concat context
- only `Concat(left, right)` is valid
- both children are evaluated in `Cell`

## Tie-Breaking Rules
- For child selection (`pick_best_child`):
  1. lower `cost`
  2. if tied, smaller `size`
- For state replacement (`pick_better`, `is_better_option`):
  1. lower `cost`
  2. if tied, smaller `size`
- For any-root final root choice (`pick_best_state`):
  1. lower `cost`
  2. if tied, smaller `size`
  3. if still tied, prefer `Cell` (because `<=` branch keeps cell)

## Fullscan Strategy
`analyze_best_fullscan` runs fixed-point rounds:
1. iterate all eclasses and all contexts
2. recompute local best candidate from enodes
3. update map if improved
4. repeat until no state changes

## Worklist Strategy
`analyze_best_worklist` builds a dependency graph:
- `build_dependents` maps each dependency key to keys that depend on it.
- queue is initialized with every `(eclass, context)` key.
- when a key improves, its dependents are pushed back to queue.

Dependencies are context-specific:
- cell/network gate operands depend on both child contexts
- concat depends on child cell contexts
- inv depends on child cell context

## Expression Reconstruction
After `BestMap` is finalized:
- `build_expr` reconstructs a `RecExpr` by following stored `choice`.
- `build_expr_inner` uses a visiting set to guard cycles.
- if reconstruction fails at any step, extraction returns
  `OptimizationError::NoJoinLikeRootFound`.

## Dedup Cost Adjustment
Final returned cost is post-processed by `dedup_adjusted_cost`.

Mechanism:
1. walk the chosen tree
2. collect join/inv dedup stats keyed by:
   - `(DedupOp, eclass_id, node_idx)`
3. each key tracks `(sum, min)`
4. saving for each key is `sum - min`
5. final cost is `raw_cost - total_saving`

Dedup contributions:
- `Join`: `gate_cost(left_is_cell) + gate_cost(right_is_cell)`
- `Inv`: `2`

`gate_cost(cell)` is `1`; `gate_cost(network)` is `0`.

## Root Handling
- Join-like entrypoint:
  - if root eclass contains concat enode, use `Concat` context
  - else use `Cell` context only
- Any-root entrypoint:
  - concat root -> `Concat`
  - otherwise pick best between root `Cell` and root `Network`

In both entrypoints, `_expected_outputs` is currently unused.

## Known Limitations
- The Deduplication is only in the post-process cost output, but in the algorithm, duplications are assumed. This leads to sub-optimal result.
- `_expected_outputs` is ignored.
- Dedup key includes `node_idx`; structurally equivalent enodes in different
  positions are not merged by dedup unless keys match.
- Strategy auto-selection is not implemented yet; mode is env-driven only.
