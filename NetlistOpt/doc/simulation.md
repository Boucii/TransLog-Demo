# Simulation Templates

**This demo does not contain functions for delay optimizations and simulation.**
This document explains the default SPICE testbench template (`src/process_template/asap7.sp`)
and the simulator command template (`src/process_template/xyce.sh`) used by NetlistOpt.

## Where These Templates Are Used

NetlistOpt reads the template paths from the simulation configuration and
uses them when generating per-expression testbenches and run scripts.
By default, the paths are:

- `src/process_template/asap7.sp` (testbench template)
- `src/process_template/xyce.sh` (simulator command template)

When a simulation is prepared, the templates are read and the placeholders
listed below are replaced with concrete paths or net names. The resulting
testbench (`.sp`) and command script (`.sh`) are written into the per-run
output directory.

## Testbench Template: `asap7.sp`

This template is used to measure delay for one signal and one edge case
(rise or fall) at a time. It is a single-testcase transient simulation
that NetlistOpt instantiates repeatedly for different vectors and edges.
You can customize this file to match your process technology, device models,
or measurement requirements before running the flow.

It includes:

- A library include line (replaced by the model/library file path).
- Supply sources (VDD/VSS).
- Two input stimulus waveforms (`stim_rise` and `stim_fall`).
- A DUT include and DUT instance line injected by the generator.
- A small output load capacitor (`Cload_out`).
- Transient control and `.measure` statements for delay.

### Placeholders

The template uses the following placeholder tokens, which are replaced during
testbench generation:

- `__LIB__`: Path to the model/library file that is copied into the run folder.
- `__CIRCUIT_DEF__`: A `.include "<path>"` line for the generated DUT netlist.
- `__CIRCUIT_INSTANCE__`: The DUT instance line with connected pins.
- `__PIN_DUT__`: The net name that drives the switching input (`stim_rise` or `stim_fall`).
- `__PIN_OUT__`: The output net name used for loading and delay measurement.
- `__EDGE_IN_SPEC__`: Edge spec for the input measurement (`RISE=1` or `FALL=1`).
- `__EDGE_OUT_SPEC__`: Edge spec for the output measurement (`RISE=1` or `FALL=1`).

### Key Parameters

- `VDD`: Default supply voltage (set to `1.8` in the template).
- `TEMP`: Simulation temperature (set to `25`).
- `Cload_out`: Output load capacitor (set to `5f`).
- `.tran 1p 1n`: Transient analysis step and stop time.

### Measurement

The template measures propagation delay by capturing when the input and output
cross `0.5 * VDD` on the selected edges, then computes `delay` using
`TRIG` / `TARG`.

## Simulator Command Template: `xyce.sh`

The command template is a small shell script that:

- Adds Spack to `PATH`.
- Loads the `xyce` package via Spack.
- Changes into the testbench directory.
- Runs `Xyce` on the generated testbench file.

### Placeholders

The template uses the following placeholder tokens, which are replaced during
command generation:

- `__TESTBENCH_DIRECTORY__`: Directory containing the generated testbench.
- `__TESTBENCH_PATH__`: The testbench filename, passed to the simulator.

The generator also accepts the legacy names `__DUT_DIRECTORY__` and
`__DUT_PATH__` as aliases for the same values.

### Output

Xyce writes a `.prn` file next to the testbench by default. NetlistOpt
expects this file when collecting simulation results.
