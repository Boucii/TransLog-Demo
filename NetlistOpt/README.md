# NetlistOpt

This repository is a demo for TransLog with two examples of **minimum-transistor extraction**.
It contains two minimum-transistor extraction flows:
- NSP-53 catalog flow.
- 3984 P-class flow.

Currently it does **not** include delay/simulation/SPICE flows.

## Prerequisites

- Rust toolchain (stable) with Cargo.

Check installation:

```bash
rustc --version
cargo --version
```

## Build

```bash
cargo build --release
```

## Run

Run both flows (same as `src/main.rs`):

```bash
cargo run --release
```

Run NSP-53 only:

```bash
cargo run --release --example nsp53
```

Run 3984 P-class only:

```bash
cargo run --release --example min_trans_extractor_3984_pclass_local_rules_report_rules
```

## Inputs

- NSP-53 cases: `testbench/53_NSP_Catalog/NSP_*.txt`
- 3984 P-class dataset: `testbench/3984_P-class/4input_pclass_s_expr`

## Outputs

Reports are written to `rpt/`:

- `rpt/nsp53_orig_dnf_best_report.txt`
- `rpt/min_trans_extractor_3984_pclass_rules_report.txt`

## Method Summary

Both flows use the same core pattern:
1. Parse input expressions.
2. Run staged rewrite rules.
3. Extract the expression with minimum transistor cost.
4. Write per-case results to report files.

## References

- Logics Lab. 2012. *Catalog of 53 Handmade Optimum Switch Networks*. Federal University of Rio Grande do Sul (UFRGS), Institute of Informatics.
  Source: https://www.inf.ufrgs.br/logics/docman/53_NSP_Catalog.pdf (accessed 2026-01-15).
