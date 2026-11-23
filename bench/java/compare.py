"""Globw against Java: the JVM as it runs (JIT) and its bytecode interpreter alone (-Xint).
Best of three runs, start-up included; the outputs must match."""
import os, subprocess, time
HERE = os.path.dirname(os.path.abspath(__file__))
GW = os.path.join(HERE, "..", "..", "target", "release", "gw.exe")
TESTS = [("fibb", "Fibb", "recursive fib(35)"), ("sumb", "Sumb", "sum loop, 200 million steps"),
         ("mandel", "Mandel", "Mandelbrot 900 x 600"), ("sieve", "Sieve", "sieve up to 10 million")]
def best(cmd, runs=3):
    t, out = [], None
    for _ in range(runs):
        s = time.perf_counter(); r = subprocess.run(cmd, capture_output=True, text=True, cwd=HERE); t.append(time.perf_counter() - s)
        out = r.stdout.strip() if r.returncode == 0 else "ERR " + r.stderr.strip()[:120]
    return min(t), out
print(f"{'test':<30} {'Globw':>8} {'Java JIT':>9} {'Java -Xint':>11}  same result")
for gw, cls, label in TESTS:
    tg, og = best([GW, os.path.join(HERE, "..", gw + ".gw")])
    tj, oj = best(["java", "-cp", HERE, cls])
    ti, oi = best(["java", "-Xint", "-cp", HERE, cls])
    ok = og == oj == oi
    print(f"{label:<30} {tg:>7.2f}s {tj:>8.2f}s {ti:>10.2f}s  {'yes' if ok else 'NO: ' + og + ' / ' + oj + ' / ' + oi}")
