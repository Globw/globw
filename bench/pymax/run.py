"""Globw at full power (import jit + nungw) against Python at full power with no
library outside the standard one: CPython 3.14 with its JIT (PYTHON_JIT=1), each
test written in the fastest form the standard library allows. Best of 3."""
import os, subprocess, sys, time
HERE = os.path.dirname(os.path.abspath(__file__))
GW = os.path.join(HERE, "..", "..", "target", "release", "gw.exe")
TESTS = [("fibb", "Recursive fib(35)"), ("sumb", "Sum loop, 200 million steps"), ("mandel", "Mandelbrot, 900 x 600"),
         ("sieve", "Sieve up to 10 million"), ("ew", "sqrt(a*a + b*b), 20 M values x 10"), ("trans", "exp, log and sin, 20 M values x 10"),
         ("reduce", "Sum, norm and mean, 50 M values x 20"), ("matmul", "Matrix multiply, 1200 x 1200 x 10")]
def best(cmd, env, runs=3):
    ts = []
    for _ in range(runs):
        t = time.perf_counter(); r = subprocess.run(cmd, capture_output=True, text=True, env={**os.environ, **env}); ts.append(time.perf_counter() - t)
        if r.returncode: return None, "ERR " + r.stderr.strip()[-200:]
    return min(ts), r.stdout.strip()
def same(a, b):
    try: x, y = float(a), float(b); return abs(x - y) <= 1e-9 * max(1, abs(x), abs(y))
    except ValueError: return a == b
cell = lambda t: "n/a" if t is None else (f"{t:.2f} s" if t >= 0.1 else f"{t:.3f} s")
rows = []
for name, label in TESTS:
    g, og = best([GW, os.path.join(HERE, "..", name + ".gw")], {"GLOBW_JIT": "1"})
    p, op = best([sys.executable, os.path.join(HERE, name + ".py")], {"PYTHON_JIT": "1"})
    rows.append((label, g, p, same(og, op)))
    print(f"{label:<38} {cell(g):>8} {cell(p):>8}  {'ok' if same(og, op) else 'DIFF ' + og + ' / ' + op}", flush=True)
print("\n| Test | Globw (JIT + nungw) | Python 3.14 (JIT, stdlib) | Globw faster |\n|---|---:|---:|---:|")
for label, g, p, ok in rows:
    print(f"| {label} | {cell(g)} | {cell(p)} | {p / g:.1f}x |" if g and p else f"| {label} | {cell(g)} | {cell(p)} | |")
