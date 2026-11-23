"""Globw against C++ (MSVC /O2): best of three runs, start-up included; the outputs must match."""
import os, subprocess, sys, time
HERE = os.path.dirname(os.path.abspath(__file__))
GW = os.path.join(HERE, "..", "..", "target", "release", "gw.exe")
TESTS = [("fibb", "recursive fib(35)"), ("sumb", "sum loop, 200 million steps"),
         ("mandel", "Mandelbrot 900 x 600"), ("sieve", "sieve up to 10 million"),
         ("ew", "nungw sqrt(a*a+b*b), 20 M x 10"), ("trans", "nungw exp+log+sin, 20 M x 10"),
         ("reduce", "nungw sum+norm+mean, 50 M x 20"), ("matmul", "nungw matmul 1200^2 x 10")]
def same(a, b):
    try:
        x, y = float(a), float(b); return abs(x - y) <= 1e-9 * max(1.0, abs(x), abs(y))
    except ValueError:
        return a == b
def best(cmd, runs=3):
    t, out = [], None
    for _ in range(runs):
        s = time.perf_counter(); r = subprocess.run(cmd, capture_output=True, text=True); t.append(time.perf_counter() - s)
        out = r.stdout.strip() if r.returncode == 0 else "ERR " + r.stderr.strip()[:120]
    return min(t), out
print(f"{'test':<32} {'Globw':>8} {'C++':>8} {'Globw/C++':>8}  same result")
for name, label in TESTS:
    tg, og = best([GW, os.path.join(HERE, "..", name + ".gw")])
    tc, oc = best([os.path.join(HERE, name + ".exe")])
    print(f"{label:<32} {tg:>7.2f}s {tc:>7.3f}s {tg/tc:>7.2f}x  {'yes' if same(og, oc) else 'NO: ' + og + ' / ' + oc}")
