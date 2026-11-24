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

## Every language in one table

`python all.py` runs every test in every language (best of three, start-up included,
each result checked against Globw's) and prints this table. Globw 0.1.6, CPython 3.14
with NumPy 2.5.4, C++ built with MSVC `/O2` (plain loops, one core), Java 21:

| Test | Globw | Python / NumPy | C++ | Java (JIT) | Java bytecode only |
|---|---:|---:|---:|---:|---:|
| Recursive fib(35) | 0.09 s | 0.75 s | 0.072 s | 0.20 s | 0.49 s |
| Sum loop, 200 million steps | 0.22 s | 3.79 s | 0.14 s | 0.28 s | 1.20 s |
| Mandelbrot, 900 x 600 | 0.11 s | 1.92 s | 0.087 s | 0.22 s | 0.70 s |
| Sieve up to 10 million | 0.26 s | 0.77 s | 0.066 s | 0.18 s | 0.42 s |
| sqrt(a*a + b*b), 20 M values x 10 | 0.34 s | 1.09 s | 0.33 s | 0.59 s | n/a |
| exp, log and sin, 20 M values x 10 | 0.55 s | 2.66 s | 1.85 s | 4.87 s | n/a |
| Sum, norm and mean, 50 M values x 20 | 0.52 s | 1.31 s | 0.88 s | 1.00 s | n/a |
| Matrix multiply, 1200 x 1200 x 10 | 0.22 s | 0.19 s | 3.89 s | 5.54 s | n/a |

Globw here runs with its JIT (on by default since 0.1.7); `GLOBW_NO_JIT=1` runs the
interpreter alone, as in 0.1.6 (fib 0.53 s, sum loop 1.05 s, Mandelbrot 0.94 s, sieve
0.77 s).

The first four are plain code, the last four arrays (`nungw`, NumPy). Java's bytecode
interpreter alone (`-Xint`) is run on plain code only: on the arrays it would take tens
of minutes. The C++ and Java array versions are plain loops on one core; with a BLAS
library and threads they would catch up with `nungw`.

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

## Against Java

The same scalar programs in Java are in [`java/`](java/) (`javac -d . *.java`, then
`python java/compare.py`). Java runs two ways: as it normally runs, where the JIT turns
hot bytecode into machine code, and with `-Xint`, its bytecode interpreter alone, which
is the like-for-like comparison with Globw's VM. Best of three, start-up included,
Globw 0.1.6 and Java 21 on the machine above:

| Test | Globw | Java `-Xint` | Java (JIT) |
|------|------:|-------------:|-----------:|
| `fibb`, recursive `fib(35)` | 0.53 s | 0.48 s | 0.21 s |
| `sumb`, 200,000,000 steps | 1.04 s | 1.24 s | 0.30 s |
| `mandel`, 900×600 | 0.93 s | 0.70 s | 0.24 s |
| `sieve`, up to 10,000,000 | 0.80 s | 0.42 s | 0.21 s |

Bytecode against bytecode, Globw is ahead on the counting loop, about even on
recursion and behind on the float loop and the sieve (array writes go through bounds
checks and tagged values, where the JVM has typed array instructions). With its JIT,
Java is 2.5 to 4 times faster than Globw.

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
