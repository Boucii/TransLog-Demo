use egg::{
    rewrite, EGraph, Id, Rewrite, Subst, Var,
};
use crate::language::TransLog;

fn is_var_only(egraph: &mut EGraph<TransLog, ()>, _id: Id, subst: &Subst) -> bool {
    let var_a: Var = "?a".parse().expect("var ?a should parse");
    let eid = subst[var_a];
    egraph[eid]
        .nodes
        .iter()
        .any(|n| matches!(n, TransLog::Var(_)))
}

/// Returns a vector containing all rewrite rules.
pub fn make_rules() -> Vec<Rewrite<TransLog, ()>> {
    vec![
        // ============================================================
        // 1. Logic Simplification (Reduction)
        //    Goal: Reduce the number of nodes/transistors.
        // ============================================================
        
        // Double Negation Elimination
        rewrite!("double-neg-elim"; "(! (! ?a))" => "?a"),

        // Idempotence: Parallel/Series of same switch = Single switch
        rewrite!("idem-and"; "(* ?a ?a)" => "?a"),
        rewrite!("idem-or";  "(+ ?a ?a)" => "?a"),
        rewrite!("idem-and-expand-var"; "?a" => "(* ?a ?a)" if is_var_only),
        //rewrite!("idem-or-expand-var"; "?a" => "(+ ?a ?a)" if is_var_only),

        // Absorption: Removes redundant branches (e.g., a * (a + b) = a)
        rewrite!("absorb-and"; "(* ?a (+ ?a ?b))" => "?a"),
        rewrite!("absorb-or";  "(+ ?a (* ?a ?b))" => "?a"),
        // Identity & Annihilator
        // Assumes Bool(true) is VDD and Bool(false) is GND
        rewrite!("ann-and-false"; "(* ?a false)" => "false"), 
        rewrite!("ann-or-true";   "(+ ?a true)"  => "true"),  
        rewrite!("id-and-true";   "(* ?a true)"  => "?a"),          
        rewrite!("id-or-false";   "(+ ?a false)" => "?a"),

        // ============================================================
        // 2. Algebraic Properties
        //    Goal: Reorder nodes to find matching patterns.
        // ============================================================
        rewrite!("comm-and";  "(* ?a ?b)" => "(* ?b ?a)"),
        rewrite!("comm-or";   "(+ ?a ?b)" => "(+ ?b ?a)"),
        rewrite!("assoc-and"; "(* ?a (* ?b ?c))" => "(* (* ?a ?b) ?c)"),
        rewrite!("assoc-or";  "(+ ?a (+ ?b ?c))" => "(+ (+ ?a ?b) ?c)"),
        rewrite!("assoc-and-rev"; "(* (* ?a ?b) ?c)" => "(* ?a (* ?b ?c))"),
        rewrite!("assoc-or-rev";  "(+ (+ ?a ?b) ?c)" => "(+ ?a (+ ?b ?c))"),
        rewrite!(
            "or-balance-4";
            "(+ (+ (+ ?x ?y) ?z) ?w)" =>
            "(+ (+ ?x ?z) (+ ?y ?w))"
        ),
        rewrite!(
            "and-balance-4";
            "(* (* (* ?x ?y) ?z) ?w)" =>
            "(* (* ?x ?z) (* ?y ?w))"
        ),

        // ============================================================
        // 3. Structural Manipulation (Distributivity vs. Factorization)
        // ============================================================

        // Distributivity (Expansion)
        // Geometric Impact: Increases parallelism, reduces stack depth, but increases Area.
        rewrite!("distribute-and-over-or"; 
            "(* ?a (+ ?b ?c))" => "(+ (* ?a ?b) (* ?a ?c))"
        ),

        // Factorization (Compression)
        // Geometric Impact: Enables "Diffusion Sharing", reduces transistor count (Area).
        // Note: Generally preferred for minimizing netlist size.
        rewrite!("factor-out-and"; 
            "(+ (* ?a ?b) (* ?a ?c))" => "(* ?a (+ ?b ?c))"
        ),
        rewrite!("factor-out-and-right";
            "(+ (* ?b ?a) (* ?c ?a))" => "(* ?a (+ ?b ?c))"
        ),
        //rewrite!(
        //    "factor-2x2";
        //    "(+ (+ (* ?a ?b) (* ?a ?c)) (+ (* ?d ?b) (* ?d ?c)))" =>
        //    "(* (+ ?a ?d) (+ ?b ?c))"
        //),
        //rewrite!(
        //    "factor-2x2-alt";
        //    "(+ (+ (* ?a ?b) (* ?d ?b)) (+ (* ?a ?c) (* ?d ?c)))" =>
        //    "(* (+ ?a ?d) (+ ?b ?c))"
        //),
        //rewrite!(
        //    "bridge-merge-sums";
        //    "(+ (* ?b (+ ?e (* ?d ?c))) (+ (* ?a (+ ?c (+ ?f ?b))) (bridge ?a ?d ?f ?e ?c)))" =>
        //    "(bridge (+ ?a ?e) (+ ?b ?f) ?d ?a ?c)"
        //),
        //rewrite!(
        //    "bridge-fold-sop-9";
        //    "(+ (+ (+ (+ (+ (+ (+ (+ (* ?a ?f) (* ?a ?b)) (* ?e ?f)) (* ?d ?a)) (* (* ?e ?c) ?a)) (* (* ?d ?c) ?f)) (* (* ?d ?c) ?b)) (* ?a ?c)) (* ?e ?b))" =>
        //    "(bridge (+ ?a ?e) (+ ?b ?f) ?d ?a ?c)"
        //),
        //rewrite!(
        //    "bridge-normalize-sums";
        //    "(bridge ?a ?d (+ ?f ?b) (+ ?a ?e) ?c)" =>
        //    "(bridge (+ ?a ?e) (+ ?b ?f) ?d ?a ?c)"
        //),
         //(x + y) * z = xz + yz
        rewrite!("distribute-right-and-over-or"; 
            "(* (+ ?x ?y) ?z)" => "(+ (* ?x ?z) (* ?y ?z))"
        ),

        // xy + x!y = x * (y + !y) = x * 1 = x
        rewrite!("shannon-reduction"; 
            "(+ (* ?x ?y) (* ?x (! ?y)))" => "?x"
        ),

        // a * !a = 0 (Annihilator for Complements)
        rewrite!("complement-and"; "(* ?a (! ?a))" => "false"),

        // a + !a = 1 (Identity for Complements)
        rewrite!("complement-or";  "(+ ?a (! ?a))" => "true"),

        // ============================================================
        // 4. Bubble Pushing (De Morgan's Laws)
        //    Goal: Move inversions to match available gates (NAND/NOR).
        //    these rules are be acheived using self join synthesis
        // ============================================================

        // AND -> Inverted OR (ab = !(!a + !b))
        rewrite!("bubble-pushing-and"; 
            "(* ?a ?b)" => "(join (+ (! ?a) (! ?b)) (+ (! ?a) (! ?b)))"
        ),
        
        // OR -> Inverted AND (a+b = !(!a * !b))
        rewrite!("bubble-pushing-or"; 
            "(+ ?a ?b)" => "(join (* (! ?a) (! ?b)) (* (! ?a) (! ?b)))"
        ),
        
         //Join-Bubble Extraction
         //Pushes the bubble out of a specific Join structure.
        rewrite!("join-bubble-extraction_and"; 
            "(join (+ ?a ?b) (+ ?a ?b))" => 
            "(! (join (* (! ?a) (! ?b)) (* (! ?a) (! ?b))))"
        ),
        rewrite!("join-bubble-extraction_or"; 
            "(join (* ?a ?b) (* ?a ?b))" => 
            "(! (join (+ (! ?a) (! ?b)) (+ (! ?a) (! ?b))))" 
        ),
        
        // ============================================================
        // 5. Structural Topology & Buffering
        //    Goal: Modify the physical 'join' structure.
        // ============================================================
        // Self-Join Collapse (Reduction)
        // Reduces a redundant structural inverter to a simple wire.
        rewrite!("self-join-collapse"; "(! (join ?x ?x))" => "?x"),
        rewrite!("self-join-collapse2"; "(join ?x ?x)" => "(! ?x)"),
        //
        //// Self-Join Synthesis (Expansion - GENERATIVE)
        //// WARNING: Creates structural inverters. Use with caution/limits.
        //rewrite!("self-join-synthesis"; "?x" => "(! (join ?x ?x))" if is_not_concat),
        rewrite!("self-join-synthesis"; "?x" => "(! (join ?x ?x))"), 
        
        //// Internal Buffer Expansion (AND)
        //// Inserts structural buffering inside an AND configuration.
        rewrite!("buffer-expansion-and"; 
            "(join (* ?a ?b) (* ?a ?b))" => 
            "(join (* ?a (! (join ?b ?b))) (* ?a (! (join ?b ?b))))"
        ),
        
        // Internal Buffer Expansion (OR)
        // Inserts structural buffering inside an OR configuration.
        rewrite!("buffer-expansion-or"; 
            "(join (+ ?a ?b) (+ ?a ?b))" => 
            "(join (+ ?a (! (join ?b ?b))) (+ ?a (! (join ?b ?b))))"
        ),

        // ============================================================
        // 6. Complex Gate Mapping 
        //    Goal: Map logic operators to CMOS 'join' primitives.
        // ============================================================
        //rewrite!("map-inv"; "(! ?a)" => "(join ?a ?a)" ),
        //rewrite!("map-nand"; 
        //    "(! (* ?a ?b))" => "(join (* ?a ?b) (* ?a ?b))"
        //),
        //rewrite!("map-nor"; 
        //    "(! (+ ?a ?b))" => "(join (+ ?a ?b) (+ ?a ?b))"
        //),
        //rewrite!("map-aoi21";
        //    "(! (+ (* ?a ?b) ?c))" => 
        //    "(join (+ (* ?a ?b) ?c) (+ (* ?a ?b) ?c))"
        //),
        //rewrite!("map-oai21";
        //    "(! (* (+ ?a ?b) ?c))" => 
        //    "(join (* (+ ?a ?b) ?c) (* (+ ?a ?b) ?c))"
        //),
        //idem-and-nested: (* ?a (* ?a ?b)) => (* ?a ?b)
        //idem-or-nested: (+ ?a (+ ?a ?b)) => (+ ?a ?b)
    ]
}


pub fn perpendicular_rules() -> Vec<Rewrite<TransLog, ()>> {
    vec![
        // Structural factorization that helps bridge folding.
        rewrite!(
            "perp-factor-out-and";
            "(+ (* ?a ?b) (* ?a ?c))" => "(* ?a (+ ?b ?c))"
        ),
        rewrite!(
            "perp-factor-out-and-right";
            "(+ (* ?b ?a) (* ?c ?a))" => "(* ?a (+ ?b ?c))"
        ),
        
        rewrite!(
            "perp-factor-out-and-keep-right";
            "(+ (* ?x ?z) (* ?y ?z))" => "(* (+ ?x ?y) ?z)"
        ),
        
        rewrite!(
            "perp-factor-nested-and";
            "(+ (* (* ?x ?y) ?a) (* (* ?x ?y) ?b))" => "(* (* ?x ?y) (+ ?a ?b))"
        ),
        rewrite!(
            "perp-factor-nested-and-right";
            "(+ (* ?a (* ?x ?y)) (* ?b (* ?x ?y)))" => "(* (+ ?a ?b) (* ?x ?y))"
        ),
    ]
}

pub fn shortcut_rules() -> Vec<Rewrite<TransLog, ()>> {
    vec![
        rewrite!("shortcut-double-neg-elim"; "(! (! ?a))" => "?a"),
        rewrite!("shortcut-idem-and"; "(* ?a ?a)" => "?a"),
        rewrite!("shortcut-idem-or";  "(+ ?a ?a)" => "?a"),
        rewrite!("shortcut-absorb-and"; "(* ?a (+ ?a ?b))" => "?a"),
        rewrite!("shortcut-absorb-or";  "(+ ?a (* ?a ?b))" => "?a"),
        rewrite!("shortcut-ann-and-false"; "(* ?a false)" => "false"),
        rewrite!("shortcut-ann-or-true";   "(+ ?a true)"  => "true"),
        rewrite!("shortcut-id-and-true";   "(* ?a true)"  => "?a"),
        rewrite!("shortcut-id-or-false";   "(+ ?a false)" => "?a"),
        rewrite!(
            "shortcut-shannon-reduction";
            "(+ (* ?x ?y) (* ?x (! ?y)))" => "?x"
        ),
        rewrite!("shortcut-complement-and"; "(* ?a (! ?a))" => "false"),
        rewrite!("shortcut-complement-or";  "(+ ?a (! ?a))" => "true"),
        rewrite!("shortcut-self-join-collapse"; "(! (join ?x ?x))" => "?x"),
        rewrite!("shortcut-self-join-collapse2"; "(join ?x ?x)" => "(! ?x)"),
    ]
}

pub fn fine_tune_rules() -> Vec<Rewrite<TransLog, ()>> {
    vec![
        rewrite!("fine-comm-and";  "(* ?a ?b)" => "(* ?b ?a)"),
        rewrite!("fine-comm-or";   "(+ ?a ?b)" => "(+ ?b ?a)"),
        rewrite!("fine-assoc-and"; "(* ?a (* ?b ?c))" => "(* (* ?a ?b) ?c)"),
        rewrite!("fine-assoc-or";  "(+ ?a (+ ?b ?c))" => "(+ (+ ?a ?b) ?c)"),
        rewrite!("fine-assoc-and-rev"; "(* (* ?a ?b) ?c)" => "(* ?a (* ?b ?c))"),
        rewrite!("fine-assoc-or-rev";  "(+ (+ ?a ?b) ?c)" => "(+ ?a (+ ?b ?c))"),
        rewrite!(
            "fine-or-balance-4";
            "(+ (+ (+ ?x ?y) ?z) ?w)" =>
            "(+ (+ ?x ?z) (+ ?y ?w))"
        ),
        rewrite!(
            "fine-and-balance-4";
            "(* (* (* ?x ?y) ?z) ?w)" =>
            "(* (* ?x ?z) (* ?y ?w))"
        ),
        rewrite!(
            "fine-bridge-swap-lr";
            "(bridge ?a ?b ?c ?d ?e)" =>
            "(bridge ?c ?d ?a ?b ?e)"
        ),
        rewrite!(
            "fine-bridge-swap-ud";
            "(bridge ?a ?b ?c ?d ?e)" =>
            "(bridge ?b ?a ?d ?c ?e)"
        ),
    ]
}

pub fn generative_rules() -> Vec<Rewrite<TransLog, ()>> {
    vec![
        rewrite!(
            "gen-distribute-and-over-or";
            "(* ?a (+ ?b ?c))" => "(+ (* ?a ?b) (* ?a ?c))"
        ),
        rewrite!(
            "gen-distribute-right-and-over-or";
            "(* (+ ?x ?y) ?z)" => "(+ (* ?x ?z) (* ?y ?z))"
        ),
    ]
}

pub fn bridge_fold_rules() -> Vec<Rewrite<TransLog, ()>> {
    vec![
        rewrite!(
            "fold-factored-to-bridge";
            "(+ (+ (* ?a ?b) (* ?c ?d)) (* ?e (+ (* ?a ?d) (* ?c ?b))))" =>
            "(bridge ?a ?b ?c ?d ?e)"
        ),
        //rewrite!(
        //    "fold-factored-to-bridge-outer-swap";
        //    "(+ (* ?e (+ (* ?a ?d) (* ?c ?b))) (+ (* ?a ?b) (* ?c ?d)))" =>
        //    "(bridge ?a ?b ?c ?d ?e)"
        //),
        rewrite!(
            "fold-cnf-to-bridge";
            "(* (* (+ ?a ?c) (+ ?b ?d)) (* (+ (+ ?a ?e) ?d) (+ (+ ?c ?e) ?b)))" =>
            "(bridge ?a ?b ?c ?d ?e)"
        ),
        //rewrite!(
        //    "fold-cnf-to-bridge-outer-swap";
        //    "(* (* (+ (+ ?a ?e) ?d) (+ (+ ?c ?e) ?b)) (* (+ ?a ?c) (+ ?b ?d)))" =>
        //    "(bridge ?a ?b ?c ?d ?e)"
        //),
        rewrite!(
            "fold-bridge-repeat-d-eq-a";
            "(+ (+ (* ?a ?b) (* ?c ?a)) (* ?e (+ (* ?a ?a) (* ?c ?b))))" =>
            "(bridge ?a ?b ?c ?a ?e)"
        ),
        rewrite!(
            "fold-bridge-repeat-b-eq-c";
            "(+ (+ (* ?a ?b) (* ?b ?d)) (* ?e (+ (* ?a ?d) (* ?b ?b))))" =>
            "(bridge ?a ?b ?b ?d ?e)"
        ),
        rewrite!(
            "fold-bridge-repeat-a-eq-c";
            "(+ (+ (* ?a ?b) (* ?a ?d)) (* ?e (+ (* ?a ?d) (* ?a ?b))))" =>
            "(bridge ?a ?b ?a ?d ?e)"
        ),
        rewrite!(
            "fold-bridge-a-eq-c-factored";
            "(+ (* ?a (+ ?b ?c)) (* ?d (+ (* ?a ?c) (* ?a ?b))))" =>
            "(bridge ?b ?a ?a ?c ?d)"
        ),
        rewrite!(
            "fold-bridge-a-eq-c-nested-or";
            "(+ (* ?a (+ ?b (+ ?c ?d))) (* ?e (+ (* ?a ?d) (* ?a (+ ?b ?c)))))" =>
            "(bridge (+ ?b ?c) ?a ?a ?d ?e)"
        ),
        rewrite!(
            "fold-bridge-triple-or";
            "(+ (* ?a (+ (+ ?b ?c) ?d)) (* ?e (* (+ ?b ?c) ?d)))" =>
            "(bridge (+ ?b ?c) ?a ?a ?d ?e)"
        ),
        rewrite!(
            "fold-bridge-a-eq-c-triple-sum";
            "(+ (* ?a (+ (+ ?b ?c) ?d)) (* (* (* ?b ?c) ?d) ?x))" =>
            "(bridge ?b ?a ?a ?c (* ?d ?x))"
        ),
        rewrite!(
            "fold-bridge-a-eq-c-with-product";
            "(+ (* ?a (+ (+ ?b ?c) ?d)) (* (* ?b ?c) ?d))" =>
            "(bridge ?b ?a ?a ?c ?d)"
        ),
    ]
}

pub fn bridge_fold_sop_rules() -> Vec<Rewrite<TransLog, ()>> {
    vec![
        rewrite!(
            "fold-bridge-sop-left";
            "(+ (+ (* ?a ?b) (* ?c ?d)) (+ (* ?e (* ?a ?d)) (* ?e (* ?c ?b))))" =>
            "(bridge ?a ?b ?c ?d ?e)"
        ),
        rewrite!(
            "fold-bridge-sop-right";
            "(+ (+ (* ?a ?b) (* ?c ?d)) (+ (* (* ?a ?d) ?e) (* (* ?c ?b) ?e)))" =>
            "(bridge ?a ?b ?c ?d ?e)"
        ),
        rewrite!(
            "fold-bridge-sop-nested-left";
            "(+ (+ (* ?a ?b) (* ?c ?d)) (+ (* (* ?e ?f) (* ?a ?d)) (* (* ?e ?f) (* ?c ?b))))" =>
            "(bridge ?a ?b ?c ?d (* ?e ?f))"
        ),
        rewrite!(
            "fold-bridge-sop-nested-right";
            "(+ (+ (* ?a ?b) (* ?c ?d)) (+ (* (* ?a ?d) (* ?e ?f)) (* (* ?c ?b) (* ?e ?f))))" =>
            "(bridge ?a ?b ?c ?d (* ?e ?f))"
        ),
        rewrite!(
            "factor-2x2-general";
            "(+ (+ (* ?a ?b) (* ?a ?c)) (+ (* ?d ?b) (* ?d ?c)))" =>
            "(* (+ ?a ?d) (+ ?b ?c))"
        ),
        rewrite!(
            "fold-bridge-cross-structure";
            "(+ (* ?ae (+ ?b ?c)) (* ?d (+ ?a (* ?b ?c))))" =>
            "(bridge ?a ?d ?b ?ae ?c)"
        ),
    ]
}

pub fn bridge_fold_late_rules() -> Vec<Rewrite<TransLog, ()>> {
    vec![
        rewrite!(
            "late-fold-bridge-cross-simple";
            "(+ (* ?a (+ ?b ?c)) (* ?d (+ ?a (* ?b ?c))))" =>
            "(bridge ?a ?d ?b ?a ?c)"
        ),
    ]
}

pub fn bridge_merge_rules() -> Vec<Rewrite<TransLog, ()>> {
    vec![
        rewrite!(
            "factor-bridge-sum";
            "(+ (* ?x (bridge ?a ?b ?c ?d ?e)) (* ?y (bridge ?a ?b ?c ?d ?e)))" =>
            "(* (+ ?x ?y) (bridge ?a ?b ?c ?d ?e))"
        ),
        rewrite!(
            "factor-common-var-in-sum";
            "(+ (* ?a ?x) (* ?a ?y))" => "(* ?a (+ ?x ?y))"
        ),
    ]
}

pub fn bridge_expand_rules() -> Vec<Rewrite<TransLog, ()>> {
    vec![
        rewrite!(
            "expand-bridge-to-factored";
            "(bridge ?a ?b ?c ?d ?e)" =>
            "(+ (+ (* ?a ?b) (* ?c ?d)) (* ?e (+ (* ?a ?d) (* ?c ?b))))"
        ),
        rewrite!(
            "expand-bridge-to-factored-outer-swap";
            "(bridge ?a ?b ?c ?d ?e)" =>
            "(+ (* ?e (+ (* ?a ?d) (* ?c ?b))) (+ (* ?a ?b) (* ?c ?d)))"
        ),
        rewrite!(
            "expand-bridge-to-cnf";
            "(bridge ?a ?b ?c ?d ?e)" =>
            "(* (* (+ ?a ?c) (+ ?b ?d)) (* (+ (+ ?a ?e) ?d) (+ (+ ?c ?e) ?b)))"
        ),
        rewrite!(
            "expand-bridge-to-cnf-outer-swap";
            "(bridge ?a ?b ?c ?d ?e)" =>
            "(* (* (+ (+ ?a ?e) ?d) (+ (+ ?c ?e) ?b)) (* (+ ?a ?c) (+ ?b ?d)))"
        ),
    ]
}

pub fn bridge_prune_rules() -> Vec<Rewrite<TransLog, ()>> {
    vec![
        rewrite!(
            "prune-bridge-e-false";
            "(bridge ?a ?b ?c ?d false)" =>
            "(+ (* ?a ?b) (* ?c ?d))"
        ),
        rewrite!(
            "prune-bridge-e-true";
            "(bridge ?a ?b ?c ?d true)" =>
            "(* (+ ?a ?c) (+ ?b ?d))"
        ),
    ]
}

pub fn sizing_coarse_grain() -> Vec<Rewrite<TransLog, ()>> {
    vec![
        rewrite!("size-to-x2"; "?a" => "(X ?a 2)" if is_var_only),
        rewrite!("size-to-x4"; "?a" => "(X ?a 4)" if is_var_only),
        rewrite!("size-to-x8"; "?a" => "(X ?a 8)" if is_var_only),

        rewrite!("size-to-x1"; "(X ?a ?b)" => "?a" if is_var_only),
    ]
}

pub fn ruleset_by_name(name: &str) -> Result<Vec<Rewrite<TransLog, ()>>, String> {
    let normalized = name.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "default" | "make_rules" => Ok(make_rules()),
        "perpendicular" | "perpendicular_rules" => Ok(perpendicular_rules()),
        "shortcut" | "shortcut_rules" => Ok(shortcut_rules()),
        "fine_tune" | "fine_tune_rules" => Ok(fine_tune_rules()),
        "generative" | "generative_rules" => Ok(generative_rules()),
        "bridge_fold" | "bridge_fold_rules" => Ok(bridge_fold_rules()),
        "bridge_fold_sop" | "bridge_fold_sop_rules" => {
            Ok(bridge_fold_sop_rules())
        }
        "bridge_fold_late" | "bridge_fold_late_rules" => {
            Ok(bridge_fold_late_rules())
        }
        "bridge_expand" | "bridge_expand_rules" => Ok(bridge_expand_rules()),
        "bridge_prune" | "bridge_prune_rules" => Ok(bridge_prune_rules()),
        "bridge_merge" | "bridge_merge_rules" => Ok(bridge_merge_rules()),
        "sizing_coarse_grain" | "sizing" => Ok(sizing_coarse_grain()),
        _ => Err(format!("unknown ruleset '{}'", name)),
    }
}
