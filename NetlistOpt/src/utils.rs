use egg::{AstSize, EGraph, Extractor, Id, Language, RecExpr, Rewrite, Symbol};
use espresso_logic::{BoolExpr, Dnf, Minimizable};
use logicng::formulas::{EncodedFormula, Formula, FormulaFactory, Literal};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Display;
use std::path::Path;
use std::io;
use crate::language::TransLog;
use egg::{Runner};
use crate::rules::make_rules;
use crate::extractor::{
    append_subexpr,
};
use std::fs::File;  // Added for file creation
use std::io::Write; // Added for writing capabilities

pub fn save_recexpr_to_png<L, P>(expr: &RecExpr<L>, filename: P) -> io::Result<()>
where
    L: Language + Display,
    P: AsRef<Path>,       
{
    let mut egraph = EGraph::<L, ()>::default();

    egraph.add_expr(expr);
    egraph.dot().to_png(filename)
}

pub const DEFAULT_PROOF_ITER_LIMIT: usize = 600;

#[derive(Clone, Debug)]
pub(crate) struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    pub(crate) fn new(seed: u64) -> Self {
        let seed = if seed == 0 { 0x9E3779B97F4A7C15 } else { seed };
        Self { state: seed }
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
        self.state
    }

    pub(crate) fn next_u128(&mut self) -> u128 {
        let hi = self.next_u64() as u128;
        let lo = self.next_u64() as u128;
        (hi << 64) | lo
    }

    pub(crate) fn gen_range(&mut self, upper: u128) -> u128 {
        if upper == 0 {
            return 0;
        }
        let mut value = self.next_u128();
        let limit = u128::MAX - (u128::MAX % upper);
        while value >= limit {
            value = self.next_u128();
        }
        value % upper
    }
}

/// Summary of an equality-saturation proof attempt.
///
/// This is designed for batch experiments (e.g., rule ablation), so it returns
/// stats and counters without printing the proof text itself.
#[derive(Debug, Clone)]
pub struct ProofReport {
    /// Whether `expr_1` and `expr_2` were proven equivalent within the limits.
    pub equivalent: bool,
    /// Number of flattened explanation steps from `expr_1` to `expr_2` (if equivalent).
    pub steps_1_to_2: Option<usize>,
    /// Number of flattened explanation steps from `expr_2` to `expr_1` (if equivalent).
    pub steps_2_to_1: Option<usize>,
    /// Number of e-classes in the final saturated e-graph.
    pub eclasses: usize,
    /// Number of e-nodes in the final saturated e-graph.
    pub enodes: usize,
    /// Number of directed child edges in the final e-graph (sum of enode arities).
    pub edges: usize,
    /// How many times each rewrite rule name appears in the proof explanation for `expr_1 -> expr_2`.
    pub in_proof_rule_counts_1_to_2: HashMap<String, usize>,
    /// How many times each rewrite rule name appears in the proof explanation for `expr_2 -> expr_1`.
    pub in_proof_rule_counts_2_to_1: HashMap<String, usize>,
}

fn egraph_edge_count(egraph: &EGraph<TransLog, ()>) -> usize {
    egraph
        .classes()
        .map(|class| {
            class
                .nodes
                .iter()
                .map(|node| node.children().len())
                .sum::<usize>()
        })
        .sum()
}

fn collect_in_proof_rule_counts(flat_terms: &[String]) -> HashMap<String, usize> {
    fn bump(map: &mut HashMap<String, usize>, name: &str) {
        *map.entry(name.to_string()).or_insert(0) += 1;
    }

    fn scan_marker(map: &mut HashMap<String, usize>, line: &str, marker: &str) {
        let mut search_idx = 0usize;
        while let Some(rel_pos) = line[search_idx..].find(marker) {
            let pos = search_idx + rel_pos;
            let mut name_start = pos + marker.len();
            while name_start < line.len()
                && line.as_bytes()[name_start].is_ascii_whitespace()
            {
                name_start += 1;
            }
            let mut name_end = name_start;
            while name_end < line.len() {
                let b = line.as_bytes()[name_end];
                if b.is_ascii_whitespace() || b == b')' {
                    break;
                }
                name_end += 1;
            }
            if name_end > name_start {
                bump(map, &line[name_start..name_end]);
            }
            search_idx = name_end;
        }
    }

    let mut counts: HashMap<String, usize> = HashMap::new();
    for term in flat_terms {
        scan_marker(&mut counts, term, "Rewrite=>");
        scan_marker(&mut counts, term, "Rewrite<=");
    }
    counts
}

/// Computes the proof report without printing.
pub fn run_proof_report_with_rules(
    expr_1: &RecExpr<TransLog>,
    expr_2: &RecExpr<TransLog>,
    iter_limit: usize,
    rules: &[Rewrite<TransLog, ()>],
) -> ProofReport {
    let mut runner = Runner::default()
        .with_explanations_enabled()
        .with_expr(expr_1)
        .with_expr(expr_2)
        .with_iter_limit(iter_limit)
        .run(rules);

    let eclasses = runner.egraph.number_of_classes();
    let enodes = runner.egraph.total_number_of_nodes();
    let edges = egraph_edge_count(&runner.egraph);

    let root_1 = runner.roots[0];
    let root_2 = runner.roots[1];
    let equivalent = runner.egraph.find(root_1) == runner.egraph.find(root_2);

    if !equivalent {
        return ProofReport {
            equivalent: false,
            steps_1_to_2: None,
            steps_2_to_1: None,
            eclasses,
            enodes,
            edges,
            in_proof_rule_counts_1_to_2: HashMap::new(),
            in_proof_rule_counts_2_to_1: HashMap::new(),
        };
    }

    let mut exp_1_to_2 = runner.explain_equivalence(expr_1, expr_2);
    let flat_1_to_2 = exp_1_to_2.get_flat_strings();
    let steps_1_to_2 = flat_1_to_2.len().saturating_sub(1);
    let in_proof_rule_counts_1_to_2 = collect_in_proof_rule_counts(&flat_1_to_2);

    let mut exp_2_to_1 = runner.explain_equivalence(expr_2, expr_1);
    let flat_2_to_1 = exp_2_to_1.get_flat_strings();
    let steps_2_to_1 = flat_2_to_1.len().saturating_sub(1);
    let in_proof_rule_counts_2_to_1 = collect_in_proof_rule_counts(&flat_2_to_1);

    ProofReport {
        equivalent: true,
        steps_1_to_2: Some(steps_1_to_2),
        steps_2_to_1: Some(steps_2_to_1),
        eclasses,
        enodes,
        edges,
        in_proof_rule_counts_1_to_2,
        in_proof_rule_counts_2_to_1,
    }
}

/// For each `(label, expr_str)` step, check whether the expression exists in `egraph`.
/// If present, print the AstSize-best representative expression in that e-class.
/// Returns a vector of `(label, present, rendered_expr)` in input order.
pub fn print_egraph_step_presence(
    egraph: &EGraph<TransLog, ()>,
    steps: &[(&str, &str)],
) -> Result<Vec<(String, bool, String)>, String> {
    let mut extractor = Extractor::new(egraph, AstSize);
    let mut rows: Vec<(String, bool, String)> = Vec::with_capacity(steps.len());

    println!("step|present|expr");
    for (label, expr_str) in steps {
        let step_expr: RecExpr<TransLog> = expr_str.parse().map_err(|err| {
            format!(
                "failed to parse step expression '{}' (label={}): {}",
                expr_str, label, err
            )
        })?;

        if let Some(id) = egraph.lookup_expr(&step_expr) {
            let (_cost, best_expr) = extractor.find_best(id);
            let rendered = best_expr.to_string();
            println!("{}|true|{}", label, rendered);
            rows.push(((*label).to_string(), true, rendered));
        } else {
            let rendered = step_expr.to_string();
            println!("{}|false|{}", label, rendered);
            rows.push(((*label).to_string(), false, rendered));
        }
    }

    Ok(rows)
}

/// Runs a runner through multiple stages, each with its own ruleset and iteration limit.
/// The runner is consumed and reconfigured per stage, returning the final runner.
pub fn run_runner_in_stages<'a>(
    mut runner: Runner<TransLog, ()>,
    stages: &[(&'a [Rewrite<TransLog, ()>], usize)],
) -> Runner<TransLog, ()> {
    for (ruleset, iterations) in stages {
        if *iterations == 0 {
            continue;
        }
        runner = runner.with_iter_limit(*iterations).run(*ruleset);
    }
    runner
}

/// Proves equivalence with a custom rewrite ruleset.
pub fn run_proof_bidirectional_with_rules(
    expr_1: &RecExpr<TransLog>,
    expr_2: &RecExpr<TransLog>,
    iter_limit: usize,
    rules: &[Rewrite<TransLog, ()>],
) -> Result<(usize, usize), String> {
    println!("=== Starting Formal Verification (Bidirectional) ===");
    println!("\n[Expression 1]:\n{}", expr_1.pretty(120));
    println!("\n[Expression 2]:\n{}", expr_2.pretty(120));

    let report = run_proof_report_with_rules(expr_1, expr_2, iter_limit, rules);
    println!(
        "\n[EGraph Stats] eclasses = {}, enodes = {}, edges = {}",
        report.eclasses, report.enodes, report.edges
    );

    if !report.equivalent {
        let failure_msg = format!(
            "Equivalence not found within iteration limit. E-Graph size: {} nodes, {} classes",
            report.enodes, report.eclasses
        );
        println!("\n[FAILURE] {}", failure_msg);
        return Err(failure_msg);
    }

    let steps_1_to_2 = report.steps_1_to_2.unwrap_or(0);
    let steps_2_to_1 = report.steps_2_to_1.unwrap_or(0);
    println!("\n[SUCCESS] Expressions are equivalent.");
    println!(
        "Steps (flat): 1 -> 2 = {}, 2 -> 1 = {}",
        steps_1_to_2, steps_2_to_1
    );

    Ok((steps_1_to_2, steps_2_to_1))
}

/// Proves equivalence between two expressions via equality saturation.
///
/// Returns the number of flattened rewrite steps for `expr_1 -> expr_2` and `expr_2 -> expr_1`.
pub fn run_proof_bidirectional(
    expr_1: &RecExpr<TransLog>,
    expr_2: &RecExpr<TransLog>,
    iter_limit: usize,
) -> Result<(usize, usize), String> {
    let rules = make_rules();
    run_proof_bidirectional_with_rules(expr_1, expr_2, iter_limit, &rules)
}

/// Convenience wrapper around [`run_proof_bidirectional`] using [`DEFAULT_PROOF_ITER_LIMIT`].
pub fn run_proof(
    expr_1: &RecExpr<TransLog>,
    expr_2: &RecExpr<TransLog>,
) -> Result<(usize, usize), String> {
    run_proof_bidirectional(expr_1, expr_2, DEFAULT_PROOF_ITER_LIMIT)
}

/// Convenience wrapper: run the proof with a caller-provided ruleset.
pub fn run_proof_with_rules(
    expr_1: &RecExpr<TransLog>,
    expr_2: &RecExpr<TransLog>,
    rules: &[Rewrite<TransLog, ()>],
) -> Result<(usize, usize), String> {
    run_proof_bidirectional_with_rules(expr_1, expr_2, DEFAULT_PROOF_ITER_LIMIT, rules)
}

pub fn prove_equivalence(
    p_expr_str: &str, 
    q_expr_str: &str, 
    proof_filename: &str
) -> Result<String, String> {
    
    println!("=== Starting Formal Verification ===");
    println!("\n[Expression P]:\n{}", p_expr_str);
    println!("\n[Expression Q]:\n{}", q_expr_str);

    // ---------------------------------------------------------
    // 1. Parse Expressions
    // ---------------------------------------------------------
    let p_recexpr: RecExpr<TransLog> = match p_expr_str.parse() {
        Ok(expr) => expr,
        Err(e) => {
            let err_msg = format!("Failed to parse P: {}", e);
            println!("❌ [FAILURE] {}", err_msg);
            return Err(err_msg);
        }
    };
    let q_recexpr: RecExpr<TransLog> = match q_expr_str.parse() {
        Ok(expr) => expr,
        Err(e) => {
            let err_msg = format!("Failed to parse Q: {}", e);
            println!("❌ [FAILURE] {}", err_msg);
            return Err(err_msg);
        }
    };

    // ---------------------------------------------------------
    // 2. Run the Egg Engine
    // ---------------------------------------------------------
    let mut runner = Runner::default()
        .with_explanations_enabled()
        .with_expr(&p_recexpr)
        .with_expr(&q_recexpr)
        .with_iter_limit(600) // From original function
        .run(&make_rules());

    // ---------------------------------------------------------
    // 3. Check Equivalence & Return/Save Proof
    // ---------------------------------------------------------
    let root_p = runner.roots[0];
    let root_q = runner.roots[1];

    if runner.egraph.find(root_p) == runner.egraph.find(root_q) {
        println!("\n✅ [SUCCESS] Proof Complete! P and Q are equivalent.");
        
        // 1. Get the proof string
        let explanation = runner.explain_equivalence(&p_recexpr, &q_recexpr);
        let proof_str = explanation.get_string_with_let();
        
        // 2. Write to file
        match File::create(proof_filename) {
            Ok(mut file) => {
                match file.write_all(proof_str.as_bytes()) {
                    Ok(_) => println!("📝 Proof trace successfully saved to '{}'", proof_filename),
                    Err(e) => println!("⚠️ Error writing to file: {}", e),
                }
            },
            Err(e) => println!("⚠️ Error creating file: {}", e),
        }
        
        // 3. Return proof string
        Ok(proof_str)
        
    } else {
        println!("\n❌ [FAILURE] Proof Failed. Equivalence not found within iteration limit.");
        
        let failure_msg = format!(
            "Equivalence not found. Possible reasons: Missing rewrite rules or the expressions are logically distinct.\nE-Graph size: {} nodes, {} classes",
            runner.egraph.total_number_of_nodes(),
            runner.egraph.number_of_classes()
        );
        
        println!("{}", failure_msg);
        
        // Optional: Dump the E-Graph (from original function)
        // runner.egraph.dot().to_dot("failure_graph.dot").unwrap();

        Err(failure_msg)
    }
}

#[derive(Copy, Clone)]
enum PrincipalForm {
    Dnf,
    Cnf,
}

pub fn principal_dnf(expr: &RecExpr<TransLog>) -> Result<RecExpr<TransLog>, String> {
    principal_form(expr, PrincipalForm::Dnf)
}

pub fn principal_cnf(expr: &RecExpr<TransLog>) -> Result<RecExpr<TransLog>, String> {
    principal_form(expr, PrincipalForm::Cnf)
}

pub fn to_dnf(expr: &RecExpr<TransLog>) -> Result<RecExpr<TransLog>, String> {
    if expr.as_ref().is_empty() {
        return Err("empty expression".to_string());
    }
    collect_vars_and_validate(expr)?;

    let bool_expr = to_bool_expr(expr)?;
    let minimized = bool_expr
        .minimize_exact()
        .map_err(|err| err.to_string())?;
    let dnf = Dnf::from(&minimized);
    Ok(recexpr_from_dnf(&dnf))
}

pub fn to_cnf(expr: &RecExpr<TransLog>) -> Result<RecExpr<TransLog>, String> {
    if expr.as_ref().is_empty() {
        return Err("empty expression".to_string());
    }
    collect_vars_and_validate(expr)?;

    let bool_expr = to_bool_expr(expr)?;
    let minimized_neg = bool_expr
        .not()
        .minimize_exact()
        .map_err(|err| err.to_string())?;
    let dnf_neg = Dnf::from(&minimized_neg);
    Ok(recexpr_from_negated_dnf(&dnf_neg))
}

pub fn to_nnf(expr: &RecExpr<TransLog>) -> Result<RecExpr<TransLog>, String> {
    if expr.as_ref().is_empty() {
        return Err("empty expression".to_string());
    }
    collect_vars_and_validate(expr)?;

    let f = FormulaFactory::new();
    let formula = to_logicng_formula(expr, &f)?;
    let nnf = f.nnf_of(formula);
    recexpr_from_logicng_nnf(nnf, &f)
}

fn principal_form(
    expr: &RecExpr<TransLog>,
    form: PrincipalForm,
) -> Result<RecExpr<TransLog>, String> {
    if expr.as_ref().is_empty() {
        return Err("empty expression".to_string());
    }

    let vars = collect_vars_and_validate(expr)?;
    let mut var_index: HashMap<Symbol, usize> = HashMap::with_capacity(vars.len());
    for (idx, sym) in vars.iter().enumerate() {
        var_index.insert(*sym, idx);
    }

    if vars.is_empty() {
        let value = eval_expr_with_assignment(expr, &var_index, &[]);
        return Ok(build_bool_expr(value));
    }

    let mut minterms: Vec<Vec<bool>> = Vec::new();
    let mut maxterms: Vec<Vec<bool>> = Vec::new();
    let mut assignment = vec![false; vars.len()];

    loop {
        let value = eval_expr_with_assignment(expr, &var_index, &assignment);
        if value {
            minterms.push(assignment.clone());
        } else {
            maxterms.push(assignment.clone());
        }

        if !increment_assignment(&mut assignment) {
            break;
        }
    }

    match form {
        PrincipalForm::Dnf => {
            if maxterms.is_empty() {
                Ok(build_bool_expr(true))
            } else if minterms.is_empty() {
                Ok(build_bool_expr(false))
            } else {
                Ok(build_or_chain(
                    &minterms
                        .iter()
                        .map(|term| build_minterm(&vars, term))
                        .collect::<Vec<_>>(),
                ))
            }
        }
        PrincipalForm::Cnf => {
            if minterms.is_empty() {
                Ok(build_bool_expr(false))
            } else if maxterms.is_empty() {
                Ok(build_bool_expr(true))
            } else {
                Ok(build_and_chain(
                    &maxterms
                        .iter()
                        .map(|term| build_maxterm(&vars, term))
                        .collect::<Vec<_>>(),
                ))
            }
        }
    }
}

fn to_bool_expr(expr: &RecExpr<TransLog>) -> Result<BoolExpr, String> {
    let mut nodes: Vec<BoolExpr> = Vec::with_capacity(expr.as_ref().len());
    for node in expr.as_ref() {
        let mapped = match node {
            TransLog::Var(sym) => BoolExpr::variable(&sym.to_string()),
            TransLog::Bool(value) => BoolExpr::constant(*value),
            TransLog::Inv(child) => {
                let inner = &nodes[usize::from(*child)];
                inner.not()
            }
            TransLog::And([left, right]) => {
                let lhs = &nodes[usize::from(*left)];
                let rhs = &nodes[usize::from(*right)];
                lhs.and(rhs)
            }
            TransLog::Or([left, right]) => {
                let lhs = &nodes[usize::from(*left)];
                let rhs = &nodes[usize::from(*right)];
                lhs.or(rhs)
            }
            other => {
                return Err(format!(
                    "unsupported node in logic expression: {:?}",
                    other
                ));
            }
        };
        nodes.push(mapped);
    }
    nodes
        .last()
        .cloned()
        .ok_or_else(|| "empty expression".to_string())
}

fn to_logicng_formula(
    expr: &RecExpr<TransLog>,
    f: &FormulaFactory,
) -> Result<EncodedFormula, String> {
    let mut nodes: Vec<EncodedFormula> = Vec::with_capacity(expr.as_ref().len());
    for node in expr.as_ref() {
        let mapped = match node {
            TransLog::Var(sym) => f.variable(&sym.to_string()),
            TransLog::Bool(value) => EncodedFormula::constant(*value),
            TransLog::Inv(child) => {
                let inner = nodes[usize::from(*child)];
                f.not(inner)
            }
            TransLog::And([left, right]) => {
                let lhs = nodes[usize::from(*left)];
                let rhs = nodes[usize::from(*right)];
                f.and(&[lhs, rhs])
            }
            TransLog::Or([left, right]) => {
                let lhs = nodes[usize::from(*left)];
                let rhs = nodes[usize::from(*right)];
                f.or(&[lhs, rhs])
            }
            other => {
                return Err(format!(
                    "unsupported node in logic expression: {:?}",
                    other
                ));
            }
        };
        nodes.push(mapped);
    }
    nodes
        .last()
        .copied()
        .ok_or_else(|| "empty expression".to_string())
}

fn recexpr_from_logicng_nnf(
    formula: EncodedFormula,
    f: &FormulaFactory,
) -> Result<RecExpr<TransLog>, String> {
    match formula.unpack(f) {
        Formula::True => Ok(build_bool_expr(true)),
        Formula::False => Ok(build_bool_expr(false)),
        Formula::Lit(lit) => Ok(recexpr_from_logicng_literal(lit, f)),
        Formula::And(ops) => {
            let mut terms: Vec<RecExpr<TransLog>> = Vec::new();
            for op in ops {
                terms.push(recexpr_from_logicng_nnf(op, f)?);
            }
            if terms.is_empty() {
                return Ok(build_bool_expr(true));
            }
            Ok(build_and_chain(&terms))
        }
        Formula::Or(ops) => {
            let mut terms: Vec<RecExpr<TransLog>> = Vec::new();
            for op in ops {
                terms.push(recexpr_from_logicng_nnf(op, f)?);
            }
            if terms.is_empty() {
                return Ok(build_bool_expr(false));
            }
            Ok(build_or_chain(&terms))
        }
        Formula::Not(inner) => match inner.unpack(f) {
            Formula::True => Ok(build_bool_expr(false)),
            Formula::False => Ok(build_bool_expr(true)),
            Formula::Lit(lit) => Ok(recexpr_from_logicng_literal(lit.negate(), f)),
            _ => Err("logicng returned non-literal negation in NNF".to_string()),
        },
        other => Err(format!(
            "logicng returned unsupported formula in NNF conversion: {:?}",
            other
        )),
    }
}

fn recexpr_from_logicng_literal(
    lit: Literal,
    f: &FormulaFactory,
) -> RecExpr<TransLog> {
    let sym = Symbol::from(lit.name(f).as_ref());
    if lit.phase() {
        build_var_expr(sym)
    } else {
        build_unary_expr(
            TransLog::Inv(Id::from(0)),
            &build_var_expr(sym),
        )
    }
}

fn recexpr_from_dnf(dnf: &Dnf) -> RecExpr<TransLog> {
    if dnf.is_empty() {
        return build_bool_expr(false);
    }

    let mut terms: Vec<RecExpr<TransLog>> = Vec::new();
    for cube in dnf.cubes() {
        if cube.is_empty() {
            return build_bool_expr(true);
        }
        let mut literals: Vec<RecExpr<TransLog>> = Vec::new();
        for (var, polarity) in cube {
            let sym = Symbol::from(var.as_ref());
            let lit = if *polarity {
                build_var_expr(sym)
            } else {
                build_unary_expr(
                    TransLog::Inv(Id::from(0)),
                    &build_var_expr(sym),
                )
            };
            literals.push(lit);
        }
        terms.push(build_and_chain(&literals));
    }

    build_or_chain(&terms)
}

fn recexpr_from_negated_dnf(dnf: &Dnf) -> RecExpr<TransLog> {
    if dnf.is_empty() {
        return build_bool_expr(true);
    }

    let mut clauses: Vec<RecExpr<TransLog>> = Vec::new();
    for cube in dnf.cubes() {
        if cube.is_empty() {
            return build_bool_expr(false);
        }
        let mut literals: Vec<RecExpr<TransLog>> = Vec::new();
        for (var, polarity) in cube {
            let sym = Symbol::from(var.as_ref());
            let lit = if *polarity {
                build_unary_expr(
                    TransLog::Inv(Id::from(0)),
                    &build_var_expr(sym),
                )
            } else {
                build_var_expr(sym)
            };
            literals.push(lit);
        }
        clauses.push(build_or_chain(&literals));
    }

    build_and_chain(&clauses)
}

fn collect_vars_and_validate(
    expr: &RecExpr<TransLog>,
) -> Result<Vec<Symbol>, String> {
    let mut vars: BTreeMap<String, Symbol> = BTreeMap::new();
    for node in expr.as_ref() {
        match node {
            TransLog::Var(sym) => {
                vars.entry(sym.to_string()).or_insert(*sym);
            }
            TransLog::Bool(_)
            | TransLog::And(_)
            | TransLog::Or(_)
            | TransLog::Inv(_) => {}
            other => {
                return Err(format!(
                    "unsupported node in logic expression: {:?}",
                    other
                ));
            }
        }
    }
    Ok(vars.into_values().collect())
}

fn eval_expr_with_assignment(
    expr: &RecExpr<TransLog>,
    var_index: &HashMap<Symbol, usize>,
    assignment: &[bool],
) -> bool {
    let mut values: Vec<bool> = Vec::with_capacity(expr.as_ref().len());
    for node in expr.as_ref().iter() {
        let value = match node {
            TransLog::Bool(b) => *b,
            TransLog::Var(sym) => {
                let idx = *var_index.get(sym).expect("variable not indexed");
                assignment[idx]
            }
            TransLog::And([a, b]) => {
                let ia: usize = (*a).into();
                let ib: usize = (*b).into();
                values[ia] && values[ib]
            }
            TransLog::Or([a, b]) => {
                let ia: usize = (*a).into();
                let ib: usize = (*b).into();
                values[ia] || values[ib]
            }
            TransLog::Inv(a) => {
                let ia: usize = (*a).into();
                !values[ia]
            }
            _ => unreachable!("unsupported node should have been filtered"),
        };
        values.push(value);
    }
    values.last().copied().unwrap_or(false)
}

fn increment_assignment(bits: &mut [bool]) -> bool {
    for idx in (0..bits.len()).rev() {
        if bits[idx] {
            bits[idx] = false;
        } else {
            bits[idx] = true;
            return true;
        }
    }
    false
}

fn build_bool_expr(value: bool) -> RecExpr<TransLog> {
    let mut expr = RecExpr::default();
    expr.add(TransLog::Bool(value));
    expr
}

fn build_var_expr(sym: Symbol) -> RecExpr<TransLog> {
    let mut expr = RecExpr::default();
    expr.add(TransLog::Var(sym));
    expr
}

fn build_literal_expr(sym: Symbol, positive: bool) -> RecExpr<TransLog> {
    if positive {
        build_var_expr(sym)
    } else {
        build_unary_expr(TransLog::Inv(Id::from(0)), &build_var_expr(sym))
    }
}

fn build_minterm(vars: &[Symbol], assignment: &[bool]) -> RecExpr<TransLog> {
    let literals: Vec<RecExpr<TransLog>> = vars
        .iter()
        .zip(assignment.iter())
        .map(|(sym, value)| build_literal_expr(*sym, *value))
        .collect();
    build_and_chain(&literals)
}

fn build_maxterm(vars: &[Symbol], assignment: &[bool]) -> RecExpr<TransLog> {
    let literals: Vec<RecExpr<TransLog>> = vars
        .iter()
        .zip(assignment.iter())
        .map(|(sym, value)| build_literal_expr(*sym, !*value))
        .collect();
    build_or_chain(&literals)
}

fn build_unary_expr(
    mut node: TransLog,
    child: &RecExpr<TransLog>,
) -> RecExpr<TransLog> {
    let mut expr = RecExpr::default();
    let child_root = append_subexpr(&mut expr, child);
    if let Some(slot) = node.children_mut().get_mut(0) {
        *slot = child_root;
    }
    expr.add(node);
    expr
}

fn build_binary_expr(
    mut node: TransLog,
    left: &RecExpr<TransLog>,
    right: &RecExpr<TransLog>,
) -> RecExpr<TransLog> {
    let mut expr = RecExpr::default();
    let left_root = append_subexpr(&mut expr, left);
    let right_root = append_subexpr(&mut expr, right);
    if let Some(children) = node.children_mut().get_mut(0..2) {
        children[0] = left_root;
        children[1] = right_root;
    }
    expr.add(node);
    expr
}

fn build_and_chain(terms: &[RecExpr<TransLog>]) -> RecExpr<TransLog> {
    let mut iter = terms.iter();
    let first = match iter.next() {
        Some(term) => term.clone(),
        None => return RecExpr::default(),
    };
    let mut acc = first;
    for term in iter {
        acc = build_binary_expr(
            TransLog::And([Id::from(0), Id::from(0)]),
            &acc,
            term,
        );
    }
    acc
}

fn build_or_chain(terms: &[RecExpr<TransLog>]) -> RecExpr<TransLog> {
    let mut iter = terms.iter();
    let first = match iter.next() {
        Some(term) => term.clone(),
        None => return RecExpr::default(),
    };
    let mut acc = first;
    for term in iter {
        acc = build_binary_expr(
            TransLog::Or([Id::from(0), Id::from(0)]),
            &acc,
            term,
        );
    }
    acc
}
