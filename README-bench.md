# Benchmarks

The programs in [`bench/`](bench/) compare Globw with CPython, each side at full
power: **scalar** tests put the Globw bytecode VM against pure CPython, and
**array** tests put `nungw` against NumPy. This file records *what* is measured,
*how*, and *on which machine*, so the numbers have context and can be reproduced.

## The machine

The published numbers were measured on a single desktop:

| Component | Detail |
|-----------|--------|
| **CPU**   | AMD Ryzen 7 9800X3D — 8 cores / 16 threads |
| **RAM**   | 32 GB |
| **GPU**   | AMD Radeon RX 9070 XT (used only when the GPU backend is enabled) |
| **OS**    | Windows 11 Pro (build 26200) |
| **Rust**  | rustc 1.99.0, built with `cargo build --release` |
| **Python**| CPython 3.14.10 with NumPy 2.5.4 |

Times are **machine-dependent**: they scale with the CPU, memory bandwidth and
toolchain versions above. Treat them as indicative, and re-run on your own
hardware for numbers you can trust.

## The programs

Scalar — Globw VM vs pure CPython (no C extensions):

| Test     | What it does | Size |
|----------|--------------|------|
| `fibb`   | naive recursive Fibonacci | `fib(35)` |
| `sumb`   | a tight counting loop | 200,000,000 iterations |
| `mandel` | Mandelbrot escape-time, float math in nested loops | 900×600, 200 iterations |

Array — `nungw` vs NumPy:

| Test     | What it does | Size |
|----------|--------------|------|
| `ew`     | fused element-wise `exp(a·1e-7) + log(a) + sin(a)` | 20,000,000 elements |
| `trans`  | matrix transpose | 20,000,000 elements |
| `reduce` | full reduction (sum) | 50,000,000 elements |
| `matmul` | dense matrix multiply | 1200 × 1200 |

(`bench/` also holds a few extra pairs — `fib`, `sum`, `sieve` — used by the
simpler PowerShell runner below.)

## How to run

Build the release binary first, then run a harness:

```sh
cargo build --release

# full suite (scalar + array), Python/NumPy side by side
python bench/bench.py

# or a smaller PowerShell subset (fib, sieve, sum) vs CPython
powershell -File bench/run.ps1
```

`bench.py` prints, per test, the Globw time, the Python time, the speedup, and
whether the two outputs matched.

## Methodology

- **Correctness first.** Every test runs both implementations and compares their
  output before trusting the timing. Scalar tests and `trans` must match
  **exactly**; `ew`, `reduce` and `matmul` match within a **1e-6 relative**
  tolerance, since floating-point results depend on summation order.
- **Wall-clock, whole process.** Each side is timed end to end, including process
  startup — a single run, not an averaged microbenchmark. This is deliberately
  conservative: it counts Globw's startup against it too.
- **Fair opponents.** Array tests face NumPy with its own optimized BLAS; scalar
  tests face plain CPython, with no C extensions doing the work.
- **CPU path by default.** `nungw`'s GPU backend is opt-in (`GLOBW_GPU=1`) and is
  **off** for these runs unless stated; the array numbers above are CPU vs CPU.

## Caveats

These are honest, reproducible comparisons, not a marketing lab. A single run on
one machine will wobble by a few percent; a shared CI runner would wobble more,
which is why there is no automated benchmark job — the point is the ratio
(*Globw vs the alternative on the same box*), not an absolute score.
