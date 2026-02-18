use std::collections::{HashMap, HashSet, VecDeque};

use egg::{EGraph, Id, Language, RecExpr};

use crate::language::TransLog;

use super::{MinTransistorResult, OptimizationError};

// TODO(strategy-auto-select): Profiling indicates worklist is noticeably faster on
// larger/more complex expressions, while smaller cases can still favor fullscan.
// Add an empirical e-graph size heuristic (for example, classes/nodes threshold)
// to automatically choose analyze_best_worklist vs analyze_best_fullscan.

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum ExtractContext {
    Cell,
    Network,
    Concat,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum DedupOp {
    Join,
    Inv,
}

type DedupKey = (DedupOp, Id, usize);

#[derive(Clone, Debug, PartialEq, Eq)]
struct Choice {
    node_idx: usize,
    child_contexts: Vec<ExtractContext>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BestState {
    cost: u64,
    size: usize,
    choice: Choice,
}

type BestKey = (Id, ExtractContext);
type BestMap = HashMap<BestKey, BestState>;

#[derive(Clone, Copy, Debug)]
struct ChildPick {
    context: ExtractContext,
    cost: u64,
    size: usize,
}

pub fn extract_min_transistor_expr_joinlike_context_dp(
    egraph: &EGraph<TransLog, ()>,
    root: Id,
    _expected_outputs: usize,
) -> Result<MinTransistorResult, OptimizationError> {
    let best = analyze_best(egraph);
    let root = egraph.find(root);
    let root_context = if is_concat_root(egraph, root) {
        ExtractContext::Concat
    } else {
        ExtractContext::Cell
    };
    let best_state = best
        .get(&(root, root_context))
        .ok_or(OptimizationError::NoJoinLikeRootFound)?;
    let expr = build_expr(egraph, root, root_context, &best)
        .ok_or(OptimizationError::NoJoinLikeRootFound)?;
    let cost = dedup_adjusted_cost(egraph, root, root_context, &best, best_state.cost)
        .unwrap_or(best_state.cost);
    Ok(MinTransistorResult {
        expr,
        cost,
    })
}

pub fn extract_min_transistor_expr_any_root_context_dp(
    egraph: &EGraph<TransLog, ()>,
    root: Id,
    _expected_outputs: usize,
) -> Result<MinTransistorResult, OptimizationError> {
    let best = analyze_best(egraph);
    let root = egraph.find(root);
    let (root_context, best_state) = if is_concat_root(egraph, root) {
        let best_state = best
            .get(&(root, ExtractContext::Concat))
            .ok_or(OptimizationError::NoJoinLikeRootFound)?;
        (ExtractContext::Concat, best_state)
    } else {
        let cell = best.get(&(root, ExtractContext::Cell));
        let network = best.get(&(root, ExtractContext::Network));
        match pick_best_state(cell, network) {
            Some((context, state)) => (context, state),
            None => return Err(OptimizationError::NoJoinLikeRootFound),
        }
    };
    let expr = build_expr(egraph, root, root_context, &best)
        .ok_or(OptimizationError::NoJoinLikeRootFound)?;
    let cost = dedup_adjusted_cost(egraph, root, root_context, &best, best_state.cost)
        .unwrap_or(best_state.cost);
    Ok(MinTransistorResult {
        expr,
        cost,
    })
}

fn analyze_best(egraph: &EGraph<TransLog, ()>) -> BestMap {
    if std::env::var("CONTEXT_DP_ANALYZE_MODE")
        .map(|mode| mode.eq_ignore_ascii_case("worklist"))
        .unwrap_or(false)
    {
        return analyze_best_worklist(egraph);
    }
    analyze_best_fullscan(egraph)
}

fn analyze_best_worklist(egraph: &EGraph<TransLog, ()>) -> BestMap {
    let mut best: BestMap = HashMap::new();
    let dependents = build_dependents(egraph);
    let mut queue: VecDeque<BestKey> = VecDeque::new();
    let mut in_queue: HashSet<BestKey> = HashSet::new();

    for class in egraph.classes() {
        for context in contexts() {
            let key = (class.id, context);
            queue.push_back(key);
            in_queue.insert(key);
        }
    }

    while let Some(key) = queue.pop_front() {
        in_queue.remove(&key);
        let existing = best.get(&key).cloned();
        let candidate = recompute_key(egraph, key, &best);
        if is_better_option(&candidate, &existing) {
            if let Some(state) = candidate {
                best.insert(key, state);
                if let Some(next_keys) = dependents.get(&key) {
                    for next_key in next_keys {
                        if in_queue.insert(*next_key) {
                            queue.push_back(*next_key);
                        }
                    }
                }
            }
        }
    }

    best
}

fn analyze_best_fullscan(egraph: &EGraph<TransLog, ()>) -> BestMap {
    let mut best: BestMap = HashMap::new();
    let mut changed = true;
    while changed {
        changed = false;
        for class in egraph.classes() {
            let id = class.id;
            for context in [
                ExtractContext::Cell,
                ExtractContext::Network,
                ExtractContext::Concat,
            ] {
                let key = (id, context);
                let existing = best.get(&key).cloned();
                let mut candidate = existing.clone();
                for (node_idx, node) in class.nodes.iter().enumerate() {
                    if matches!(node, TransLog::Size(_)) {
                        continue;
                    }
                    if let Some(next) = make_candidate(
                        egraph,
                        id,
                        node_idx,
                        node,
                        context,
                        &best,
                    ) {
                        candidate = pick_better(candidate, next);
                    }
                }
                if is_better_option(&candidate, &existing) {
                    if let Some(state) = candidate {
                        best.insert(key, state);
                        changed = true;
                    }
                }
            }
        }
    }
    best
}

fn contexts() -> [ExtractContext; 3] {
    [
        ExtractContext::Cell,
        ExtractContext::Network,
        ExtractContext::Concat,
    ]
}

fn recompute_key(
    egraph: &EGraph<TransLog, ()>,
    key: BestKey,
    best: &BestMap,
) -> Option<BestState> {
    let (id, context) = key;
    let class = &egraph[id];
    let mut candidate: Option<BestState> = None;
    for (node_idx, node) in class.nodes.iter().enumerate() {
        if matches!(node, TransLog::Size(_)) {
            continue;
        }
        if let Some(next) = make_candidate(egraph, id, node_idx, node, context, best) {
            candidate = pick_better(candidate, next);
        }
    }
    candidate
}

fn build_dependents(
    egraph: &EGraph<TransLog, ()>,
) -> HashMap<BestKey, Vec<BestKey>> {
    let mut dependents: HashMap<BestKey, Vec<BestKey>> = HashMap::new();
    for class in egraph.classes() {
        let id = class.id;
        for context in contexts() {
            let key = (id, context);
            let mut deps: HashSet<BestKey> = HashSet::new();
            for node in &class.nodes {
                collect_node_dependencies(egraph, context, node, &mut deps);
            }
            for dep in deps {
                dependents.entry(dep).or_default().push(key);
            }
        }
    }
    dependents
}

fn collect_node_dependencies(
    egraph: &EGraph<TransLog, ()>,
    context: ExtractContext,
    node: &TransLog,
    deps: &mut HashSet<BestKey>,
) {
    match context {
        ExtractContext::Cell => match node {
            TransLog::Join([left, right]) => {
                collect_gate_operand_dependencies(egraph, *left, deps);
                collect_gate_operand_dependencies(egraph, *right, deps);
            }
            TransLog::Inv(child) => {
                deps.insert((egraph.find(*child), ExtractContext::Cell));
            }
            TransLog::Concat([left, right]) => {
                deps.insert((egraph.find(*left), ExtractContext::Cell));
                deps.insert((egraph.find(*right), ExtractContext::Cell));
            }
            _ => {}
        },
        ExtractContext::Network => match node {
            TransLog::And([left, right]) | TransLog::Or([left, right]) => {
                collect_gate_operand_dependencies(egraph, *left, deps);
                collect_gate_operand_dependencies(egraph, *right, deps);
            }
            TransLog::Bridge([a, b, c, d, e]) => {
                collect_gate_operand_dependencies(egraph, *a, deps);
                collect_gate_operand_dependencies(egraph, *b, deps);
                collect_gate_operand_dependencies(egraph, *c, deps);
                collect_gate_operand_dependencies(egraph, *d, deps);
                collect_gate_operand_dependencies(egraph, *e, deps);
            }
            _ => {}
        },
        ExtractContext::Concat => {
            if let TransLog::Concat([left, right]) = node {
                deps.insert((egraph.find(*left), ExtractContext::Cell));
                deps.insert((egraph.find(*right), ExtractContext::Cell));
            }
        }
    }
}

fn collect_gate_operand_dependencies(
    egraph: &EGraph<TransLog, ()>,
    id: Id,
    deps: &mut HashSet<BestKey>,
) {
    let id = egraph.find(id);
    deps.insert((id, ExtractContext::Cell));
    deps.insert((id, ExtractContext::Network));
}

fn make_candidate(
    egraph: &EGraph<TransLog, ()>,
    _class_id: Id,
    node_idx: usize,
    node: &TransLog,
    context: ExtractContext,
    best: &BestMap,
) -> Option<BestState> {
    match context {
        ExtractContext::Cell => match node {
            TransLog::Var(_) | TransLog::Bool(_) => Some(BestState {
                cost: 0,
                size: 1,
                choice: Choice {
                    node_idx,
                    child_contexts: Vec::new(),
                },
            }),
            TransLog::Join([left, right]) => {
                let left_pick = select_gate_operand(egraph, *left, best)?;
                let right_pick = select_gate_operand(egraph, *right, best)?;
                Some(BestState {
                    cost: left_pick.cost.saturating_add(right_pick.cost),
                    size: left_pick
                        .size
                        .saturating_add(right_pick.size)
                        .saturating_add(1),
                    choice: Choice {
                        node_idx,
                        child_contexts: vec![left_pick.context, right_pick.context],
                    },
                })
            }
            TransLog::Inv(child) => {
                let child_id = egraph.find(*child);
                let child = best.get(&(child_id, ExtractContext::Cell))?;
                Some(BestState {
                    cost: child.cost.saturating_add(2),
                    size: child.size.saturating_add(1),
                    choice: Choice {
                        node_idx,
                        child_contexts: vec![ExtractContext::Cell],
                    },
                })
            }
            TransLog::Concat([left, right]) => {
                let left_id = egraph.find(*left);
                let right_id = egraph.find(*right);
                let left = best.get(&(left_id, ExtractContext::Cell))?;
                let right = best.get(&(right_id, ExtractContext::Cell))?;
                Some(BestState {
                    cost: left.cost.saturating_add(right.cost),
                    size: left
                        .size
                        .saturating_add(right.size)
                        .saturating_add(1),
                    choice: Choice {
                        node_idx,
                        child_contexts: vec![ExtractContext::Cell, ExtractContext::Cell],
                    },
                })
            }
            _ => None,
        },
        ExtractContext::Network => match node {
            TransLog::And([left, right])
            | TransLog::Or([left, right]) => {
                let left_pick = select_gate_operand(egraph, *left, best)?;
                let right_pick = select_gate_operand(egraph, *right, best)?;
                Some(BestState {
                    cost: left_pick.cost.saturating_add(right_pick.cost),
                    size: left_pick
                        .size
                        .saturating_add(right_pick.size)
                        .saturating_add(1),
                    choice: Choice {
                        node_idx,
                        child_contexts: vec![left_pick.context, right_pick.context],
                    },
                })
            }
            TransLog::Bridge([a, b, c, d, e]) => {
                let pa = select_gate_operand(egraph, *a, best)?;
                let pb = select_gate_operand(egraph, *b, best)?;
                let pc = select_gate_operand(egraph, *c, best)?;
                let pd = select_gate_operand(egraph, *d, best)?;
                let pe = select_gate_operand(egraph, *e, best)?;
                Some(BestState {
                    cost: pa
                        .cost
                        .saturating_add(pb.cost)
                        .saturating_add(pc.cost)
                        .saturating_add(pd.cost)
                        .saturating_add(pe.cost),
                    size: pa
                        .size
                        .saturating_add(pb.size)
                        .saturating_add(pc.size)
                        .saturating_add(pd.size)
                        .saturating_add(pe.size)
                        .saturating_add(1),
                    choice: Choice {
                        node_idx,
                        child_contexts: vec![
                            pa.context, pb.context, pc.context, pd.context, pe.context,
                        ],
                    },
                })
            }
            _ => None,
        },
        ExtractContext::Concat => match node {
            TransLog::Concat([left, right]) => {
                let left_id = egraph.find(*left);
                let right_id = egraph.find(*right);
                let left = best.get(&(left_id, ExtractContext::Cell))?;
                let right = best.get(&(right_id, ExtractContext::Cell))?;
                Some(BestState {
                    cost: left.cost.saturating_add(right.cost),
                    size: left
                        .size
                        .saturating_add(right.size)
                        .saturating_add(1),
                    choice: Choice {
                        node_idx,
                        child_contexts: vec![ExtractContext::Cell, ExtractContext::Cell],
                    },
                })
            }
            _ => None,
        },
    }
}

fn select_gate_operand(
    egraph: &EGraph<TransLog, ()>,
    id: Id,
    best: &BestMap,
) -> Option<ChildPick> {
    let id = egraph.find(id);
    let cell = best.get(&(id, ExtractContext::Cell)).map(|state| ChildPick {
        context: ExtractContext::Cell,
        cost: state.cost.saturating_add(1),
        size: state.size,
    });
    let network = best
        .get(&(id, ExtractContext::Network))
        .map(|state| ChildPick {
            context: ExtractContext::Network,
            cost: state.cost,
            size: state.size,
        });
    pick_best_child(cell, network)
}

fn pick_best_child(
    left: Option<ChildPick>,
    right: Option<ChildPick>,
) -> Option<ChildPick> {
    match (left, right) {
        (Some(a), Some(b)) => {
            if a.cost < b.cost || (a.cost == b.cost && a.size <= b.size) {
                Some(a)
            } else {
                Some(b)
            }
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn pick_better(current: Option<BestState>, next: BestState) -> Option<BestState> {
    match current {
        None => Some(next),
        Some(existing) => {
            if next.cost < existing.cost
                || (next.cost == existing.cost && next.size < existing.size)
            {
                Some(next)
            } else {
                Some(existing)
            }
        }
    }
}

fn is_better_option(
    next: &Option<BestState>,
    current: &Option<BestState>,
) -> bool {
    match (next, current) {
        (Some(_), None) => true,
        (Some(n), Some(c)) => {
            n.cost < c.cost || (n.cost == c.cost && n.size < c.size)
        }
        _ => false,
    }
}

fn pick_best_state<'a>(
    cell: Option<&'a BestState>,
    network: Option<&'a BestState>,
) -> Option<(ExtractContext, &'a BestState)> {
    match (cell, network) {
        (Some(cell_state), Some(net_state)) => {
            if cell_state.cost < net_state.cost
                || (cell_state.cost == net_state.cost
                    && cell_state.size <= net_state.size)
            {
                Some((ExtractContext::Cell, cell_state))
            } else {
                Some((ExtractContext::Network, net_state))
            }
        }
        (Some(cell_state), None) => Some((ExtractContext::Cell, cell_state)),
        (None, Some(net_state)) => Some((ExtractContext::Network, net_state)),
        (None, None) => None,
    }
}

fn build_expr(
    egraph: &EGraph<TransLog, ()>,
    id: Id,
    context: ExtractContext,
    best: &BestMap,
) -> Option<RecExpr<TransLog>> {
    let mut visiting: HashSet<BestKey> = HashSet::new();
    build_expr_inner(egraph, id, context, best, &mut visiting)
}

fn build_expr_inner(
    egraph: &EGraph<TransLog, ()>,
    id: Id,
    context: ExtractContext,
    best: &BestMap,
    visiting: &mut HashSet<BestKey>,
) -> Option<RecExpr<TransLog>> {
    let id = egraph.find(id);
    let key = (id, context);
    if !visiting.insert(key) {
        return None;
    }
    let state = best.get(&key)?;
    let node = &egraph[id].nodes[state.choice.node_idx];
    let mut child_exprs = Vec::new();
    for (child_id, child_context) in node
        .children()
        .iter()
        .zip(state.choice.child_contexts.iter())
    {
        let expr = build_expr_inner(
            egraph,
            *child_id,
            *child_context,
            best,
            visiting,
        )?;
        child_exprs.push(expr);
    }
    let expr = build_node_expr(node, &child_exprs);
    visiting.remove(&key);
    Some(expr)
}

fn build_node_expr(
    node: &TransLog,
    child_exprs: &[RecExpr<TransLog>],
) -> RecExpr<TransLog> {
    let mut expr = RecExpr::default();
    let mut child_ids = Vec::new();
    for child in child_exprs {
        let id = append_subexpr(&mut expr, child);
        child_ids.push(id);
    }
    let mut new_node = node.clone();
    for (slot, id) in new_node.children_mut().iter_mut().zip(child_ids) {
        *slot = id;
    }
    expr.add(new_node);
    expr
}

fn append_subexpr(
    target: &mut RecExpr<TransLog>,
    sub: &RecExpr<TransLog>,
) -> Id {
    let offset = target.as_ref().len();
    for node in sub.as_ref() {
        let mut new_node = node.clone();
        for child in new_node.children_mut() {
            let old_index: usize = usize::from(*child);
            *child = Id::from(old_index + offset);
        }
        target.add(new_node);
    }
    Id::from(target.as_ref().len().saturating_sub(1))
}

fn dedup_adjusted_cost(
    egraph: &EGraph<TransLog, ()>,
    root: Id,
    context: ExtractContext,
    best: &BestMap,
    raw_cost: u64,
) -> Option<u64> {
    let mut visiting: HashSet<BestKey> = HashSet::new();
    let mut stats: HashMap<DedupKey, (u64, u64)> = HashMap::new();
    collect_dedup_stats(
        egraph,
        root,
        context,
        best,
        &mut visiting,
        &mut stats,
    )?;
    let saving = stats
        .values()
        .fold(0u64, |acc, (sum, min)| acc.saturating_add(sum.saturating_sub(*min)));
    Some(raw_cost.saturating_sub(saving))
}

fn collect_dedup_stats(
    egraph: &EGraph<TransLog, ()>,
    id: Id,
    context: ExtractContext,
    best: &BestMap,
    visiting: &mut HashSet<BestKey>,
    stats: &mut HashMap<DedupKey, (u64, u64)>,
) -> Option<()> {
    let id = egraph.find(id);
    let key = (id, context);
    if !visiting.insert(key) {
        return None;
    }
    let state = best.get(&key)?;
    let node = &egraph[id].nodes[state.choice.node_idx];
    match node {
        TransLog::Join(_) => {
            if state.choice.child_contexts.len() < 2 {
                return None;
            }
            let left_is_cell = state.choice.child_contexts[0] == ExtractContext::Cell;
            let right_is_cell = state.choice.child_contexts[1] == ExtractContext::Cell;
            let value = gate_cost(left_is_cell).saturating_add(gate_cost(right_is_cell));
            add_dedup_stat(
                stats,
                (DedupOp::Join, id, state.choice.node_idx),
                value,
            );
        }
        TransLog::Inv(_) => {
            add_dedup_stat(
                stats,
                (DedupOp::Inv, id, state.choice.node_idx),
                2,
            );
        }
        _ => {}
    }
    for (child_id, child_context) in node
        .children()
        .iter()
        .zip(state.choice.child_contexts.iter())
    {
        collect_dedup_stats(
            egraph,
            *child_id,
            *child_context,
            best,
            visiting,
            stats,
        )?;
    }
    visiting.remove(&key);
    Some(())
}

fn add_dedup_stat(
    stats: &mut HashMap<DedupKey, (u64, u64)>,
    key: DedupKey,
    value: u64,
) {
    stats
        .entry(key)
        .and_modify(|entry| {
            entry.0 = entry.0.saturating_add(value);
            entry.1 = entry.1.min(value);
        })
        .or_insert((value, value));
}

fn is_concat_root(egraph: &EGraph<TransLog, ()>, root: Id) -> bool {
    let root = egraph.find(root);
    egraph[root]
        .nodes
        .iter()
        .any(|node| matches!(node, TransLog::Concat(_)))
}

fn gate_cost(is_cell: bool) -> u64 {
    if is_cell {
        1
    } else {
        0
    }
}
