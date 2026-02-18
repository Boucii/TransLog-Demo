# Repository File Map 

## Root

- `Cargo.toml`: Rust package manifest and dependencies.
- `Cargo.lock`: Locked dependency versions for reproducible builds.
- `README.md`: Project overview, run commands, and references.

## Core Source (`src/`)

- `src/lib.rs`: Library module exports.
- `src/main.rs`: Binary entrypoint; runs both demo flows.
- `src/language.rs`: `TransLog` language definition.
- `src/rules.rs`: Rewrite rule sets used by extraction flows.
- `src/input.rs`: Expression/input parsing helpers.
- `src/logic.rs`: Logic expression formatting and utilities.
- `src/utils.rs`: Shared helpers (proof/report/form conversion utilities).
- `src/config.rs`: Rewrite stage config structures.
- `src/log.rs`: Logging placeholder module.

## Extractors (`src/extractors/`)

- `extractor.rs`: Public extractor API and common expression helpers.
- `min_transistor_fixed_point_extractor.rs`: Fixed-point minimum-transistor extractor.
- `min_transistor_context_dp_extractor.rs`: Context-DP minimum-transistor extractor.

## Tests and Examples

- `tests/nsp53.rs`: NSP-53 flow test/runner.
- `tests/pclass3984.rs`: 3984 P-class flow test/runner.
- `examples/nsp53.rs`: Run NSP-53 flow directly.
- `examples/min_trans_extractor_3984_pclass_local_rules_report_rules.rs`: Run 3984 flow directly.

## Bench Data (`testbench/`)

- `testbench/53_NSP_Catalog/NSP_*.txt`: NSP-53 benchmark expressions.
- `testbench/3984_P-class/4input_pclass_s_expr`: 3984 P-class expressions.
- `testbench/3984_P-class/*.py`: Dataset generation/conversion scripts.
- `testbench/3984_P-class/4input_pclass_set.txt`: Auxiliary 3984 dataset file.

## Documentation (`doc/`)

- `doc/files.md`: This concise file index.
- `doc/semantics.md`: Language and rewrite semantics notes.
- `doc/NSP_53_golden.md`: NSP-53 reference notes.
- `doc/min_transistor_fixed_point_extractor.md`: Fixed-point extractor design note.
- `doc/min_transistor_context_dp_extractor.md`: Context-DP extractor design note.
- `doc/simulation.md`: Simulation document (flow removed from this trimmed repo).

## Generated / Local

- `rpt/`: Generated run reports.