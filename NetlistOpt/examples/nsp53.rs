// Run command (release):
// cargo run --release --example nsp53

#[path = "../tests/nsp53.rs"]
mod nsp53_run;

fn main() {
    nsp53_run::nsp53_staged_rewrite_flow_orig_dnf_best();
}
