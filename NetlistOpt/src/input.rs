use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::Path;

use egg::{EGraph, Id, Symbol};

use crate::language::TransLog;

#[derive(Debug)]
pub struct EqnParseResult {
    pub egraph: EGraph<TransLog, ()>,
    pub root_id: Id,
    pub input_order: Vec<String>,
    pub input_ids: Vec<Id>,
    pub output_order: Vec<String>,
    pub output_ids: Vec<Id>,
    pub one_out: bool,
}

#[derive(Debug, Clone)]
pub struct EqnParseError {
    stage: &'static str,
    line: Option<usize>,
    message: String,
}

impl EqnParseError {
    fn new(stage: &'static str, line: Option<usize>, message: impl Into<String>) -> Self {
        Self {
            stage,
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for EqnParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "{}: line {}: {}", self.stage, line, self.message),
            None => write!(f, "{}: {}", self.stage, self.message),
        }
    }
}

impl std::error::Error for EqnParseError {}

pub fn parse_eqn_file<P: AsRef<Path>>(path: P) -> Result<EqnParseResult, EqnParseError> {
    let text = std::fs::read_to_string(&path).map_err(|e| {
        EqnParseError::new(
            "io",
            None,
            format!("failed to read '{}': {}", path.as_ref().display(), e),
        )
    })?;
    let text = preprocess_concat(&text)?;
    parse_eqn_text(&text)
}

pub fn parse_eqn_file_as_joined_circuit<P: AsRef<Path>>(
    path: P,
) -> Result<EqnParseResult, EqnParseError> {
    let mut result = parse_eqn_file(path)?;
    joinize_outputs(&mut result);
    Ok(result)
}

pub fn parse_eqn_file_as_valid_circuit<P: AsRef<Path>>(
    path: P,
) -> Result<EqnParseResult, EqnParseError> {
    let text = std::fs::read_to_string(&path).map_err(|e| {
        EqnParseError::new(
            "io",
            None,
            format!("failed to read '{}': {}", path.as_ref().display(), e),
        )
    })?;
    let text = preprocess_concat(&text)?;
    let mut result = parse_eqn_text_valid(&text)?;
    joinize_outputs(&mut result);
    Ok(result)
}

pub fn parse_eqn_text(text: &str) -> Result<EqnParseResult, EqnParseError> {
    let mut egraph: EGraph<TransLog, ()> = EGraph::default();
    let mut vars: HashMap<String, Id> = HashMap::new();

    let false_id = egraph.add(TransLog::Bool(false));
    let true_id = egraph.add(TransLog::Bool(true));
    vars.insert("0".to_string(), false_id);
    vars.insert("1".to_string(), true_id);

    let mut input_order: Vec<String> = Vec::new();
    let mut input_ids: Vec<Id> = Vec::new();
    let mut output_order: Vec<String> = Vec::new();
    let mut output_ids: Vec<Id> = Vec::new();
    let mut output_set: HashSet<String> = HashSet::new();
    let mut output_map: HashMap<String, Id> = HashMap::new();

    let mut has_inorder = false;
    let mut has_outorder = false;

    for (idx, raw_line) in text.lines().enumerate() {
        let line_no = idx + 1;
        let stripped = strip_inline_comment(raw_line);
        let trimmed = stripped.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("new_") {
            continue;
        }

        let mut line = trimmed.to_string();
        if line.ends_with(';') {
            line.pop();
            line = line.trim_end().to_string();
        }
        if line.contains(';') {
            return Err(EqnParseError::new(
                "parse",
                Some(line_no),
                "unexpected ';' in line",
            ));
        }

        if starts_with_keyword(&line, "INORDER") {
            if has_inorder {
                return Err(EqnParseError::new(
                    "parse",
                    Some(line_no),
                    "duplicate INORDER line",
                ));
            }
            has_inorder = true;
            let tokens = parse_order_tokens(&line, "INORDER", line_no)?;
            for name in tokens {
                let sym = Symbol::from(name.as_str());
                let id = egraph.add(TransLog::Var(sym));
                vars.insert(name.clone(), id);
                input_order.push(name);
                input_ids.push(id);
            }
            continue;
        }

        if starts_with_keyword(&line, "OUTORDER") {
            if has_outorder {
                return Err(EqnParseError::new(
                    "parse",
                    Some(line_no),
                    "duplicate OUTORDER line",
                ));
            }
            has_outorder = true;
            let tokens = parse_order_tokens(&line, "OUTORDER", line_no)?;
            for name in tokens {
                output_set.insert(name.clone());
                output_order.push(name);
            }
            continue;
        }

        let (lhs, rhs) = split_assignment(&line).ok_or_else(|| {
            EqnParseError::new("parse", Some(line_no), "expected '<lhs> = <rhs>'")
        })?;
        let lhs = lhs.trim();
        let rhs = rhs.trim();
        if lhs.is_empty() {
            return Err(EqnParseError::new(
                "parse",
                Some(line_no),
                "missing left-hand side",
            ));
        }
        if !is_valid_ident(lhs) {
            return Err(EqnParseError::new(
                "parse",
                Some(line_no),
                format!("invalid identifier '{}'", lhs),
            ));
        }
        if rhs.is_empty() {
            return Err(EqnParseError::new(
                "parse",
                Some(line_no),
                "missing right-hand side",
            ));
        }

        let rhs_id = parse_rhs(rhs, &mut egraph, &vars, line_no)?;
        vars.insert(lhs.to_string(), rhs_id);
        if output_set.contains(lhs) {
            output_map.insert(lhs.to_string(), rhs_id);
        }
    }

    if !has_inorder {
        return Err(EqnParseError::new(
            "parse",
            None,
            "missing INORDER line",
        ));
    }
    if !has_outorder {
        return Err(EqnParseError::new(
            "parse",
            None,
            "missing OUTORDER line",
        ));
    }

    let mut missing_outputs: Vec<String> = Vec::new();
    for name in &output_order {
        match output_map.get(name) {
            Some(id) => output_ids.push(*id),
            None => missing_outputs.push(name.clone()),
        }
    }
    if !missing_outputs.is_empty() {
        missing_outputs.sort();
        return Err(EqnParseError::new(
            "parse",
            None,
            format!("outputs declared but not assigned: {}", missing_outputs.join(", ")),
        ));
    }

    if output_ids.is_empty() {
        return Err(EqnParseError::new("parse", None, "no outputs found"));
    }

    let one_out = output_ids.len() == 1;
    let root_id = if one_out {
        output_ids[0]
    } else {
        let mut concat = egraph.add(TransLog::Concat([output_ids[0], output_ids[1]]));
        for id in output_ids.iter().skip(2) {
            concat = egraph.add(TransLog::Concat([concat, *id]));
        }
        concat
    };

    egraph.rebuild();

    Ok(EqnParseResult {
        egraph,
        root_id,
        input_order,
        input_ids,
        output_order,
        output_ids,
        one_out,
    })
}

fn parse_eqn_text_valid(text: &str) -> Result<EqnParseResult, EqnParseError> {
    let mut egraph: EGraph<TransLog, ()> = EGraph::default();
    let mut vars: HashMap<String, Id> = HashMap::new();

    let false_id = egraph.add(TransLog::Bool(false));
    let true_id = egraph.add(TransLog::Bool(true));
    vars.insert("0".to_string(), false_id);
    vars.insert("1".to_string(), true_id);

    let mut input_order: Vec<String> = Vec::new();
    let mut input_ids: Vec<Id> = Vec::new();
    let mut output_order: Vec<String> = Vec::new();
    let mut output_ids: Vec<Id> = Vec::new();
    let mut output_set: HashSet<String> = HashSet::new();
    let mut output_map: HashMap<String, Id> = HashMap::new();

    let mut has_inorder = false;
    let mut has_outorder = false;

    for (idx, raw_line) in text.lines().enumerate() {
        let line_no = idx + 1;
        let stripped = strip_inline_comment(raw_line);
        let trimmed = stripped.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("new_") {
            continue;
        }

        let mut line = trimmed.to_string();
        if line.ends_with(';') {
            line.pop();
            line = line.trim_end().to_string();
        }
        if line.contains(';') {
            return Err(EqnParseError::new(
                "parse",
                Some(line_no),
                "unexpected ';' in line",
            ));
        }

        if starts_with_keyword(&line, "INORDER") {
            if has_inorder {
                return Err(EqnParseError::new(
                    "parse",
                    Some(line_no),
                    "duplicate INORDER line",
                ));
            }
            has_inorder = true;
            let tokens = parse_order_tokens(&line, "INORDER", line_no)?;
            for name in tokens {
                let sym = Symbol::from(name.as_str());
                let id = egraph.add(TransLog::Var(sym));
                vars.insert(name.clone(), id);
                input_order.push(name);
                input_ids.push(id);
            }
            continue;
        }

        if starts_with_keyword(&line, "OUTORDER") {
            if has_outorder {
                return Err(EqnParseError::new(
                    "parse",
                    Some(line_no),
                    "duplicate OUTORDER line",
                ));
            }
            has_outorder = true;
            let tokens = parse_order_tokens(&line, "OUTORDER", line_no)?;
            for name in tokens {
                output_set.insert(name.clone());
                output_order.push(name);
            }
            continue;
        }

        let (lhs, rhs) = split_assignment(&line).ok_or_else(|| {
            EqnParseError::new("parse", Some(line_no), "expected '<lhs> = <rhs>'")
        })?;
        let lhs = lhs.trim();
        let rhs = rhs.trim();
        if lhs.is_empty() {
            return Err(EqnParseError::new(
                "parse",
                Some(line_no),
                "missing left-hand side",
            ));
        }
        if !is_valid_ident(lhs) {
            return Err(EqnParseError::new(
                "parse",
                Some(line_no),
                format!("invalid identifier '{}'", lhs),
            ));
        }
        if rhs.is_empty() {
            return Err(EqnParseError::new(
                "parse",
                Some(line_no),
                "missing right-hand side",
            ));
        }

        let rhs_id = parse_rhs_valid(rhs, &mut egraph, &vars, line_no)?;
        vars.insert(lhs.to_string(), rhs_id);
        if output_set.contains(lhs) {
            output_map.insert(lhs.to_string(), rhs_id);
        }
    }

    if !has_inorder {
        return Err(EqnParseError::new(
            "parse",
            None,
            "missing INORDER line",
        ));
    }
    if !has_outorder {
        return Err(EqnParseError::new(
            "parse",
            None,
            "missing OUTORDER line",
        ));
    }

    let mut missing_outputs: Vec<String> = Vec::new();
    for name in &output_order {
        match output_map.get(name) {
            Some(id) => output_ids.push(*id),
            None => missing_outputs.push(name.clone()),
        }
    }
    if !missing_outputs.is_empty() {
        missing_outputs.sort();
        return Err(EqnParseError::new(
            "parse",
            None,
            format!("outputs declared but not assigned: {}", missing_outputs.join(", ")),
        ));
    }

    if output_ids.is_empty() {
        return Err(EqnParseError::new("parse", None, "no outputs found"));
    }

    let one_out = output_ids.len() == 1;
    let root_id = if one_out {
        output_ids[0]
    } else {
        let mut concat = egraph.add(TransLog::Concat([output_ids[0], output_ids[1]]));
        for id in output_ids.iter().skip(2) {
            concat = egraph.add(TransLog::Concat([concat, *id]));
        }
        concat
    };

    egraph.rebuild();

    Ok(EqnParseResult {
        egraph,
        root_id,
        input_order,
        input_ids,
        output_order,
        output_ids,
        one_out,
    })
}

fn preprocess_concat(text: &str) -> Result<String, EqnParseError> {
    let mut out_lines: Vec<String> = Vec::new();
    let mut buffer = String::new();
    let mut in_order: Option<&'static str> = None;
    let mut start_line: usize = 0;

    for (idx, raw_line) in text.lines().enumerate() {
        let line_no = idx + 1;
        let stripped = strip_inline_comment(raw_line);
        let trimmed = stripped.trim();

        if in_order.is_none() {
            if trimmed.is_empty() {
                out_lines.push(String::new());
                continue;
            }
            if starts_with_keyword(trimmed, "INORDER") || starts_with_keyword(trimmed, "OUTORDER") {
                in_order = Some(if starts_with_keyword(trimmed, "INORDER") {
                    "INORDER"
                } else {
                    "OUTORDER"
                });
                start_line = line_no;
                buffer.clear();
                buffer.push_str(trimmed);
                if trimmed.ends_with(';') {
                    out_lines.push(buffer.clone());
                    buffer.clear();
                    in_order = None;
                } else {
                    buffer.push(' ');
                }
                continue;
            }
            out_lines.push(trimmed.to_string());
            continue;
        }

        if trimmed.is_empty() {
            continue;
        }
        if starts_with_keyword(trimmed, "INORDER") || starts_with_keyword(trimmed, "OUTORDER") {
            return Err(EqnParseError::new(
                "preprocess_concat",
                Some(line_no),
                "missing ';' before new order line",
            ));
        }
        buffer.push_str(trimmed);
        if trimmed.ends_with(';') {
            out_lines.push(buffer.clone());
            buffer.clear();
            in_order = None;
        } else {
            buffer.push(' ');
        }
    }

    if in_order.is_some() {
        return Err(EqnParseError::new(
            "preprocess_concat",
            Some(start_line),
            "unterminated INORDER/OUTORDER block",
        ));
    }

    Ok(out_lines.join("\n"))
}

fn parse_order_tokens(
    line: &str,
    keyword: &'static str,
    line_no: usize,
) -> Result<Vec<String>, EqnParseError> {
    let rhs = line
        .splitn(2, '=')
        .nth(1)
        .ok_or_else(|| {
            EqnParseError::new(
                "parse",
                Some(line_no),
                format!("missing '=' in {}", keyword),
            )
        })?
        .trim();
    if rhs.is_empty() {
        return Err(EqnParseError::new(
            "parse",
            Some(line_no),
            format!("empty {} list", keyword),
        ));
    }
    let mut tokens: Vec<String> = Vec::new();
    for token in rhs.split_whitespace() {
        if !is_valid_ident(token) {
            return Err(EqnParseError::new(
                "parse",
                Some(line_no),
                format!("invalid identifier '{}'", token),
            ));
        }
        tokens.push(token.to_string());
    }
    if tokens.is_empty() {
        return Err(EqnParseError::new(
            "parse",
            Some(line_no),
            format!("empty {} list", keyword),
        ));
    }
    Ok(tokens)
}

fn joinize_outputs(result: &mut EqnParseResult) {
    let mut joined_ids: Vec<Id> = Vec::with_capacity(result.output_ids.len());
    for id in &result.output_ids {
        let join_id = result
            .egraph
            .add(TransLog::Join([*id, *id]));
        let inv_id = result.egraph.add(TransLog::Inv(join_id));
        joined_ids.push(inv_id);
    }
    result.output_ids = joined_ids;
    result.one_out = result.output_ids.len() == 1;
    result.root_id = if result.one_out {
        result.output_ids[0]
    } else {
        let mut concat = result
            .egraph
            .add(TransLog::Concat([result.output_ids[0], result.output_ids[1]]));
        for id in result.output_ids.iter().skip(2) {
            concat = result.egraph.add(TransLog::Concat([concat, *id]));
        }
        concat
    };
    result.egraph.rebuild();
}

fn parse_rhs(
    rhs: &str,
    egraph: &mut EGraph<TransLog, ()>,
    vars: &HashMap<String, Id>,
    line_no: usize,
) -> Result<Id, EqnParseError> {
    let trimmed = rhs.trim();
    if trimmed.is_empty() {
        return Err(EqnParseError::new(
            "parse",
            Some(line_no),
            "empty right-hand side",
        ));
    }

    let plus_count = trimmed.matches('+').count();
    let star_count = trimmed.matches('*').count();

    if plus_count > 0 && star_count > 0 {
        return Err(EqnParseError::new(
            "parse",
            Some(line_no),
            "RHS cannot contain both '+' and '*'",
        ));
    }
    if plus_count > 1 || star_count > 1 {
        return Err(EqnParseError::new(
            "parse",
            Some(line_no),
            "RHS contains more than one operator",
        ));
    }

    if plus_count == 1 {
        let (left, right) = split_binary(trimmed, '+', line_no)?;
        let lhs_id = parse_operand(left, egraph, vars, line_no)?;
        let rhs_id = parse_operand(right, egraph, vars, line_no)?;
        return Ok(egraph.add(TransLog::Or([lhs_id, rhs_id])));
    }
    if star_count == 1 {
        let (left, right) = split_binary(trimmed, '*', line_no)?;
        let lhs_id = parse_operand(left, egraph, vars, line_no)?;
        let rhs_id = parse_operand(right, egraph, vars, line_no)?;
        return Ok(egraph.add(TransLog::And([lhs_id, rhs_id])));
    }

    parse_operand(trimmed, egraph, vars, line_no)
}

fn parse_rhs_valid(
    rhs: &str,
    egraph: &mut EGraph<TransLog, ()>,
    vars: &HashMap<String, Id>,
    line_no: usize,
) -> Result<Id, EqnParseError> {
    let trimmed = rhs.trim();
    if trimmed.is_empty() {
        return Err(EqnParseError::new(
            "parse",
            Some(line_no),
            "empty right-hand side",
        ));
    }

    let plus_count = trimmed.matches('+').count();
    let star_count = trimmed.matches('*').count();

    if plus_count > 0 && star_count > 0 {
        return Err(EqnParseError::new(
            "parse",
            Some(line_no),
            "RHS cannot contain both '+' and '*'",
        ));
    }
    if plus_count > 1 || star_count > 1 {
        return Err(EqnParseError::new(
            "parse",
            Some(line_no),
            "RHS contains more than one operator",
        ));
    }

    if plus_count == 1 {
        let (left, right) = split_binary(trimmed, '+', line_no)?;
        let lhs_id = parse_operand_valid(left, egraph, vars, line_no)?;
        let rhs_id = parse_operand_valid(right, egraph, vars, line_no)?;
        return Ok(egraph.add(TransLog::Or([lhs_id, rhs_id])));
    }
    if star_count == 1 {
        let (left, right) = split_binary(trimmed, '*', line_no)?;
        let lhs_id = parse_operand_valid(left, egraph, vars, line_no)?;
        let rhs_id = parse_operand_valid(right, egraph, vars, line_no)?;
        return Ok(egraph.add(TransLog::And([lhs_id, rhs_id])));
    }

    parse_operand_valid(trimmed, egraph, vars, line_no)
}

fn parse_operand(
    token: &str,
    egraph: &mut EGraph<TransLog, ()>,
    vars: &HashMap<String, Id>,
    line_no: usize,
) -> Result<Id, EqnParseError> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err(EqnParseError::new(
            "parse",
            Some(line_no),
            "empty operand",
        ));
    }

    let (negated, rest) = if trimmed.starts_with('!') {
        (true, trimmed[1..].trim())
    } else {
        (false, trimmed)
    };

    if rest.is_empty() {
        return Err(EqnParseError::new(
            "parse",
            Some(line_no),
            "missing operand after '!'",
        ));
    }
    if rest.contains('!') {
        return Err(EqnParseError::new(
            "parse",
            Some(line_no),
            "unexpected '!' in operand",
        ));
    }
    if rest.split_whitespace().count() != 1 {
        return Err(EqnParseError::new(
            "parse",
            Some(line_no),
            format!("invalid operand '{}'", rest),
        ));
    }
    if !(rest == "0" || rest == "1" || is_valid_ident(rest)) {
        return Err(EqnParseError::new(
            "parse",
            Some(line_no),
            format!("invalid token '{}'", rest),
        ));
    }
    let base_id = vars.get(rest).copied().ok_or_else(|| {
        EqnParseError::new(
            "parse",
            Some(line_no),
            format!("undefined variable '{}'", rest),
        )
    })?;

    if negated {
        Ok(egraph.add(TransLog::Inv(base_id)))
    } else {
        Ok(base_id)
    }
}

fn parse_operand_valid(
    token: &str,
    egraph: &mut EGraph<TransLog, ()>,
    vars: &HashMap<String, Id>,
    line_no: usize,
) -> Result<Id, EqnParseError> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err(EqnParseError::new(
            "parse",
            Some(line_no),
            "empty operand",
        ));
    }

    let (negated, rest) = if trimmed.starts_with('!') {
        (true, trimmed[1..].trim())
    } else {
        (false, trimmed)
    };

    if rest.is_empty() {
        return Err(EqnParseError::new(
            "parse",
            Some(line_no),
            "missing operand after '!'",
        ));
    }
    if rest.contains('!') {
        return Err(EqnParseError::new(
            "parse",
            Some(line_no),
            "unexpected '!' in operand",
        ));
    }
    if rest.split_whitespace().count() != 1 {
        return Err(EqnParseError::new(
            "parse",
            Some(line_no),
            format!("invalid operand '{}'", rest),
        ));
    }
    if !(rest == "0" || rest == "1" || is_valid_ident(rest)) {
        return Err(EqnParseError::new(
            "parse",
            Some(line_no),
            format!("invalid token '{}'", rest),
        ));
    }
    let base_id = vars.get(rest).copied().ok_or_else(|| {
        EqnParseError::new(
            "parse",
            Some(line_no),
            format!("undefined variable '{}'", rest),
        )
    })?;

    if negated {
        if has_or_or_and(egraph, base_id) {
            Ok(egraph.add(TransLog::Join([base_id, base_id])))
        } else {
            Ok(egraph.add(TransLog::Inv(base_id)))
        }
    } else {
        Ok(base_id)
    }
}

fn has_or_or_and(egraph: &EGraph<TransLog, ()>, id: Id) -> bool {
    let class_id = egraph.find(id);
    egraph[class_id]
        .nodes
        .iter()
        .any(|node| matches!(node, TransLog::Or(_) | TransLog::And(_)))
}

fn split_binary<'a>(
    text: &'a str,
    op: char,
    line_no: usize,
) -> Result<(&'a str, &'a str), EqnParseError> {
    let mut iter = text.splitn(2, op);
    let left = iter.next().unwrap_or("");
    let right = iter.next().unwrap_or("");
    if right.is_empty() {
        return Err(EqnParseError::new(
            "parse",
            Some(line_no),
            "missing operand in binary expression",
        ));
    }
    Ok((left, right))
}

fn split_assignment(line: &str) -> Option<(&str, &str)> {
    let mut iter = line.splitn(2, '=');
    let lhs = iter.next()?;
    let rhs = iter.next()?;
    Some((lhs, rhs))
}

fn is_valid_ident(token: &str) -> bool {
    let mut chars = token.chars().peekable();
    let first = match chars.next() {
        Some(ch) => ch,
        None => return false,
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }

    while let Some(&ch) = chars.peek() {
        if ch == '[' {
            break;
        }
        if !(ch.is_ascii_alphanumeric() || ch == '_') {
            return false;
        }
        chars.next();
    }

    if let Some(ch) = chars.next() {
        if ch != '[' {
            return false;
        }
        let mut has_digit = false;
        while let Some(&digit) = chars.peek() {
            if digit == ']' {
                break;
            }
            if !digit.is_ascii_digit() {
                return false;
            }
            has_digit = true;
            chars.next();
        }
        if !has_digit {
            return false;
        }
        match chars.next() {
            Some(']') => {}
            _ => return false,
        }
    }

    chars.next().is_none()
}

fn starts_with_keyword(line: &str, keyword: &str) -> bool {
    if !line.starts_with(keyword) {
        return false;
    }
    let rest = &line[keyword.len()..];
    rest.is_empty() || rest.starts_with(|ch: char| ch.is_whitespace() || ch == '=')
}

fn strip_inline_comment(line: &str) -> &str {
    match line.split_once('#') {
        Some((before, _)) => before,
        None => line,
    }
}
