# Benchmarks

The programs in [`bench/`](bench/) compare Globw with CPython, each side at full
power: **scalar** tests put the Globw bytecode VM against pure CPython, and
**array** tests put `nungw` against NumPy. This file records *what* is measured,
*how*, and *on which machine*, so the numbers have context and can be reproduced.

## The machine

Every number in this file was measured on one desktop, on 2026-11-27 unless a table
says otherwise:

| Component | Detail |
|-----------|--------|
| **CPU**   | AMD Ryzen 7 9800X3D: 8 cores / 16 threads, 4.7 GHz base, 8 MB L2, 96 MB L3 (3D V-Cache) |
| **RAM**   | 32 GB DDR5 (2 modules, 4800 MT/s) |
| **GPU**   | AMD Radeon RX 9070 XT (used only when the GPU backend is enabled; not in these tests) |
| **OS**    | Windows 11 Pro 10.0.26200, power plan "High performance" |
| **Globw** | 0.1.7, `cargo build --release` with rustc 1.99.0; JIT on Cranelift 0.138 |
| **C++**   | MSVC 19.44 (Visual Studio 2022), `/O2` |
| **Go**    | go 1.27.4 (`go build`, default settings) |
| **Java**  | Java 21.0.11 LTS (HotSpot) |
| **Python**| CPython 3.14.10 (its JIT on with `PYTHON_JIT=1` where stated), NumPy 2.5.4 |

Times are **machine-dependent**: they scale with the CPU, memory bandwidth and
toolchain versions above. Re-run on your own hardware for numbers you can trust; the
commands are below.

## Method

- **Best of N runs** (3 in the main table, 9 in the Go comparison), each run a fresh
  process: start-up included, for every language alike.
- **Interleaved runs** in the Go comparison (Globw, Go, Globw, Go, ...), so background
  noise hits both sides the same way.
- **Results checked**: every program prints its result, and the runner compares it
  with Globw's; a table is only kept when all agree.
- **Fair pairs**: one core against one core (*base vs base*), or every core against
  every core (*max vs max*, Go parallelised by hand with goroutines). Mixing the two
  is said explicitly where it happens.
- Small tests last 50-100 ms, of which ~45-60 ms are the operating system starting a
  process (the same for every language here); the larger tests are for comparing
  compiled speed.

To reproduce: `cargo build --release` in the repository, then in `bench/`:
`cpp\build.bat`, `go\build.bat`, `javac -d java java\*.java`, `python all.py`; for
the large Go comparison `large\build.bat` then `python large\run.py`; for Python at
full power `python pymax\run.py`.

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
each result checked against Globw's). Globw runs twice: as the interpreter alone, and
with its JIT, as `import jit` turns it on. Globw 0.1.7, 2026-11-27:

| Test | Globw | Globw JIT | Python / NumPy | C++ | Go | Java (JIT) | Java bytecode only |
|---|---:|---:|---:|---:|---:|---:|---:|
| Recursive fib(35) | 0.54 s | 0.076 s | 0.74 s | 0.070 s | 0.071 s | 0.19 s | 0.49 s |
| Sum loop, 200 million steps | 1.19 s | 0.060 s * | 3.81 s | 0.14 s | 0.14 s | 0.29 s | 1.23 s |
| Mandelbrot, 900 x 600 | 0.92 s | 0.055 s * | 1.95 s | 0.087 s | 0.11 s | 0.23 s | 0.67 s |
| Sieve up to 10 million | 0.52 s | 0.076 s * | 0.78 s | 0.067 s | 0.075 s | 0.19 s | 0.43 s |
| sqrt(a*a + b*b), 20 M values x 10 | 0.28 s | 0.28 s | 1.12 s | 0.33 s | 0.48 s | 0.60 s | n/a |
| exp, log and sin, 20 M values x 10 | 0.49 s | 0.49 s | 2.70 s | 1.85 s | 3.06 s | 4.90 s | n/a |
| Sum, norm and mean, 50 M values x 20 | 0.46 s | 0.47 s | 1.32 s | 0.87 s | 0.89 s | 1.05 s | n/a |
| Matrix multiply, 1200 x 1200 x 10 | 0.15 s | 0.17 s | 0.20 s | 3.87 s | 3.97 s | 5.57 s | n/a |

\* the JIT runs these loops on every core by itself; the C++, Go and Java versions
are plain loops on one core. For a fair comparison with Go see the next section.
The first four tests are plain code, the last four arrays (`nungw`, NumPy); the
C++, Go and Java array versions are plain loops on one core, which a BLAS library
and threads would speed up. Java's bytecode interpreter alone (`-Xint`) runs the plain
code only. These tests last tens of milliseconds, so process start-up weighs a lot.

## Globw with its JIT against Go, fairly

The plain-code tests made ten times bigger, so the computation decides
(`large/`, `python large/run.py`, best of 9 interleaved runs, results checked):

| Test | base: Globw | base: Go | max: Globw | max: Go (goroutines) |
|---|---:|---:|---:|---:|
| Sum loop, 2 billion steps | 1.008 s | 0.979 s | 0.170 s | 0.184 s |
| Mandelbrot, 2700 x 1800 | 0.512 s | 0.453 s | 0.088 s | 0.076 s |
| Fill and count two 10 M arrays x 100 | 1.573 s | 1.202 s | 0.309 s | 0.247 s |

*base*: one core each (Globw with `GLOBW_JIT_NO_PAR=1`). *max*: every core, Globw as
`import jit` runs it, Go parallelised by hand with goroutines, a `WaitGroup` and
partial sums. Go is ahead by 3-31% on one core and by up to 25% on every core, except
the sum loop, where Globw is 8% ahead. The difference is in what one writes: the
Globw programs are the plain loops, made parallel by the JIT with exactly the
sequential result and errors; the Go ones are 30-40 lines of hand-written concurrency.

## Python at full power

Globw with its JIT against CPython 3.14 with its own JIT (`PYTHON_JIT=1`) and the
standard library only, each test written in its fastest standard-library form
(`pymax/`, `python pymax/run.py`, best of three; Python measured 2026-11-25, Globw
2026-11-27):

| Test | Globw | Python | Globw faster |
|---|---:|---:|---:|
| Recursive fib(35) | 0.076 s | 0.76 s | 10x |
| Sum loop, 200 million steps | 0.060 s | 3.84 s | 64x |
| Mandelbrot, 900 x 600 | 0.055 s | 1.71 s | 31x |
| Sieve up to 10 million | 0.076 s | 0.078 s | even |
| sqrt(a*a + b*b), 20 M values x 10 | 0.28 s | 30.9 s | 110x |
| exp, log and sin, 20 M values x 10 | 0.49 s | 24.3 s | 50x |
| Sum, norm and mean, 50 M values x 20 | 0.47 s | 22.8 s | 49x |
| Matrix multiply, 1200 x 1200 x 10 | 0.17 s | 145 s | 850x |

Python's sieve uses two operations written in C (a `bytearray` slice assignment and
`.count`), and its start-up is longer than the work itself.

## Against C++

The same programs written in C++ are in [`cpp/`](cpp/): plain loops on one core, no
libraries, built with MSVC `/O2` (`cppuild.bat`), sizes read from the command line so
nothing is computed at compile time. `python cpp/compare.py` runs each side three times
(best run, start-up included) and checks that the results agree.

Measured with Globw 0.1.6, the interpreter alone (no JIT):

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
Globw 0.1.6 (the interpreter alone) and Java 21:

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

## How `bench.py` measures

- **Correctness first.** Every test runs both implementations and compares their
  output before trusting the timing. Scalar tests and `trans` must match
  **exactly**; `ew`, `reduce` and `matmul` match within a **1e-6 relative**
  tolerance, since floating-point results depend on summation order.
- **Wall-clock, whole process.** Each side is timed end to end, including process
  startup. `bench.py` takes a single run; `all.py`, `large/run.py` and
  `pymax/run.py` keep the best of several (see *Method* above). Counting start-up
  is deliberately conservative: it counts Globw's start-up against it too.
- **Fair opponents.** Array tests face NumPy with its own optimized BLAS; scalar
  tests face plain CPython, with no C extensions doing the work.
- **CPU path by default.** `nungw`'s GPU backend is opt-in (`GLOBW_GPU=1`) and is
  **off** for these runs unless stated; the array numbers above are CPU vs CPU.

## Caveats

These are honest, reproducible comparisons, not a marketing lab. A single run on
one machine will wobble by a few percent; a shared CI runner would wobble more,
which is why there is no automated benchmark job — the point is the ratio
(*Globw vs the alternative on the same box*), not an absolute score.
