use std::collections::HashMap;
use std::fmt;

use egg::{EGraph, Id, Language, RecExpr};

use crate::language::TransLog;

mod min_transistor_fixed_point_extractor;
mod min_transistor_context_dp_extractor;

#[derive(Clone, Debug)]
pub struct MinTransistorResult {
    pub expr: RecExpr<TransLog>,
    pub cost: u64,
}

pub use min_transistor_fixed_point_extractor::{
    extract_min_transistor_expr,
    extract_min_transistor_expr_any_root,
};
pub use min_transistor_context_dp_extractor::{
    extract_min_transistor_expr_any_root_context_dp,
    extract_min_transistor_expr_joinlike_context_dp,
};

/// Error type for optimization and visualization pipeline.
#[derive(Debug)]
pub enum OptimizationError {
    /// Failed to parse the input expression string.
    ParseFailed(String),
    /// I/O error while saving the SPICE visualization image.
    FileWriteFailed(String),
    /// No expression with root = join or !(join(...)) was found in the root e-class.
    NoJoinLikeRootFound,
    /// Concat root contains a cycle; cannot enumerate outputs.
    ConcatCycleDetected,
    /// Simulation setup is missing required vectors.
    SimulationSetupFailed(String),
    /// Size literal is not a valid decimal value.
    InvalidSizeLiteral(String),
}

impl fmt::Display for OptimizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OptimizationError::ParseFailed(msg) => write!(f, "Parse Failed: {}", msg),
            OptimizationError::FileWriteFailed(msg) => write!(f, "File I/O Error: {}", msg),
            OptimizationError::NoJoinLikeRootFound => write!(
                f,
                "No equivalent expression with root = join or !(join(...)) was found"
            ),
            OptimizationError::ConcatCycleDetected => {
                write!(f, "Concat root contains a cycle")
            }
            OptimizationError::SimulationSetupFailed(msg) => {
                write!(f, "Simulation setup failed: {}", msg)
            }
            OptimizationError::InvalidSizeLiteral(msg) => {
                write!(f, "Invalid size literal: {}", msg)
            }
        }
    }
}

impl std::error::Error for OptimizationError {}

/// Append a subexpression `sub` to `target`, shifting child Ids by the length of `target`.
pub fn append_subexpr(
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
    Id::from(target.as_ref().len() - 1)
}

/// Build a RecExpr from a parse-only egraph where every eclass has exactly one node.
pub fn build_rec_expr_from_singleton_egraph(
    egraph: &EGraph<TransLog, ()>,
    root_id: Id,
) -> Result<RecExpr<TransLog>, String> {
    let mut expr = RecExpr::default();
    let mut memo: HashMap<Id, Id> = HashMap::new();
    let _root = build_rec_expr_node(egraph, root_id, &mut expr, &mut memo)?;
    Ok(expr)
}

fn build_rec_expr_node(
    egraph: &EGraph<TransLog, ()>,
    id: Id,
    expr: &mut RecExpr<TransLog>,
    memo: &mut HashMap<Id, Id>,
) -> Result<Id, String> {
    let class_id = egraph.find(id);
    if let Some(mapped) = memo.get(&class_id) {
        return Ok(*mapped);
    }

    let eclass = &egraph[class_id];
    if eclass.nodes.len() != 1 {
        return Err(format!(
            "eclass {:?} has {} nodes; singleton build expects parse-only egraphs",
            class_id,
            eclass.nodes.len()
        ));
    }
    let node = &eclass.nodes[0];
    let new_node = match node {
        TransLog::And([a, b]) => {
            let lhs = build_rec_expr_node(egraph, *a, expr, memo)?;
            let rhs = build_rec_expr_node(egraph, *b, expr, memo)?;
            TransLog::And([lhs, rhs])
        }
        TransLog::Or([a, b]) => {
            let lhs = build_rec_expr_node(egraph, *a, expr, memo)?;
            let rhs = build_rec_expr_node(egraph, *b, expr, memo)?;
            TransLog::Or([lhs, rhs])
        }
        TransLog::Inv(a) => {
            let inner = build_rec_expr_node(egraph, *a, expr, memo)?;
            TransLog::Inv(inner)
        }
        TransLog::Join([a, b]) => {
            let lhs = build_rec_expr_node(egraph, *a, expr, memo)?;
            let rhs = build_rec_expr_node(egraph, *b, expr, memo)?;
            TransLog::Join([lhs, rhs])
        }
        TransLog::Bridge([a, b, c, d, e]) => {
            let na = build_rec_expr_node(egraph, *a, expr, memo)?;
            let nb = build_rec_expr_node(egraph, *b, expr, memo)?;
            let nc = build_rec_expr_node(egraph, *c, expr, memo)?;
            let nd = build_rec_expr_node(egraph, *d, expr, memo)?;
            let ne = build_rec_expr_node(egraph, *e, expr, memo)?;
            TransLog::Bridge([na, nb, nc, nd, ne])
        }
        TransLog::Concat([a, b]) => {
            let lhs = build_rec_expr_node(egraph, *a, expr, memo)?;
            let rhs = build_rec_expr_node(egraph, *b, expr, memo)?;
            TransLog::Concat([lhs, rhs])
        }
        TransLog::Size([a, b]) => {
            let lhs = build_rec_expr_node(egraph, *a, expr, memo)?;
            let rhs = build_rec_expr_node(egraph, *b, expr, memo)?;
            TransLog::Size([lhs, rhs])
        }
        TransLog::Var(sym) => TransLog::Var(*sym),
        TransLog::Bool(b) => TransLog::Bool(*b),
    };

    let new_id = expr.add(new_node);
    memo.insert(class_id, new_id);
    Ok(new_id)
}

fn collect_output_ids_expr(
    expr: &RecExpr<TransLog>,
    id: usize,
    outputs: &mut Vec<usize>,
) {
    match &expr.as_ref()[id] {
        TransLog::Concat([left, right]) => {
            collect_output_ids_expr(expr, (*left).into(), outputs);
            collect_output_ids_expr(expr, (*right).into(), outputs);
        }
        _ => outputs.push(id),
    }
}

pub fn expr_output_ids(expr: &RecExpr<TransLog>) -> Vec<usize> {
    if expr.as_ref().is_empty() {
        return Vec::new();
    }
    let root_id = expr.as_ref().len() - 1;
    let mut outputs = Vec::new();
    collect_output_ids_expr(expr, root_id, &mut outputs);
    outputs
}

pub fn is_join_like_node(expr: &RecExpr<TransLog>, id: usize) -> bool {
    match &expr.as_ref()[id] {
        TransLog::Join(_) => true,
        TransLog::Inv(child_id) => {
            let idx: usize = (*child_id).into();
            matches!(expr.as_ref().get(idx), Some(TransLog::Join(_)))
        }
        _ => false,
    }
}

pub fn is_join_like_root(expr: &RecExpr<TransLog>) -> bool {
    let nodes = expr.as_ref();
    let root = match nodes.last() {
        Some(n) => n,
        None => return false,
    };

    match root {
        TransLog::Join(_) => true,
        TransLog::Inv(child_id) => {
            let idx: usize = usize::from(*child_id);
            matches!(nodes.get(idx), Some(TransLog::Join(_)))
        }
        _ => false,
    }
}

pub fn is_multi_output_expr_valid(
    expr: &RecExpr<TransLog>,
    output_ids: &[usize],
) -> bool {
    if output_ids.len() <= 1 {
        return false;
    }
    if !is_structurally_valid(expr) {
        return false;
    }
    output_ids.iter().all(|id| is_join_like_node(expr, *id))
}

/// Structural sanity check: disallow inversions directly over AND/OR.
pub fn is_structurally_valid(expr: &RecExpr<TransLog>) -> bool {
    for node in expr.as_ref() {
        if let TransLog::Inv(child) = node {
            match &expr[*child] {
                TransLog::Var(_)
                | TransLog::Bool(_)
                | TransLog::Join(_)
                | TransLog::Inv(_) => {}
                _ => return false,
            }
        }
    }
    true
}

pub fn map_expr_to_choices(
    egraph: &EGraph<TransLog, ()>,
    expr: &RecExpr<TransLog>,
) -> Option<HashMap<Id, TransLog>> {
    let ids = egraph.lookup_expr_ids(expr)?;
    let mut choices = HashMap::new();
    for (idx, node) in expr.as_ref().iter().enumerate() {
        let eid = egraph.find(ids[idx]);
        let mapped = remap_node_children(node, &ids, egraph);
        choices.insert(eid, mapped);
    }
    Some(choices)
}

fn remap_node_children(
    node: &TransLog,
    node_eclass_ids: &[Id],
    egraph: &EGraph<TransLog, ()>,
) -> TransLog {
    match node {
        TransLog::And([a, b]) => {
            let lhs = egraph.find(node_eclass_ids[usize::from(*a)]);
            let rhs = egraph.find(node_eclass_ids[usize::from(*b)]);
            TransLog::And([lhs, rhs])
        }
        TransLog::Or([a, b]) => {
            let lhs = egraph.find(node_eclass_ids[usize::from(*a)]);
            let rhs = egraph.find(node_eclass_ids[usize::from(*b)]);
            TransLog::Or([lhs, rhs])
        }
        TransLog::Inv(a) => {
            let inner = egraph.find(node_eclass_ids[usize::from(*a)]);
            TransLog::Inv(inner)
        }
        TransLog::Join([a, b]) => {
            let lhs = egraph.find(node_eclass_ids[usize::from(*a)]);
            let rhs = egraph.find(node_eclass_ids[usize::from(*b)]);
            TransLog::Join([lhs, rhs])
        }
        TransLog::Bridge([a, b, c, d, e]) => {
            let na = egraph.find(node_eclass_ids[usize::from(*a)]);
            let nb = egraph.find(node_eclass_ids[usize::from(*b)]);
            let nc = egraph.find(node_eclass_ids[usize::from(*c)]);
            let nd = egraph.find(node_eclass_ids[usize::from(*d)]);
            let ne = egraph.find(node_eclass_ids[usize::from(*e)]);
            TransLog::Bridge([na, nb, nc, nd, ne])
        }
        TransLog::Concat([a, b]) => {
            let lhs = egraph.find(node_eclass_ids[usize::from(*a)]);
            let rhs = egraph.find(node_eclass_ids[usize::from(*b)]);
            TransLog::Concat([lhs, rhs])
        }
        TransLog::Size([a, b]) => {
            let lhs = egraph.find(node_eclass_ids[usize::from(*a)]);
            let rhs = egraph.find(node_eclass_ids[usize::from(*b)]);
            TransLog::Size([lhs, rhs])
        }
        TransLog::Var(sym) => TransLog::Var(*sym),
        TransLog::Bool(b) => TransLog::Bool(*b),
    }
}
