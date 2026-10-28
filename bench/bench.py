#!/usr/bin/env python3
"""Globw vs Python benchmark. Both at max power:
   scalar tests -> Globw VM vs pure CPython; array tests -> nungw vs NumPy."""
import subprocess, time, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
GW = os.path.join(HERE, "..", "target", "release", "gw.exe")
PY = sys.executable

# name, category, compare: "exact" or "approx"
TESTS = [
    ("fibb",   "scalar (vs CPython)", "exact"),
    ("sumb",   "scalar (vs CPython)", "exact"),
    ("mandel", "scalar (vs CPython)", "exact"),
    ("ew",     "array  (vs NumPy)",   "approx"),
    ("trans",  "array  (vs NumPy)",   "exact"),
    ("reduce", "array  (vs NumPy)",   "approx"),
    ("matmul", "array  (vs NumPy)",   "approx"),
]

def run(cmd):
    t = time.perf_counter()
    out = subprocess.run(cmd, capture_output=True, text=True)
    dt = time.perf_counter() - t
    if out.returncode != 0:
        return dt, "ERR: " + (out.stderr.strip() or out.stdout.strip())[:200]
    return dt, out.stdout.strip()

def same(a, b, mode):
    if mode == "exact":
        return a == b
    try:
        x, y = float(a), float(b)
        return abs(x - y) <= 1e-6 * max(1.0, abs(x), abs(y))
    except ValueError:
        return a == b

print(f"globw: {GW}\npython: {PY}\n")
print(f"{'test':<8} {'kind':<20} {'globw':>9} {'python':>9} {'speedup':>9}  match")
print("-" * 70)
rows = []
for name, kind, mode in TESTS:
    gw = os.path.join(HERE, name + ".gw")
    py = os.path.join(HERE, name + ".py")
    tg, og = run([GW, gw])
    tp, op = run([PY, py])
    ok = same(og, op, mode)
    sp = (tp / tg) if tg > 0 else 0.0
    rows.append((name, kind, tg, tp, sp, ok, og, op))
    print(f"{name:<8} {kind:<20} {tg:>8.2f}s {tp:>8.2f}s {sp:>8.1f}x  {'OK' if ok else 'DIFF'}")

print("\nOutputs (globw | python):")
for name, kind, tg, tp, sp, ok, og, op in rows:
    print(f"  {name:<8} {og[:30]!r} | {op[:30]!r}")
