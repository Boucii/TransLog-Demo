use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egg::{BackoffScheduler, EGraph, Id, RecExpr, Runner};

use NetlistOpt::config::RewriteStageConfig;
use NetlistOpt::extractor::{
    expr_output_ids, extract_min_transistor_expr,
};
use NetlistOpt::language::TransLog;
use NetlistOpt::rules;
use NetlistOpt::utils::{to_cnf, to_dnf};

#[derive(Clone)]
struct SweepConfig {
    name: &'static str,
    stages: Vec<RewriteStageConfig>,
}

#[derive(Clone)]
struct TaskResult {
    config_name: &'static str,
    cost: u64,
    outputs: usize,
    expr: String,
    expr_nodes: usize,
    rounds_used: usize,
}

#[derive(Clone, Copy)]
struct RunnerLimits {
    stage_time_limit_ms: u64,
    stage_node_limit: usize,
}

#[derive(Clone, Copy)]
struct SearchPolicy {
    seed_mode: SeedMode,
    dnf_max_input_nodes: usize,
    extra_rounds: usize,
    hard_case_cost_threshold: u64,
}

fn stage(ruleset: &'static str, iter_limit: usize) -> RewriteStageConfig {
    RewriteStageConfig {
        ruleset: ruleset.to_string(),
        iter_limit,
    }
}

fn parse_positive_usize(var: &str) -> Option<usize> {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&v| v > 0)
}

fn parse_positive_u64(var: &str) -> Option<u64> {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&v| v > 0)
}

fn parse_non_negative_usize(var: &str) -> Option<usize> {
    std::env::var(var).ok().and_then(|v| v.parse::<usize>().ok())
}

fn parse_case_limit() -> Option<usize> {
    parse_positive_usize("MIN_TRANS_CASE_LIMIT")
}

fn parse_case_start() -> usize {
    parse_positive_usize("MIN_TRANS_CASE_START").unwrap_or(1)
}

fn parse_joinlike_rounds() -> usize {
    parse_positive_usize("MIN_TRANS_JOINLIKE_ROUNDS").unwrap_or(1)
}

fn runner_limits() -> RunnerLimits {
    RunnerLimits {
        stage_time_limit_ms: parse_positive_u64("MIN_TRANS_STAGE_TIME_LIMIT_MS").unwrap_or(500),
        stage_node_limit: parse_positive_usize("MIN_TRANS_STAGE_NODE_LIMIT").unwrap_or(50_000),
    }
}

fn parse_bool_env(var: &str, default: bool) -> bool {
    match std::env::var(var) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
}

#[derive(Clone, Copy)]
enum SeedMode {
    None,
    Cnf,
    Dnf,
    Both,
}

impl SeedMode {
    fn label(self) -> &'static str {
        match self {
            SeedMode::None => "none",
            SeedMode::Cnf => "cnf",
            SeedMode::Dnf => "dnf",
            SeedMode::Both => "both",
        }
    }
}

fn parse_seed_mode() -> SeedMode {
    if let Ok(mode) = std::env::var("MIN_TRANS_SEED_MODE") {
        match mode.trim().to_ascii_lowercase().as_str() {
            "none" => return SeedMode::None,
            "cnf" => return SeedMode::Cnf,
            "dnf" => return SeedMode::Dnf,
            "both" => return SeedMode::Both,
            _ => {}
        }
    }

    if parse_bool_env("MIN_TRANS_ENABLE_CNF_DNF_SEEDS", false) {
        SeedMode::Both
    } else {
        SeedMode::None
    }
}

fn parse_search_policy() -> SearchPolicy {
    SearchPolicy {
        seed_mode: parse_seed_mode(),
        dnf_max_input_nodes: parse_positive_usize("MIN_TRANS_DNF_MAX_INPUT_NODES").unwrap_or(48),
        extra_rounds: parse_non_negative_usize("MIN_TRANS_JOINLIKE_EXTRA_ROUNDS").unwrap_or(0),
        hard_case_cost_threshold: parse_positive_u64("MIN_TRANS_HARD_CASE_COST_THRESHOLD")
            .unwrap_or(28),
    }
}

fn joinlike_portfolio_configs() -> Vec<SweepConfig> {
    let m6 = SweepConfig {
        name: "m6_mk3_perp7_fsop6_fold6_merge4",
        stages: vec![
            stage("make_rules", 3),
            stage("perpendicular", 7),
            stage("bridge_fold_sop", 6),
            stage("bridge_fold", 6),
            stage("bridge_merge", 4),
        ],
    };

    let j1 = SweepConfig {
        name: "j1_mk2_fold2_merge2",
        stages: vec![
            stage("make_rules", 2),
            stage("bridge_fold", 2),
            stage("bridge_merge", 2),
        ],
    };

    let j3 = SweepConfig {
        name: "j3_ft1_mk3_perp2_fsop1_fold3_merge2",
        stages: vec![
            stage("fine_tune", 1),
            stage("make_rules", 3),
            stage("perpendicular", 2),
            stage("bridge_fold_sop", 1),
            stage("bridge_fold", 3),
            stage("bridge_merge", 2),
        ],
    };

    let j7 = SweepConfig {
        name: "j7_ft2_mk3_gen2_short2",
        stages: vec![
            stage("fine_tune", 2),
            stage("make_rules", 3),
            stage("generative", 2),
            stage("shortcut", 2),
        ],
    };

    let j10 = SweepConfig {
        name: "j10_ft2_mk3_bexp3_prune3_short2",
        stages: vec![
            stage("fine_tune", 2),
            stage("make_rules", 3),
            stage("bridge_expand", 3),
            stage("bridge_prune", 3),
            stage("shortcut", 2),
        ],
    };

    let named = vec![m6.clone(), j1.clone(), j3.clone(), j7.clone(), j10.clone()];

    let profile = std::env::var("MIN_TRANS_JOINLIKE_PROFILE")
        .unwrap_or_else(|_| "quality".to_string())
        .to_ascii_lowercase();

    if let Some(rest) = profile.strip_prefix("single:") {
        let target = rest.trim();
        return named.into_iter().filter(|cfg| cfg.name == target).collect();
    }

    if let Some(rest) = profile.strip_prefix("set:") {
        let wanted: BTreeSet<String> = rest
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        let selected: Vec<SweepConfig> = named
            .into_iter()
            .filter(|cfg| wanted.contains(cfg.name))
            .collect();
        if !selected.is_empty() {
            return selected;
        }
    }

    match profile.as_str() {
        "speed" => vec![j1, m6],
        "quality" => vec![m6, j1, j3, j7, j10],
        _ => vec![m6, j1, j3],
    }
}

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn report_root() -> PathBuf {
    let dir = project_root().join("rpt");
    fs::create_dir_all(&dir)
        .unwrap_or_else(|err| panic!("failed to create report dir: {}", err));
    dir
}

fn parse_report_tag() -> Option<String> {
    std::env::var("MIN_TRANS_REPORT_TAG")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn apply_report_tag(file_name: &str) -> String {
    match parse_report_tag() {
        Some(tag) => {
            if let Some(stem) = file_name.strip_suffix(".txt") {
                format!("{}.{}.txt", stem, tag)
            } else {
                format!("{}.{}", file_name, tag)
            }
        }
        None => file_name.to_string(),
    }
}

fn write_report_line(report: &mut BufWriter<File>, line: &str) {
    writeln!(report, "{}", line).unwrap();
    println!("{}", line);
    report.flush().unwrap();
}

fn run_rewrite_stages(
    expr: &RecExpr<TransLog>,
    stages: &[RewriteStageConfig],
    limits: RunnerLimits,
) -> (EGraph<TransLog, ()>, Id) {
    let mut egraph: EGraph<TransLog, ()> = EGraph::default();
    let root = egraph.add_expr(expr);
    egraph.rebuild();
    let mut root_id = root;
    let mut current_egraph = egraph;

    for stage in stages {
        let rules = rules::ruleset_by_name(&stage.ruleset)
            .unwrap_or_else(|err| panic!("unknown ruleset: {}", err));
        let scheduler = BackoffScheduler::default();
        let mut runner = Runner::default()
            .with_scheduler(scheduler)
            .with_egraph(current_egraph)
            .with_iter_limit(stage.iter_limit)
            .with_time_limit(Duration::from_millis(limits.stage_time_limit_ms))
            .with_node_limit(limits.stage_node_limit);
        runner.roots = vec![root_id];
        let runner = runner.run(&rules);
        root_id = runner.egraph.find(root_id);
        current_egraph = runner.egraph;
    }

    (current_egraph, root_id)
}

fn is_better(a: &TaskResult, b: &TaskResult) -> bool {
    (a.cost, a.expr_nodes) < (b.cost, b.expr_nodes)
}

fn run_joinlike_case_portfolio(
    case_expr: &str,
    configs: &[SweepConfig],
    rounds: usize,
    limits: RunnerLimits,
    policy: SearchPolicy,
) -> Result<TaskResult, String> {
    let mut seed_set: BTreeSet<String> = BTreeSet::new();
    seed_set.insert(case_expr.to_string());
    if !matches!(policy.seed_mode, SeedMode::None) {
        if let Ok(parsed) = case_expr.parse::<RecExpr<TransLog>>() {
            let input_nodes = parsed.as_ref().len();
            if matches!(policy.seed_mode, SeedMode::Cnf | SeedMode::Both) {
                if let Ok(cnf_expr) = to_cnf(&parsed) {
                    seed_set.insert(cnf_expr.to_string());
                }
            }
            if matches!(policy.seed_mode, SeedMode::Dnf | SeedMode::Both)
                && input_nodes <= policy.dnf_max_input_nodes
            {
                if let Ok(dnf_expr) = to_dnf(&parsed) {
                    seed_set.insert(dnf_expr.to_string());
                }
            }
        }
    }
    let mut seeds: Vec<String> = seed_set.into_iter().collect();
    let mut global_best: Option<TaskResult> = None;
    let mut errors: Vec<String> = Vec::new();
    let base_rounds = rounds.max(1);
    let mut planned_rounds = base_rounds;
    let mut round_idx = 0usize;
    while round_idx < planned_rounds {
        let mut best_by_config: BTreeMap<&'static str, TaskResult> = BTreeMap::new();

        for seed_expr in &seeds {
            let expr: RecExpr<TransLog> = match seed_expr.parse() {
                Ok(parsed) => parsed,
                Err(err) => {
                    errors.push(format!("seed parse error: {}", err));
                    continue;
                }
            };
            let expected_outputs = expr_output_ids(&expr).len();
            if expected_outputs == 0 {
                errors.push("seed expression has no outputs".to_string());
                continue;
            }

            for config in configs {
                let (egraph, root) = run_rewrite_stages(&expr, &config.stages, limits);
                match extract_min_transistor_expr(&egraph, root, expected_outputs) {
                    Ok(result) => {
                        let candidate = TaskResult {
                            config_name: config.name,
                            cost: result.cost,
                            outputs: expr_output_ids(&result.expr).len(),
                            expr_nodes: result.expr.as_ref().len(),
                            expr: result.expr.to_string(),
                            rounds_used: round_idx + 1,
                        };

                        if let Some(current) = best_by_config.get(config.name) {
                            if is_better(&candidate, current) {
                                best_by_config.insert(config.name, candidate.clone());
                            }
                        } else {
                            best_by_config.insert(config.name, candidate.clone());
                        }

                        if let Some(current) = &global_best {
                            if is_better(&candidate, current) {
                                global_best = Some(candidate);
                            }
                        } else {
                            global_best = Some(candidate);
                        }
                    }
                    Err(err) => {
                        errors.push(format!("{}: {}", config.name, err));
                    }
                }
            }
        }

        if best_by_config.is_empty() {
            break;
        }

        let mut next_seeds = BTreeSet::new();
        for best in best_by_config.values() {
            next_seeds.insert(best.expr.clone());
        }
        seeds = next_seeds.into_iter().collect();
        round_idx += 1;
        if round_idx == base_rounds && policy.extra_rounds > 0 {
            if let Some(best) = &global_best {
                if best.cost >= policy.hard_case_cost_threshold {
                    planned_rounds = planned_rounds.saturating_add(policy.extra_rounds);
                }
            }
        }
    }

    global_best.ok_or_else(|| {
        if errors.is_empty() {
            "portfolio produced no result".to_string()
        } else {
            format!("portfolio produced no result; first error: {}", errors[0])
        }
    })
}

#[cfg_attr(test, test)]
pub fn min_transistor_joinlike_root_3984_pclass_local_rules_report() {
    let test_start = Instant::now();
    let input_path = project_root()
        .join("testbench")
        .join("3984_P-class")
        .join("4input_pclass_s_expr");
    let content = std::fs::read_to_string(&input_path).unwrap_or_else(|err| {
        panic!("failed to read 3984 pclass s_expr file: {}", err)
    });

    let limits = runner_limits();
    let case_limit = parse_case_limit();
    let case_start = parse_case_start();
    let rounds = parse_joinlike_rounds();
    let configs = joinlike_portfolio_configs();
    let policy = parse_search_policy();

    let report_file_name = apply_report_tag("min_trans_extractor_3984_pclass_rules_report.txt");
    let report_path = report_root().join(report_file_name);
    let report_file =
        File::create(&report_path).unwrap_or_else(|err| panic!("failed to create report: {}", err));
    let mut report = BufWriter::new(report_file);
    write_report_line(
        &mut report,
        "min_trans_extractor_3984_pclass_rules_portfolio",
    );
    write_report_line(&mut report, "line|config|status|cost|outputs|expr|note");

    let lines: Vec<&str> = content.split('\n').collect();
    let start_index = case_start.saturating_sub(1).min(lines.len());
    let end_index = case_limit
        .map(|n| start_index.saturating_add(n).min(lines.len()))
        .unwrap_or(lines.len());
    let mut total = 0usize;
    let mut ok = 0usize;
    let mut fail = 0usize;
    let mut cost_sum = 0u64;
    let mut cost_min: Option<u64> = None;
    let mut cost_max: Option<u64> = None;
    let mut wins: BTreeMap<&'static str, usize> = BTreeMap::new();

    for (index, raw_line) in lines
        .iter()
        .enumerate()
        .skip(start_index)
        .take(end_index.saturating_sub(start_index))
    {
        let line_no = index + 1;
        let expr_text = raw_line.trim_end_matches('\r').trim();
        total += 1;

        if expr_text.is_empty() {
            fail += 1;
            let row = format!("{}|portfolio|fail|-|-|-|empty expression", line_no);
            write_report_line(&mut report, &row);
            continue;
        }

        let parse_ok = expr_text.parse::<RecExpr<TransLog>>();
        if let Err(err) = parse_ok {
            fail += 1;
            let row = format!("{}|portfolio|fail|-|-|-|parse error: {}", line_no, err);
            write_report_line(&mut report, &row);
            continue;
        }

        match run_joinlike_case_portfolio(
            expr_text,
            &configs,
            rounds,
            limits,
            policy,
        ) {
            Ok(best) => {
                ok += 1;
                cost_sum = cost_sum.saturating_add(best.cost);
                cost_min = Some(cost_min.map_or(best.cost, |v| v.min(best.cost)));
                cost_max = Some(cost_max.map_or(best.cost, |v| v.max(best.cost)));
                *wins.entry(best.config_name).or_insert(0) += 1;

                let row = format!(
                    "{}|portfolio|ok|{}|{}|{}|best_config={};rounds={};rounds_used={}",
                    line_no,
                    best.cost,
                    best.outputs,
                    best.expr,
                    best.config_name,
                    rounds,
                    best.rounds_used
                );
                write_report_line(&mut report, &row);
            }
            Err(err) => {
                fail += 1;
                let row = format!("{}|portfolio|fail|-|-|-|{}", line_no, err);
                write_report_line(&mut report, &row);
            }
        }
    }

    let (min_text, max_text, avg_text) = if ok > 0 {
        let min_value = cost_min.unwrap_or(0);
        let max_value = cost_max.unwrap_or(0);
        let avg_value = cost_sum as f64 / ok as f64;
        (
            min_value.to_string(),
            max_value.to_string(),
            format!("{:.3}", avg_value),
        )
    } else {
        ("-".to_string(), "-".to_string(), "-".to_string())
    };

    let summary = format!(
        "summary|portfolio|configs={}|rounds={}|extra_rounds={}|hard_case_threshold={}|total={}|ok={}|fail={}|total_cost={}|min={}|avg={}|max={}|case_start={}|case_limit={}|seed_mode={}|dnf_max_input_nodes={}",
        configs.len(),
        rounds,
        policy.extra_rounds,
        policy.hard_case_cost_threshold,
        total,
        ok,
        fail,
        cost_sum,
        min_text,
        avg_text,
        max_text,
        case_start,
        case_limit
            .map(|n| n.to_string())
            .unwrap_or_else(|| "-".to_string()),
        policy.seed_mode.label(),
        policy.dnf_max_input_nodes,
    );
    write_report_line(&mut report, &summary);

    for (config_name, win_count) in wins {
        let line = format!("win|{}|count={}", config_name, win_count);
        write_report_line(&mut report, &line);
    }

    let elapsed = test_start.elapsed();
    let runtime_line = format!(
        "runtime|elapsed_ms={}|elapsed_sec={:.3}",
        elapsed.as_millis(),
        elapsed.as_secs_f64()
    );
    write_report_line(&mut report, &runtime_line);
}
