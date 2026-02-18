use egg::{Id, RecExpr, Symbol};
use std::collections::HashMap;
use crate::language::TransLog;

pub fn extract_logic_expr(input_expr: &RecExpr<TransLog>) -> (RecExpr<TransLog>, String) {
    let root = input_expr.as_ref().len() - 1;
    let mut new_expr = RecExpr::default();
    
    copy_and_simplify(root, input_expr, &mut new_expr);

    let expr_str = format_logic_expr(&new_expr);
    (new_expr, expr_str)
}

pub fn extract_logic_expr_with_outputs(
    input_expr: &RecExpr<TransLog>,
    output_names: &[String],
) -> Result<(RecExpr<TransLog>, String), String> {
    let root = input_expr.as_ref().len().checked_sub(1).ok_or_else(|| {
        "empty expression".to_string()
    })?;
    let mut new_expr = RecExpr::default();

    copy_and_simplify(root, input_expr, &mut new_expr);

    let expr_str = format_logic_expr_with_outputs(&new_expr, output_names)?;
    Ok((new_expr, expr_str))
}

fn copy_and_simplify(
    id: usize,
    old_expr: &RecExpr<TransLog>,
    new_expr: &mut RecExpr<TransLog>,
) -> Id {
    let node = &old_expr.as_ref()[id];
    match node {
        // Join(a, b) 变成 !a
        TransLog::Join([pun, _pdn]) => {
            let pun_idx: usize = (*pun).into();
            let new_a = copy_and_simplify(pun_idx, old_expr, new_expr);
            new_expr.add(TransLog::Inv(new_a))
        }
        TransLog::Bridge([a, b, c, d, e]) => {
            let new_a = copy_and_simplify((*a).into(), old_expr, new_expr);
            let new_b = copy_and_simplify((*b).into(), old_expr, new_expr);
            let new_c = copy_and_simplify((*c).into(), old_expr, new_expr);
            let new_d = copy_and_simplify((*d).into(), old_expr, new_expr);
            let new_e = copy_and_simplify((*e).into(), old_expr, new_expr);
            new_expr.add(TransLog::Bridge([new_a, new_b, new_c, new_d, new_e]))
        }
        TransLog::Concat([a, b]) => {
            let new_a = copy_and_simplify((*a).into(), old_expr, new_expr);
            let new_b = copy_and_simplify((*b).into(), old_expr, new_expr);
            new_expr.add(TransLog::Concat([new_a, new_b]))
        }
        TransLog::And([a, b]) => {
            let new_a = copy_and_simplify((*a).into(), old_expr, new_expr);
            let new_b = copy_and_simplify((*b).into(), old_expr, new_expr);
            new_expr.add(TransLog::And([new_a, new_b]))
        }
        TransLog::Or([a, b]) => {
            let new_a = copy_and_simplify((*a).into(), old_expr, new_expr);
            let new_b = copy_and_simplify((*b).into(), old_expr, new_expr);
            new_expr.add(TransLog::Or([new_a, new_b]))
        }
        TransLog::Inv(a) => {
            let new_a = copy_and_simplify((*a).into(), old_expr, new_expr);
            new_expr.add(TransLog::Inv(new_a))
        }
        TransLog::Size([a, b]) => {
            let new_a = copy_and_simplify((*a).into(), old_expr, new_expr);
            let new_b = copy_and_simplify((*b).into(), old_expr, new_expr);
            new_expr.add(TransLog::Size([new_a, new_b]))
        }
        TransLog::Var(sym) => new_expr.add(TransLog::Var(*sym)),
        TransLog::Bool(b) => new_expr.add(TransLog::Bool(*b)),
    }
}

fn format_logic_expr(expr: &RecExpr<TransLog>) -> String {
    let parts = build_logic_parts(expr);
    format_logic_with_parts(expr, &parts)
}

pub fn format_logic_expr_with_outputs(
    expr: &RecExpr<TransLog>,
    output_names: &[String],
) -> Result<String, String> {
    let parts = build_logic_parts(expr);
    let root_idx = expr
        .as_ref()
        .len()
        .checked_sub(1)
        .ok_or_else(|| "empty expression".to_string())?;
    let mut outputs: Vec<usize> = Vec::new();
    if matches!(expr.as_ref()[root_idx], TransLog::Concat(_)) {
        collect_concat_outputs(expr, root_idx, &mut outputs);
    } else {
        outputs.push(root_idx);
    }
    if outputs.len() != output_names.len() {
        return Err(format!(
            "output name count {} does not match outputs {}",
            output_names.len(),
            outputs.len()
        ));
    }
    let mut lines = Vec::with_capacity(outputs.len());
    for (idx, name) in outputs.iter().zip(output_names.iter()) {
        lines.push(format!("{} = {}", name, parts[*idx]));
    }
    Ok(lines.join("\n"))
}

fn build_logic_parts(expr: &RecExpr<TransLog>) -> Vec<String> {
    let mut parts: Vec<String> = Vec::with_capacity(expr.as_ref().len());

    for node in expr.as_ref().iter() {
        let s = match node {
            TransLog::Bool(b) => b.to_string(),
            TransLog::Var(sym) => sym.to_string(),
            TransLog::And([a, b]) => {
                let ia: usize = (*a).into();
                let ib: usize = (*b).into();
                format!("(* {} {})", parts[ia], parts[ib])
            }
            TransLog::Or([a, b]) => {
                let ia: usize = (*a).into();
                let ib: usize = (*b).into();
                format!("(+ {} {})", parts[ia], parts[ib])
            }
            TransLog::Inv(a) => {
                let ia: usize = (*a).into();
                format!("(! {})", parts[ia])
            }
            TransLog::Join([a, b]) => {
                let ia: usize = (*a).into();
                let ib: usize = (*b).into();
                format!("(join {} {})", parts[ia], parts[ib])
            }
            TransLog::Bridge([a, b, c, d, e]) => {
                let ia: usize = (*a).into();
                let ib: usize = (*b).into();
                let ic: usize = (*c).into();
                let id: usize = (*d).into();
                let ie: usize = (*e).into();
                let ab = format!("(* {} {})", parts[ia], parts[ib]);
                let cd = format!("(* {} {})", parts[ic], parts[id]);
                let ad = format!("(* {} {})", parts[ia], parts[id]);
                let cb = format!("(* {} {})", parts[ic], parts[ib]);
                let cross = format!("(+ {} {})", ad, cb);
                let e_cross = format!("(* {} {})", parts[ie], cross);
                format!("(+ (+ {} {}) {})", ab, cd, e_cross)
            }
            TransLog::Concat([a, b]) => {
                let ia: usize = (*a).into();
                let ib: usize = (*b).into();
                format!("(& {} {})", parts[ia], parts[ib])
            }
            TransLog::Size([a, b]) => {
                let ia: usize = (*a).into();
                let ib: usize = (*b).into();
                // Emit as "<signal>X<size>", e.g., aX4
                format!("{}X{}", parts[ia], parts[ib])
            }
        };
        parts.push(s);
    }

    parts
}

fn format_logic_with_parts(expr: &RecExpr<TransLog>, parts: &[String]) -> String {
    let root_idx = match expr.as_ref().len().checked_sub(1) {
        Some(idx) => idx,
        None => return String::new(),
    };

    if matches!(expr.as_ref()[root_idx], TransLog::Concat(_)) {
        let mut outputs: Vec<usize> = Vec::new();
        collect_concat_outputs(expr, root_idx, &mut outputs);
        return outputs
            .into_iter()
            .map(|idx| parts[idx].clone())
            .collect::<Vec<String>>()
            .join("\n");
    }

    parts.last().cloned().unwrap_or_default()
}

fn collect_concat_outputs(expr: &RecExpr<TransLog>, id: usize, outputs: &mut Vec<usize>) {
    match &expr.as_ref()[id] {
        TransLog::Concat([left, right]) => {
            collect_concat_outputs(expr, (*left).into(), outputs);
            collect_concat_outputs(expr, (*right).into(), outputs);
        }
        _ => outputs.push(id),
    }
}
    pub fn eval_rec_expr(
        expr: &RecExpr<TransLog>,
        env: &HashMap<Symbol, bool>,
    ) -> bool {
        let nodes = expr.as_ref();
        let mut values: Vec<bool> = Vec::with_capacity(nodes.len());
    
        for node in nodes.iter() {
            let v = match node {
                TransLog::Bool(b) => *b,
                TransLog::Var(sym) => {
                    *env.get(sym).unwrap_or(&false)
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
                TransLog::Size([a, _scale]) => {
                    let ia: usize = (*a).into();
                    values[ia]
                }
                TransLog::Join([a, _b]) => {
                    let ia: usize = (*a).into();
                    !values[ia]
                }
                TransLog::Bridge([a, b, c, d, e]) => {
                    let ia: usize = (*a).into();
                    let ib: usize = (*b).into();
                    let ic: usize = (*c).into();
                    let id: usize = (*d).into();
                    let ie: usize = (*e).into();
                    let ab = values[ia] && values[ib];
                    let cd = values[ic] && values[id];
                    let aed = values[ia] && values[ie] && values[id];
                    let ceb = values[ic] && values[ie] && values[ib];
                    ab || cd || aed || ceb
                }
                TransLog::Concat([_a, b]) => {
                    let ib: usize = (*b).into();
                    values[ib]
                }
            };
            values.push(v);
        }
    
        values.last().copied().unwrap_or(false)
    }
