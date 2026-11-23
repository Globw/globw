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
| `ew`     | fused element-wise `sqrt(a·a + b·b)`, then the sum | 20,000,000 elements, 10 times |
| `trans`  | fused element-wise `exp(a·1e-7) + log(a) + sin(a)`, then the sum | 20,000,000 elements, 10 times |
| `reduce` | sum, norm and mean | 50,000,000 elements, 20 times |
| `matmul` | dense matrix multiply, then the sum | 1200 × 1200, 10 times |

(`bench/` also holds a few extra pairs — `fib`, `sum`, `sieve` — used by the
simpler PowerShell runner below.)

## Against C++

The same programs written in C++ are in [`cpp/`](cpp/): plain loops on one core, no
libraries, built with MSVC `/O2` (`cppuild.bat`), sizes read from the command line so
nothing is computed at compile time. `python cpp/compare.py` runs each side three times
(best run, start-up included) and checks that the results agree.

Measured with Globw 0.1.6 on the machine above:

| Test | Globw | C++ | |
|------|------:|----:|--|
| `fibb`, recursive `fib(35)` | 0.51 s | 0.072 s | C++ 7.1× faster |
| `sumb`, 200,000,000 steps | 1.02 s | 0.147 s | C++ 7.0× faster |
| `mandel`, 900×600 | 0.92 s | 0.115 s | C++ 8.0× faster |
| `sieve`, up to 10,000,000 | 0.80 s | 0.091 s | C++ 8.9× faster |
| `ew` (nungw) | 0.37 s | 0.358 s | even |
| `trans` (nungw) | 0.58 s | 1.85 s | Globw 3.2× faster |
| `reduce` (nungw) | 0.55 s | 0.89 s | Globw 1.6× faster |
| `matmul` (nungw) | 0.23 s | 3.89 s | Globw 17× faster |

Plain code runs on Globw's bytecode VM, so compiled C++ is 7 to 9 times faster. On
arrays `nungw` uses every core, SIMD and a blocked matrix kernel, against C++ loops on
one core: C++ with a BLAS library and OpenMP would match or beat it, the matrix
multiply most of all. `ew` is limited by memory bandwidth on both sides.

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
