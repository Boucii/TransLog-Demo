use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::Duration;

use egg::{BackoffScheduler, EGraph, Id, RecExpr, Runner};

use NetlistOpt::config::RewriteStageConfig;
use NetlistOpt::extractor::{expr_output_ids, extract_min_transistor_expr_any_root};
use NetlistOpt::language::TransLog;
use NetlistOpt::rules::ruleset_by_name;
use NetlistOpt::utils::to_dnf;

#[derive(Clone)]
pub struct Case {
    pub name: String,
    pub original: String,
}

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn nsp_catalog_root() -> PathBuf {
    project_root().join("testbench").join("53_NSP_Catalog")
}

fn parse_nsp_index(file_name: &str) -> Option<usize> {
    file_name
        .strip_prefix("NSP_")
        .and_then(|s| s.strip_suffix(".txt"))
        .and_then(|s| s.parse::<usize>().ok())
}

pub fn nsp53_cases() -> Vec<Case> {
    let root = nsp_catalog_root();
    let entries = fs::read_dir(&root)
        .unwrap_or_else(|err| panic!("failed to read NSP catalog {:?}: {}", root, err));

    let mut files: Vec<(usize, String, PathBuf)> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            let file_name = path.file_name()?.to_str()?;
            let index = parse_nsp_index(file_name)?;
            Some((index, file_name.trim_end_matches(".txt").to_string(), path))
        })
        .collect();

    files.sort_by_key(|(index, _, _)| *index);

    let mut cases = Vec::with_capacity(files.len());
    for (_, case_name, path) in files {
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {:?}: {}", path, err));
        let original = content
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or_else(|| panic!("empty expression file: {:?}", path))
            .to_string();
        cases.push(Case {
            name: case_name,
            original,
        });
    }

    if cases.is_empty() {
        panic!("no NSP cases found under {:?}", root);
    }

    cases
}

struct SweepConfig {
    name: &'static str,
    stages: Vec<RewriteStageConfig>,
}

fn stage(ruleset: &'static str, iter_limit: usize) -> RewriteStageConfig {
    RewriteStageConfig {
        ruleset: ruleset.to_string(),
        iter_limit,
    }
}

fn report_root() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("rpt");
    fs::create_dir_all(&dir)
        .unwrap_or_else(|err| panic!("failed to create report dir: {}", err));
    dir
}

fn write_report_line(report: &mut BufWriter<File>, line: &str) {
    writeln!(report, "{}", line).unwrap();
}

fn run_rewrite_stages(
    expr: &RecExpr<TransLog>,
    stages: &[RewriteStageConfig],
) -> (EGraph<TransLog, ()>, Id) {
    let mut egraph: EGraph<TransLog, ()> = EGraph::default();
    let root = egraph.add_expr(expr);
    egraph.rebuild();
    let mut root_id = root;
    let mut current_egraph = egraph;

    for stage in stages {
        let rules = ruleset_by_name(&stage.ruleset)
            .unwrap_or_else(|err| panic!("unknown ruleset: {}", err));
        let scheduler = BackoffScheduler::default();
        let mut runner = Runner::default()
            .with_scheduler(scheduler)
            .with_egraph(current_egraph)
            .with_iter_limit(stage.iter_limit)
            .with_time_limit(Duration::from_millis(500))
            .with_node_limit(50_000);
        runner.roots = vec![root_id];
        let runner = runner.run(&rules);
        root_id = runner.egraph.find(root_id);
        current_egraph = runner.egraph;
    }

    (current_egraph, root_id)
}
#[cfg_attr(test, test)]
pub fn nsp53_staged_rewrite_flow_orig_dnf_best() {
    let cases = nsp53_cases();
    let report_path = report_root().join("nsp53_orig_dnf_best_report.txt");
    let report_file = File::create(&report_path)
        .unwrap_or_else(|err| panic!("failed to create report {:?}: {}", report_path, err));
    let mut report = BufWriter::new(report_file);
    write_report_line(&mut report, "nsp53_orig_dnf_best");
    write_report_line(
        &mut report,
        "case|status|cost|expr|chosen|orig_cost|dnf_cost|note",
    );

    let orig_config = SweepConfig {
        name: "orig_m14_with_late_fold",
        stages: vec![
            stage("fine_tune", 5),
            stage("perpendicular", 10),
            stage("bridge_fold_sop", 15),
            stage("bridge_fold", 15),
            stage("bridge_merge", 10),
            stage("fine_tune", 3),
            stage("perpendicular", 6),
            stage("bridge_fold_sop", 8),
            stage("bridge_fold", 8),
            stage("bridge_merge", 6),
            stage("bridge_fold_late", 5),
            stage("bridge_merge", 3),
            stage("fine_tune", 2),
        ],
    };

    let dnf_config = SweepConfig {
        name: "dnf_rebuild_v6_deep_tail",
        stages: vec![
            stage("fine_tune", 4),
            stage("bridge_fold_sop", 7),
            stage("perpendicular", 7),
            stage("bridge_fold", 7),
            stage("bridge_merge", 5),
            stage("bridge_fold_late", 4),
            stage("bridge_fold", 3),
            stage("bridge_merge", 3),
            stage("perpendicular", 3),
            stage("bridge_fold_sop", 3),
            stage("bridge_fold", 3),
            stage("bridge_merge", 2),
        ],
    };

    let mut total_cost = 0u64;
    let mut case_count = 0usize;

    println!(
        "=== Mix Configs: orig={}, dnf={} ===",
        orig_config.name, dnf_config.name
    );

    for case in &cases {
        let orig_expr: RecExpr<TransLog> = match case.original.parse() {
            Ok(parsed) => parsed,
            Err(err) => {
                println!("{}|mix|fail|parse error: {}", case.name, err);
                let row = format!("{}|fail||||||parse error: {}", case.name, err);
                write_report_line(&mut report, &row);
                continue;
            }
        };

        let mut orig_cost: Option<u64> = None;
        let mut orig_best_expr: Option<String> = None;
        let orig_outputs = expr_output_ids(&orig_expr).len();
        if orig_outputs > 0 {
            let (egraph, root) = run_rewrite_stages(&orig_expr, &orig_config.stages);
            match extract_min_transistor_expr_any_root(&egraph, root, orig_outputs) {
                Ok(result) => {
                    orig_cost = Some(result.cost);
                    orig_best_expr = Some(result.expr.to_string());
                }
                Err(err) => {
                    println!("{}|orig|fail|{}", case.name, err);
                    let row = format!("{}|fail||||||orig extract error: {}", case.name, err);
                    write_report_line(&mut report, &row);
                }
            }
        } else {
            println!("{}|orig|fail|expression contains no outputs", case.name);
            let row = format!("{}|fail||||||orig has no outputs", case.name);
            write_report_line(&mut report, &row);
        }

        let mut dnf_cost: Option<u64> = None;
        let mut dnf_best_expr: Option<String> = None;
        match to_dnf(&orig_expr) {
            Ok(dnf_expr) => {
                let dnf_outputs = expr_output_ids(&dnf_expr).len();
                if dnf_outputs > 0 {
                    let (egraph, root) = run_rewrite_stages(&dnf_expr, &dnf_config.stages);
                    match extract_min_transistor_expr_any_root(&egraph, root, dnf_outputs) {
                        Ok(result) => {
                            dnf_cost = Some(result.cost);
                            dnf_best_expr = Some(result.expr.to_string());
                        }
                        Err(err) => {
                            println!("{}|dnf|fail|{}", case.name, err);
                            let row =
                                format!("{}|fail||||||dnf extract error: {}", case.name, err);
                            write_report_line(&mut report, &row);
                        }
                    }
                } else {
                    println!("{}|dnf|fail|expression contains no outputs", case.name);
                    let row = format!("{}|fail||||||dnf has no outputs", case.name);
                    write_report_line(&mut report, &row);
                }
            }
            Err(err) => {
                println!("{}|dnf|fail|to_dnf error: {}", case.name, err);
                let row = format!("{}|fail||||||to_dnf error: {}", case.name, err);
                write_report_line(&mut report, &row);
            }
        }

        let chosen = match (orig_cost, dnf_cost) {
            (Some(oc), Some(dc)) => {
                if dc < oc {
                    Some(("dnf", dc, dnf_best_expr.unwrap_or_default()))
                } else {
                    Some(("orig", oc, orig_best_expr.unwrap_or_default()))
                }
            }
            (Some(oc), None) => Some(("orig", oc, orig_best_expr.unwrap_or_default())),
            (None, Some(dc)) => Some(("dnf", dc, dnf_best_expr.unwrap_or_default())),
            (None, None) => None,
        };

        if let Some((label, cost, expr_text)) = chosen {
            let row = format!(
                "{}|mix|{}|{}|chosen={}|orig_cost={}|dnf_cost={}",
                case.name,
                cost,
                expr_text,
                label,
                orig_cost
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "fail".to_string()),
                dnf_cost
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "fail".to_string())
            );
            println!("{}", row);
            let report_row = format!(
                "{}|ok|{}|{}|{}|{}|{}|",
                case.name,
                cost,
                expr_text,
                label,
                orig_cost
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "fail".to_string()),
                dnf_cost
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "fail".to_string())
            );
            write_report_line(&mut report, &report_row);
            total_cost += cost;
            case_count += 1;
        } else {
            println!("{}|mix|fail|no extraction result", case.name);
            let row = format!("{}|fail||||||no extraction result", case.name);
            write_report_line(&mut report, &row);
        }
    }

    println!("\n=== Orig+DNF Best Summary ===");
    println!("Total cases: {}", case_count);
    println!("Total cost: {}", total_cost);
    if case_count > 0 {
        println!("Average cost: {:.2}", total_cost as f64 / case_count as f64);
    }
    let avg_text = if case_count > 0 {
        format!("{:.2}", total_cost as f64 / case_count as f64)
    } else {
        "-".to_string()
    };
    let summary = format!(
        "summary|ok|{}|||cases={}|average={}||",
        total_cost, case_count, avg_text
    );
    write_report_line(&mut report, &summary);
}
