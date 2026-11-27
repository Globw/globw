"""Globw against Go on problems ten times bigger than the main suite, so the
computation, not the start of the process, decides. Two fair comparisons:

  base vs base  one core each: Globw with its JIT (GLOBW_JIT=1, parallel loops
                off) against plain Go loops
  max vs max    every core: Globw with its JIT, as `import jit` runs it, against
                Go parallelised by hand with goroutines (the *_par programs)

Runs are interleaved and the best of 9 is kept. Build the Go side first: build.bat.
"""
import os
import subprocess
import time

HERE = os.path.dirname(os.path.abspath(__file__))
GW = os.path.join(HERE, "..", "..", "target", "release", "gw.exe")
JIT = {**os.environ, "GLOBW_JIT": "1"}
ONE = {**JIT, "GLOBW_JIT_NO_PAR": "1"}


def run(cmd, env=None):
    t = time.perf_counter()
    r = subprocess.run(cmd, capture_output=True, text=True, env=env)
    return time.perf_counter() - t, r.stdout.strip()


print("| Test | base: Globw | base: Go | max: Globw | max: Go (goroutines) |")
print("|---|---:|---:|---:|---:|")
for name, label in [("sumb", "Sum loop, 2 billion steps"), ("mandel", "Mandelbrot, 2700 x 1800"), ("fill", "Fill and count two 10 M arrays x 100")]:
    gw = [GW, os.path.join(HERE, name + ".gw")]
    go1 = [os.path.join(HERE, "go", name + ".exe")]
    gop = [os.path.join(HERE, "go", name + "_par.exe")]
    best = {"g1": 9e9, "c1": 9e9, "g": 9e9, "c": 9e9}
    outs = set()
    for _ in range(9):
        for key, cmd, env in (("g1", gw, ONE), ("c1", go1, None), ("g", gw, JIT), ("c", gop, None)):
            t, out = run(cmd, env)
            best[key] = min(best[key], t)
            outs.add(out)
    same = "" if len(outs) == 1 else "  (RESULTS DIFFER)"
    print(f"| {label} | {best['g1']:.3f} s | {best['c1']:.3f} s | {best['g']:.3f} s | {best['c']:.3f} s |{same}")
