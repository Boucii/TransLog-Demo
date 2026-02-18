#[path = "../tests/nsp53.rs"]
mod nsp53_run;
#[path = "../tests/pclass3984.rs"]
mod pclass3984_run;

fn main() {
    println!(
        "[START] tests NSP-53 minimal transistor count rewrite flow. " 
    );
    nsp53_run::nsp53_staged_rewrite_flow_orig_dnf_best();
    println!(
        "[START] tests minimal transistor count rewrite flow for PClass-3984. "
    );
    pclass3984_run::min_transistor_joinlike_root_3984_pclass_local_rules_report();
}
