use std::collections::{HashMap, HashSet};

use egg::{EGraph, Id, Language, RecExpr};

use crate::language::TransLog;

use super::{MinTransistorResult, OptimizationError};

#[derive(Clone, Debug)]
struct Candidate {
    expr: RecExpr<TransLog>,
    cost: u64,
    dedup: HashMap<DedupKey, u64>,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum DedupOp {
    Join,
    Inv,
}

type EnodeId = usize;
type DedupKey = (DedupOp, Id, EnodeId);

const ANYROOT_MAX_ROUNDS: usize = 32;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum AnyRootContext {
    AnyRoot,
    JoinlikeRoot,
    Cell,
    Network,
    ConcatRoot,
}

#[derive(Clone, Debug)]
struct EvalResult {
    candidate: Option<Candidate>,
    exact: bool,
}

#[derive(Clone, Debug)]
struct ChildChoice {
    candidate: Candidate,
    gate_weight: u64,
    exact: bool,
}

#[derive(Clone, Debug)]
struct FallbackEntry {
    candidate: Candidate,
    exact: bool,
}

#[derive(Default, Clone, Debug)]
struct SolverStats {
    rounds: usize,
    eval_calls: usize,
    round_memo_hits: usize,
    exact_memo_hits: usize,
    visiting_hits: usize,
    fallback_hits: usize,
    fallback_misses: usize,
    fallback_updates: usize,
    exact_promotions: usize,
    candidates_considered: usize,
    candidates_pruned_by_bound: usize,
}

struct AnyRootFixedPointSolver<'a> {
    egraph: &'a EGraph<TransLog, ()>,
    round_memo: HashMap<(Id, AnyRootContext), EvalResult>,
    exact_memo: HashMap<(Id, AnyRootContext), Candidate>,
    fallback_memo: HashMap<(Id, AnyRootContext), FallbackEntry>,
    visiting: HashSet<(Id, AnyRootContext)>,
    changed: bool,
    stats: SolverStats,
    print_stats: bool,
    stats_label: &'static str,
    root_ctx: AnyRootContext,
}

pub fn extract_min_transistor_expr(
    egraph: &EGraph<TransLog, ()>,
    root: Id,
    _expected_outputs: usize,
) -> Result<MinTransistorResult, OptimizationError> {
    let mut solver = AnyRootFixedPointSolver::new_joinlike(egraph);
    let candidate = solver
        .solve(root)?
        .ok_or(OptimizationError::NoJoinLikeRootFound)?;
    Ok(MinTransistorResult {
        expr: candidate.expr,
        cost: candidate.cost,
    })
}

pub fn extract_min_transistor_expr_any_root(
    egraph: &EGraph<TransLog, ()>,
    root: Id,
    _expected_outputs: usize,
) -> Result<MinTransistorResult, OptimizationError> {
    let mut solver = AnyRootFixedPointSolver::new_any_root(egraph);
    let candidate = solver
        .solve(root)?
        .ok_or(OptimizationError::NoJoinLikeRootFound)?;
    Ok(MinTransistorResult {
        expr: candidate.expr,
        cost: candidate.cost,
    })
}

impl<'a> AnyRootFixedPointSolver<'a> {
    fn new_any_root(egraph: &'a EGraph<TransLog, ()>) -> Self {
        Self::new(egraph, AnyRootContext::AnyRoot, "anyroot")
    }

    fn new_joinlike(egraph: &'a EGraph<TransLog, ()>) -> Self {
        Self::new(egraph, AnyRootContext::JoinlikeRoot, "joinlike")
    }

    fn new(
        egraph: &'a EGraph<TransLog, ()>,
        root_ctx: AnyRootContext,
        stats_label: &'static str,
    ) -> Self {
        Self {
            egraph,
            round_memo: HashMap::new(),
            exact_memo: HashMap::new(),
            fallback_memo: HashMap::new(),
            visiting: HashSet::new(),
            changed: false,
            stats: SolverStats::default(),
            print_stats: std::env::var("MIN_TRANS_REFACTOR_STATS").is_ok(),
            stats_label,
            root_ctx,
        }
    }

    fn solve(&mut self, root: Id) -> Result<Option<Candidate>, OptimizationError> {
        let root = self.egraph.find(root);
        for _ in 0..ANYROOT_MAX_ROUNDS {
            self.stats.rounds = self.stats.rounds.saturating_add(1);
            self.round_memo.clear();
            self.exact_memo.clear();
            self.visiting.clear();
            self.changed = false;
            self.eval(root, self.root_ctx)?;
            if !self.changed {
                break;
            }
        }
        let result = self
            .fallback_memo
            .get(&(root, self.root_ctx))
            .map(|entry| entry.candidate.clone());
        if self.print_stats {
            let root_cost = result.as_ref().map(|candidate| candidate.cost);
            eprintln!(
                "[refactor-{}] rounds={} eval_calls={} round_hits={} exact_hits={} visiting_hits={} fallback_hits={} fallback_misses={} fallback_updates={} exact_promotions={} candidates_considered={} pruned_by_bound={} root_cost={:?}",
                self.stats_label,
                self.stats.rounds,
                self.stats.eval_calls,
                self.stats.round_memo_hits,
                self.stats.exact_memo_hits,
                self.stats.visiting_hits,
                self.stats.fallback_hits,
                self.stats.fallback_misses,
                self.stats.fallback_updates,
                self.stats.exact_promotions,
                self.stats.candidates_considered,
                self.stats.candidates_pruned_by_bound,
                root_cost,
            );
        }
        Ok(result)
    }

    fn eval(
        &mut self,
        root_id: Id,
        ctx: AnyRootContext,
    ) -> Result<EvalResult, OptimizationError> {
        self.stats.eval_calls = self.stats.eval_calls.saturating_add(1);
        let id = self.egraph.find(root_id);
        let key = (id, ctx);

        if let Some(cached) = self.round_memo.get(&key) {
            self.stats.round_memo_hits = self.stats.round_memo_hits.saturating_add(1);
            return Ok(cached.clone());
        }

        if let Some(cached) = self.exact_memo.get(&key) {
            self.stats.exact_memo_hits = self.stats.exact_memo_hits.saturating_add(1);
            return Ok(EvalResult {
                candidate: Some(cached.clone()),
                exact: true,
            });
        }

        if let Some(entry) = self.fallback_memo.get(&key) {
            if entry.exact {
                self.stats.fallback_hits = self.stats.fallback_hits.saturating_add(1);
                return Ok(EvalResult {
                    candidate: Some(entry.candidate.clone()),
                    exact: true,
                });
            }
        }

        if self.visiting.contains(&key) {
            self.stats.visiting_hits = self.stats.visiting_hits.saturating_add(1);
            if let Some(entry) = self.fallback_memo.get(&key) {
                self.stats.fallback_hits = self.stats.fallback_hits.saturating_add(1);
                return Ok(EvalResult {
                    candidate: Some(entry.candidate.clone()),
                    exact: entry.exact,
                });
            }
            self.stats.fallback_misses = self.stats.fallback_misses.saturating_add(1);
            let cycle_miss = EvalResult {
                candidate: None,
                exact: false,
            };
            self.round_memo.insert(key, cycle_miss.clone());
            return Ok(cycle_miss);
        }

        self.visiting.insert(key);
        let result = match ctx {
            AnyRootContext::AnyRoot => self.eval_any_root(id)?,
            AnyRootContext::JoinlikeRoot => self.eval_joinlike_root(id)?,
            AnyRootContext::Cell => self.eval_cell(id)?,
            AnyRootContext::Network => self.eval_network(id)?,
            AnyRootContext::ConcatRoot => self.eval_concat_root(id)?,
        };
        self.visiting.remove(&key);

        if let Some(candidate) = result.candidate.as_ref() {
            self.update_fallback(key, candidate, result.exact);
            if result.exact {
                self.exact_memo.insert(key, candidate.clone());
            }
        }

        self.round_memo.insert(key, result.clone());
        Ok(result)
    }

    fn eval_any_root(&mut self, id: Id) -> Result<EvalResult, OptimizationError> {
        if is_concat_root(self.egraph, id) {
            return self.eval(id, AnyRootContext::ConcatRoot);
        }

        let cell = self.eval(id, AnyRootContext::Cell)?;
        let network = self.eval(id, AnyRootContext::Network)?;
        match (cell.candidate, network.candidate) {
            (Some(cell_candidate), Some(network_candidate)) => {
                if cell_candidate.cost <= network_candidate.cost {
                    Ok(EvalResult {
                        candidate: Some(cell_candidate),
                        exact: cell.exact,
                    })
                } else {
                    Ok(EvalResult {
                        candidate: Some(network_candidate),
                        exact: network.exact,
                    })
                }
            }
            (Some(cell_candidate), None) => Ok(EvalResult {
                candidate: Some(cell_candidate),
                exact: cell.exact,
            }),
            (None, Some(network_candidate)) => Ok(EvalResult {
                candidate: Some(network_candidate),
                exact: network.exact,
            }),
            (None, None) => Ok(EvalResult {
                candidate: None,
                exact: false,
            }),
        }
    }

    fn eval_joinlike_root(&mut self, id: Id) -> Result<EvalResult, OptimizationError> {
        if is_concat_root(self.egraph, id) {
            return self.eval(id, AnyRootContext::ConcatRoot);
        }
        self.eval(id, AnyRootContext::Cell)
    }

    fn eval_concat_root(&mut self, id: Id) -> Result<EvalResult, OptimizationError> {
        let key = (id, AnyRootContext::ConcatRoot);
        let mut best = self
            .fallback_memo
            .get(&key)
            .map(|entry| entry.candidate.clone());
        let mut best_exact = self.fallback_memo.get(&key).is_some_and(|entry| entry.exact);
        let mut incumbent = best.as_ref().map(|candidate| candidate.cost);
        let nodes = self.egraph[id].nodes.clone();

        for node in &nodes {
            let TransLog::Concat([left, right]) = node else {
                continue;
            };

            let left_result = self.eval(*left, AnyRootContext::Cell)?;
            let Some(left_candidate) = left_result.candidate else {
                continue;
            };
            let right_result = self.eval(*right, AnyRootContext::Cell)?;
            let Some(right_candidate) = right_result.candidate else {
                continue;
            };

            let lower_bound = lower_bound_concat_cost(&left_candidate, &right_candidate);
            if incumbent.is_some_and(|bound| lower_bound >= bound) {
                self.stats.candidates_pruned_by_bound =
                    self.stats.candidates_pruned_by_bound.saturating_add(1);
                continue;
            }

            self.stats.candidates_considered = self.stats.candidates_considered.saturating_add(1);
            let candidate = build_binary_candidate(node, &left_candidate, &right_candidate)?;
            let candidate_exact = left_result.exact && right_result.exact;
            let (next_best, next_exact) = pick_better_with_exact(
                best,
                best_exact,
                candidate,
                candidate_exact,
            );
            best = next_best;
            best_exact = next_exact;
            incumbent = best.as_ref().map(|value| value.cost);
        }

        Ok(EvalResult {
            candidate: best,
            exact: best_exact,
        })
    }

    fn eval_cell(&mut self, id: Id) -> Result<EvalResult, OptimizationError> {
        let key = (id, AnyRootContext::Cell);
        let mut best = self
            .fallback_memo
            .get(&key)
            .map(|entry| entry.candidate.clone());
        let mut best_exact = self.fallback_memo.get(&key).is_some_and(|entry| entry.exact);
        let mut incumbent = best.as_ref().map(|candidate| candidate.cost);
        let nodes = self.egraph[id].nodes.clone();

        for (enode_id, node) in nodes.iter().enumerate() {
            let (candidate, candidate_exact) = match node {
                TransLog::Var(_) | TransLog::Bool(_) => {
                    (Some(build_literal_candidate(node)?), true)
                }
                TransLog::Size([child, size_id]) => {
                    let child_result = self.eval(*child, AnyRootContext::Cell)?;
                    let Some(child_candidate) = child_result.candidate else {
                        continue;
                    };
                    let size_lit = best_size_literal_expr_in_eclass(self.egraph, *size_id)?;
                    let Some(size_lit) = size_lit else {
                        continue;
                    };
                    self.stats.candidates_considered =
                        self.stats.candidates_considered.saturating_add(1);
                    let candidate = build_size_candidate(node, &child_candidate, &size_lit.expr)?;
                    (Some(candidate), child_result.exact)
                }
                TransLog::Join([left, right]) => {
                    let Some(left_choice) = self.choose_cell_or_network(*left)? else {
                        continue;
                    };
                    if incumbent.is_some_and(|bound| {
                        lower_bound_join_partial(&left_choice.candidate, left_choice.gate_weight)
                            >= bound
                    }) {
                        self.stats.candidates_pruned_by_bound =
                            self.stats.candidates_pruned_by_bound.saturating_add(1);
                        continue;
                    }
                    let Some(right_choice) = self.choose_cell_or_network(*right)? else {
                        continue;
                    };

                    let lower_bound = lower_bound_join_cost(
                        &left_choice.candidate,
                        left_choice.gate_weight,
                        &right_choice.candidate,
                        right_choice.gate_weight,
                    );
                    if incumbent.is_some_and(|bound| lower_bound >= bound) {
                        self.stats.candidates_pruned_by_bound =
                            self.stats.candidates_pruned_by_bound.saturating_add(1);
                        continue;
                    }

                    self.stats.candidates_considered =
                        self.stats.candidates_considered.saturating_add(1);
                    let mut candidate =
                        build_binary_candidate(node, &left_choice.candidate, &right_choice.candidate)?;
                    let join_cost = left_choice
                        .gate_weight
                        .saturating_add(right_choice.gate_weight);
                    let key = (DedupOp::Join, id, enode_id);
                    if self.use_instance_level_dedup() {
                        candidate.cost = candidate.cost.saturating_add(join_cost);
                        add_instance_dedup_entry(&mut candidate, key);
                    } else {
                        add_dedup_entry(&mut candidate, key, join_cost);
                    }
                    (
                        Some(candidate),
                        left_choice.exact && right_choice.exact,
                    )
                }
                TransLog::Inv(child) => {
                    let child_result = self.eval(*child, AnyRootContext::Cell)?;
                    let Some(child_candidate) = child_result.candidate else {
                        continue;
                    };

                    let lower_bound = lower_bound_inv_cost(&child_candidate);
                    if incumbent.is_some_and(|bound| lower_bound >= bound) {
                        self.stats.candidates_pruned_by_bound =
                            self.stats.candidates_pruned_by_bound.saturating_add(1);
                        continue;
                    }

                    self.stats.candidates_considered =
                        self.stats.candidates_considered.saturating_add(1);
                    let mut candidate = build_unary_candidate(node, &child_candidate)?;
                    let key = (DedupOp::Inv, id, enode_id);
                    if self.use_instance_level_dedup() {
                        candidate.cost = candidate.cost.saturating_add(2);
                        add_instance_dedup_entry(&mut candidate, key);
                    } else {
                        add_dedup_entry(&mut candidate, key, 2);
                    }
                    (Some(candidate), child_result.exact)
                }
                TransLog::Concat([left, right]) => {
                    let left_result = self.eval(*left, AnyRootContext::Cell)?;
                    let Some(left_candidate) = left_result.candidate else {
                        continue;
                    };
                    let right_result = self.eval(*right, AnyRootContext::Cell)?;
                    let Some(right_candidate) = right_result.candidate else {
                        continue;
                    };

                    let lower_bound = lower_bound_concat_cost(&left_candidate, &right_candidate);
                    if incumbent.is_some_and(|bound| lower_bound >= bound) {
                        self.stats.candidates_pruned_by_bound =
                            self.stats.candidates_pruned_by_bound.saturating_add(1);
                        continue;
                    }

                    self.stats.candidates_considered =
                        self.stats.candidates_considered.saturating_add(1);
                    let candidate = build_binary_candidate(node, &left_candidate, &right_candidate)?;
                    (
                        Some(candidate),
                        left_result.exact && right_result.exact,
                    )
                }
                _ => (None, false),
            };

            if let Some(candidate) = candidate {
                let (next_best, next_exact) = pick_better_with_exact(
                    best,
                    best_exact,
                    candidate,
                    candidate_exact,
                );
                best = next_best;
                best_exact = next_exact;
                incumbent = best.as_ref().map(|value| value.cost);
            }
        }

        Ok(EvalResult {
            candidate: best,
            exact: best_exact,
        })
    }

    fn eval_network(&mut self, id: Id) -> Result<EvalResult, OptimizationError> {
        let key = (id, AnyRootContext::Network);
        let mut best = self
            .fallback_memo
            .get(&key)
            .map(|entry| entry.candidate.clone());
        let mut best_exact = self.fallback_memo.get(&key).is_some_and(|entry| entry.exact);
        let mut incumbent = best.as_ref().map(|candidate| candidate.cost);
        let nodes = self.egraph[id].nodes.clone();

        for node in &nodes {
            if is_size_node(node) {
                continue;
            }

            let (candidate, candidate_exact) = match node {
                TransLog::And([left, right]) | TransLog::Or([left, right]) => {
                    let Some(left_choice) = self.choose_cell_or_network(*left)? else {
                        continue;
                    };
                    if incumbent.is_some_and(|bound| {
                        lower_bound_gate_partial(&left_choice.candidate, left_choice.gate_weight)
                            >= bound
                    }) {
                        self.stats.candidates_pruned_by_bound =
                            self.stats.candidates_pruned_by_bound.saturating_add(1);
                        continue;
                    }
                    let Some(right_choice) = self.choose_cell_or_network(*right)? else {
                        continue;
                    };

                    let lower_bound = lower_bound_gate_cost(
                        &left_choice.candidate,
                        left_choice.gate_weight,
                        &right_choice.candidate,
                        right_choice.gate_weight,
                    );
                    if incumbent.is_some_and(|bound| lower_bound >= bound) {
                        self.stats.candidates_pruned_by_bound =
                            self.stats.candidates_pruned_by_bound.saturating_add(1);
                        continue;
                    }

                    self.stats.candidates_considered =
                        self.stats.candidates_considered.saturating_add(1);
                    let mut candidate =
                        build_binary_candidate(node, &left_choice.candidate, &right_choice.candidate)?;
                    let gate_sum = left_choice
                        .gate_weight
                        .saturating_add(right_choice.gate_weight);
                    candidate.cost = candidate.cost.saturating_add(gate_sum);
                    (
                        Some(candidate),
                        left_choice.exact && right_choice.exact,
                    )
                }
                TransLog::Bridge([a, b, c, d, e]) => {
                    let Some(a_choice) = self.choose_cell_or_network(*a)? else {
                        continue;
                    };
                    let Some(b_choice) = self.choose_cell_or_network(*b)? else {
                        continue;
                    };
                    let Some(c_choice) = self.choose_cell_or_network(*c)? else {
                        continue;
                    };
                    let Some(d_choice) = self.choose_cell_or_network(*d)? else {
                        continue;
                    };
                    let Some(e_choice) = self.choose_cell_or_network(*e)? else {
                        continue;
                    };

                    let lower_bound = lower_bound_bridge_cost([
                        (&a_choice.candidate, a_choice.gate_weight),
                        (&b_choice.candidate, b_choice.gate_weight),
                        (&c_choice.candidate, c_choice.gate_weight),
                        (&d_choice.candidate, d_choice.gate_weight),
                        (&e_choice.candidate, e_choice.gate_weight),
                    ]);
                    if incumbent.is_some_and(|bound| lower_bound >= bound) {
                        self.stats.candidates_pruned_by_bound =
                            self.stats.candidates_pruned_by_bound.saturating_add(1);
                        continue;
                    }

                    self.stats.candidates_considered =
                        self.stats.candidates_considered.saturating_add(1);
                    let mut candidate = build_bridge_candidate(
                        node,
                        &a_choice.candidate,
                        &b_choice.candidate,
                        &c_choice.candidate,
                        &d_choice.candidate,
                        &e_choice.candidate,
                    )?;
                    let gate_sum = a_choice
                        .gate_weight
                        .saturating_add(b_choice.gate_weight)
                        .saturating_add(c_choice.gate_weight)
                        .saturating_add(d_choice.gate_weight)
                        .saturating_add(e_choice.gate_weight);
                    candidate.cost = candidate.cost.saturating_add(gate_sum);
                    (
                        Some(candidate),
                        a_choice.exact
                            && b_choice.exact
                            && c_choice.exact
                            && d_choice.exact
                            && e_choice.exact,
                    )
                }
                _ => (None, false),
            };

            if let Some(candidate) = candidate {
                let (next_best, next_exact) = pick_better_with_exact(
                    best,
                    best_exact,
                    candidate,
                    candidate_exact,
                );
                best = next_best;
                best_exact = next_exact;
                incumbent = best.as_ref().map(|value| value.cost);
            }
        }

        Ok(EvalResult {
            candidate: best,
            exact: best_exact,
        })
    }

    fn choose_cell_or_network(
        &mut self,
        child_id: Id,
    ) -> Result<Option<ChildChoice>, OptimizationError> {
        let cell = self.eval(child_id, AnyRootContext::Cell)?;
        let network = self.eval(child_id, AnyRootContext::Network)?;
        match (cell.candidate, network.candidate) {
            (Some(cell_candidate), Some(network_candidate)) => {
                let cell_weight = candidate_cell_weight(&cell_candidate)?;
                let cell_cost = cell_candidate.cost.saturating_add(cell_weight);
                let network_cost = network_candidate.cost;
                if cell_cost <= network_cost {
                    Ok(Some(ChildChoice {
                        candidate: cell_candidate,
                        gate_weight: cell_weight,
                        exact: cell.exact,
                    }))
                } else {
                    Ok(Some(ChildChoice {
                        candidate: network_candidate,
                        gate_weight: 0,
                        exact: network.exact,
                    }))
                }
            }
            (Some(cell_candidate), None) => {
                let cell_weight = candidate_cell_weight(&cell_candidate)?;
                Ok(Some(ChildChoice {
                    candidate: cell_candidate,
                    gate_weight: cell_weight,
                    exact: cell.exact,
                }))
            }
            (None, Some(network_candidate)) => Ok(Some(ChildChoice {
                candidate: network_candidate,
                gate_weight: 0,
                exact: network.exact,
            })),
            (None, None) => Ok(None),
        }
    }

    fn update_fallback(
        &mut self,
        key: (Id, AnyRootContext),
        candidate: &Candidate,
        exact: bool,
    ) {
        match self.fallback_memo.get(&key) {
            Some(existing) => {
                let should_replace = is_strictly_better_candidate(candidate, &existing.candidate);
                let promote_exact = exact && !existing.exact;
                if should_replace || promote_exact {
                    self.fallback_memo.insert(
                        key,
                        FallbackEntry {
                            candidate: candidate.clone(),
                            exact: existing.exact || exact,
                        },
                    );
                    self.changed = true;
                    self.stats.fallback_updates = self.stats.fallback_updates.saturating_add(1);
                    if promote_exact {
                        self.stats.exact_promotions = self.stats.exact_promotions.saturating_add(1);
                    }
                }
            }
            None => {
                self.fallback_memo.insert(
                    key,
                    FallbackEntry {
                        candidate: candidate.clone(),
                        exact,
                    },
                );
                self.changed = true;
                self.stats.fallback_updates = self.stats.fallback_updates.saturating_add(1);
                if exact {
                    self.stats.exact_promotions = self.stats.exact_promotions.saturating_add(1);
                }
            }
        }
    }

    fn use_instance_level_dedup(&self) -> bool {
        true
    }
}

/// Combine two candidates according to semantics (cost + dedup policy).
fn merge_candidates(left: &Candidate, right: &Candidate) -> Result<Candidate, OptimizationError> {
    let (dedup, cost) =
        merge_dedup_cost_maps(left.dedup.clone(), left.cost, &right.dedup, right.cost);
    Ok(Candidate {
        expr: RecExpr::default(),
        cost,
        dedup,
    })
}

/// Create a candidate for a binary enode, respecting TransLog semantics.
fn build_binary_candidate(
    node: &TransLog,
    left: &Candidate,
    right: &Candidate,
) -> Result<Candidate, OptimizationError> {
    let merged = merge_candidates(left, right)?;
    let expr = build_binary_expr(node, &left.expr, &right.expr)?;
    Ok(Candidate {
        expr,
        cost: merged.cost,
        dedup: merged.dedup,
    })
}

/// Create a candidate for a unary enode, respecting TransLog semantics.
fn build_unary_candidate(
    node: &TransLog,
    child: &Candidate,
) -> Result<Candidate, OptimizationError> {
    let expr = build_unary_expr(node, &child.expr)?;
    Ok(Candidate {
        expr,
        cost: child.cost,
        dedup: child.dedup.clone(),
    })
}

/// Check if a node is a size operator. Size nodes are ignored by design.
fn is_size_node(node: &TransLog) -> bool {
    matches!(node, TransLog::Size(_))
}

/// Check whether the root eclass includes a concat ('&') node.
fn is_concat_root(egraph: &EGraph<TransLog, ()>, root: Id) -> bool {
    let root = egraph.find(root);
    egraph[root]
        .nodes
        .iter()
        .any(|node| matches!(node, TransLog::Concat(_)))
}

fn is_strictly_better_candidate(next: &Candidate, current: &Candidate) -> bool {
    next.cost < current.cost
        || (next.cost == current.cost
            && next.expr.as_ref().len() < current.expr.as_ref().len())
}

fn pick_better_with_exact(
    current: Option<Candidate>,
    current_exact: bool,
    next: Candidate,
    next_exact: bool,
) -> (Option<Candidate>, bool) {
    match current {
        None => (Some(next), next_exact),
        Some(curr) => {
            if is_strictly_better_candidate(&next, &curr)
                || (next.cost == curr.cost
                    && next.expr.as_ref().len() == curr.expr.as_ref().len()
                    && next_exact
                    && !current_exact)
            {
                (Some(next), next_exact)
            } else {
                (Some(curr), current_exact)
            }
        }
    }
}

fn dedup_total_cost(candidate: &Candidate) -> u64 {
    candidate
        .dedup
        .values()
        .fold(0_u64, |acc, value| acc.saturating_add(*value))
}

fn candidate_floor_cost(candidate: &Candidate) -> u64 {
    candidate.cost.saturating_sub(dedup_total_cost(candidate))
}

fn lower_bound_join_partial(left: &Candidate, left_gate_weight: u64) -> u64 {
    candidate_floor_cost(left).saturating_add(left_gate_weight)
}

fn lower_bound_join_cost(
    left: &Candidate,
    left_gate_weight: u64,
    right: &Candidate,
    right_gate_weight: u64,
) -> u64 {
    candidate_floor_cost(left)
        .saturating_add(candidate_floor_cost(right))
        .saturating_add(left_gate_weight)
        .saturating_add(right_gate_weight)
}

fn lower_bound_gate_partial(left: &Candidate, left_gate_weight: u64) -> u64 {
    candidate_floor_cost(left).saturating_add(left_gate_weight)
}

fn lower_bound_gate_cost(
    left: &Candidate,
    left_gate_weight: u64,
    right: &Candidate,
    right_gate_weight: u64,
) -> u64 {
    lower_bound_join_cost(left, left_gate_weight, right, right_gate_weight)
}

fn lower_bound_bridge_cost(items: [(&Candidate, u64); 5]) -> u64 {
    items.iter().fold(0_u64, |acc, (candidate, gate_weight)| {
        acc.saturating_add(candidate_floor_cost(candidate))
            .saturating_add(*gate_weight)
    })
}

fn lower_bound_inv_cost(child: &Candidate) -> u64 {
    candidate_floor_cost(child).saturating_add(2)
}

fn lower_bound_concat_cost(left: &Candidate, right: &Candidate) -> u64 {
    candidate_floor_cost(left).saturating_add(candidate_floor_cost(right))
}

fn build_size_candidate(
    node: &TransLog,
    child: &Candidate,
    size_expr: &RecExpr<TransLog>,
) -> Result<Candidate, OptimizationError> {
    let expr = build_binary_expr(node, &child.expr, size_expr)?;
    Ok(Candidate {
        expr,
        cost: child.cost,
        dedup: child.dedup.clone(),
    })
}

#[derive(Clone, Debug)]
struct SizeLiteralExpr {
    expr: RecExpr<TransLog>,
    value: u64,
}

fn best_size_literal_expr_in_eclass(
    egraph: &EGraph<TransLog, ()>,
    id: Id,
) -> Result<Option<SizeLiteralExpr>, OptimizationError> {
    let id = egraph.find(id);
    let eclass = &egraph[id];
    let mut best: Option<SizeLiteralExpr> = None;

    for node in &eclass.nodes {
        if let TransLog::Var(sym) = node {
            let value = parse_size_value(&sym.to_string())?;
            let mut expr = RecExpr::default();
            expr.add(node.clone());
            match &best {
                Some(current) if current.value <= value => {}
                _ => {
                    best = Some(SizeLiteralExpr { expr, value });
                }
            }
        }
    }
    Ok(best)
}

fn candidate_cell_weight(candidate: &Candidate) -> Result<u64, OptimizationError> {
    if candidate.expr.as_ref().is_empty() {
        return Ok(0);
    }
    let root = candidate.expr.as_ref().len().saturating_sub(1);
    match &candidate.expr.as_ref()[root] {
        TransLog::Join(_)
        | TransLog::Inv(_)
        | TransLog::Concat(_)
        | TransLog::Var(_)
        | TransLog::Bool(_) => Ok(1),
        TransLog::Size([_child, size_id]) => {
            match &candidate.expr.as_ref()[usize::from(*size_id)] {
                TransLog::Var(sym) => parse_size_value(&sym.to_string()),
                _ => Err(OptimizationError::InvalidSizeLiteral(
                    "size literal must be decimal".to_string(),
                )),
            }
        }
        _ => Ok(0),
    }
}

fn parse_size_value(token: &str) -> Result<u64, OptimizationError> {
    if token.is_empty() || !token.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(OptimizationError::InvalidSizeLiteral(token.to_string()));
    }
    let value = token.parse::<u64>().unwrap_or(0);
    if value == 0 {
        return Err(OptimizationError::InvalidSizeLiteral(token.to_string()));
    }
    Ok(value)
}

/// Build a literal candidate for Var/Bool nodes.
fn build_literal_candidate(node: &TransLog) -> Result<Candidate, OptimizationError> {
    let mut expr = RecExpr::default();
    expr.add(node.clone());
    Ok(Candidate {
        expr,
        cost: 0,
        dedup: HashMap::new(),
    })
}

/// Build a candidate for a bridge node.
fn build_bridge_candidate(
    node: &TransLog,
    a: &Candidate,
    b: &Candidate,
    c: &Candidate,
    d: &Candidate,
    e: &Candidate,
) -> Result<Candidate, OptimizationError> {
    let merged_ab = merge_candidates(a, b)?;
    let merged_cd = merge_candidates(c, d)?;
    let merged = merge_candidates(&merged_ab, &merged_cd)?;
    let merged = merge_candidates(&merged, e)?;
    let expr = build_bridge_expr(node, &a.expr, &b.expr, &c.expr, &d.expr, &e.expr)?;
    Ok(Candidate {
        expr,
        cost: merged.cost,
        dedup: merged.dedup,
    })
}

/// Append a subexpression into the target expression.
fn append_subexpr(target: &mut RecExpr<TransLog>, sub: &RecExpr<TransLog>) -> Id {
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

/// Build a binary expression from two candidates.
fn build_binary_expr(
    node: &TransLog,
    left: &RecExpr<TransLog>,
    right: &RecExpr<TransLog>,
) -> Result<RecExpr<TransLog>, OptimizationError> {
    let mut expr = RecExpr::default();
    let left_root = append_subexpr(&mut expr, left);
    let right_root = append_subexpr(&mut expr, right);
    let mut new_node = node.clone();
    if let Some(children) = new_node.children_mut().get_mut(0..2) {
        children[0] = left_root;
        children[1] = right_root;
    }
    expr.add(new_node);
    Ok(expr)
}

/// Build a unary expression from a candidate.
fn build_unary_expr(
    node: &TransLog,
    child: &RecExpr<TransLog>,
) -> Result<RecExpr<TransLog>, OptimizationError> {
    let mut expr = RecExpr::default();
    let child_root = append_subexpr(&mut expr, child);
    let mut new_node = node.clone();
    if let Some(slot) = new_node.children_mut().get_mut(0) {
        *slot = child_root;
    }
    expr.add(new_node);
    Ok(expr)
}

/// Build a bridge expression from five candidates.
fn build_bridge_expr(
    node: &TransLog,
    a: &RecExpr<TransLog>,
    b: &RecExpr<TransLog>,
    c: &RecExpr<TransLog>,
    d: &RecExpr<TransLog>,
    e: &RecExpr<TransLog>,
) -> Result<RecExpr<TransLog>, OptimizationError> {
    let mut expr = RecExpr::default();
    let na = append_subexpr(&mut expr, a);
    let nb = append_subexpr(&mut expr, b);
    let nc = append_subexpr(&mut expr, c);
    let nd = append_subexpr(&mut expr, d);
    let ne = append_subexpr(&mut expr, e);
    let mut new_node = node.clone();
    if let Some(children) = new_node.children_mut().get_mut(0..5) {
        children[0] = na;
        children[1] = nb;
        children[2] = nc;
        children[3] = nd;
        children[4] = ne;
    }
    expr.add(new_node);
    Ok(expr)
}


fn add_dedup_entry(candidate: &mut Candidate, key: DedupKey, value: u64) {
    if let Some(existing) = candidate.dedup.get(&key) {
        if value < *existing {
            candidate.cost = candidate.cost.saturating_sub(*existing);
            candidate.cost = candidate.cost.saturating_add(value);
            candidate.dedup.insert(key, value);
        }
    } else {
        candidate.dedup.insert(key, value);
        candidate.cost = candidate.cost.saturating_add(value);
    }
}

fn add_instance_dedup_entry(candidate: &mut Candidate, key: DedupKey) {
    if candidate.dedup.contains_key(&key) {
        return;
    }
    let shareable_cost = candidate_floor_cost(candidate);
    candidate.dedup.insert(key, shareable_cost);
}

fn merge_dedup_cost_maps(
    mut base: HashMap<DedupKey, u64>,
    mut base_cost: u64,
    other: &HashMap<DedupKey, u64>,
    other_cost: u64,
) -> (HashMap<DedupKey, u64>, u64) {
    base_cost = base_cost.saturating_add(other_cost);
    for (key, cost) in other {
        if let Some(existing) = base.get(key) {
            let max_cost = (*existing).max(*cost);
            let min_cost = (*existing).min(*cost);
            base_cost = base_cost.saturating_sub(max_cost);
            base.insert(*key, min_cost);
        } else {
            base.insert(*key, *cost);
        }
    }
    (base, base_cost)
}
