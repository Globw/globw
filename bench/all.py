"""Every benchmark, every language, one table.

Globw against Python (pure CPython for plain code, NumPy for arrays), C++ (MSVC /O2,
plain loops on one core, cpp/), Java with its JIT and Java's bytecode interpreter alone
(-Xint, plain code only; java/). Best of three runs, start-up included; every result is
checked against Globw's. Build first: cpp\\build.bat and `javac -d java java\\*.java`.
"""
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
GW = os.path.join(HERE, "..", "target", "release", "gw.exe")
PY = sys.executable
JAVA = ["java", "-cp", os.path.join(HERE, "java")]

# file, Java class, label, plain code (True) or arrays (False)
TESTS = [
    ("fibb", "Fibb", "Recursive fib(35)", True),
    ("sumb", "Sumb", "Sum loop, 200 million steps", True),
    ("mandel", "Mandel", "Mandelbrot, 900 x 600", True),
    ("sieve", "Sieve", "Sieve up to 10 million", True),
    ("ew", "Ew", "sqrt(a*a + b*b), 20 M values x 10", False),
    ("trans", "Trans", "exp, log and sin, 20 M values x 10", False),
    ("reduce", "Reduce", "Sum, norm and mean, 50 M values x 20", False),
    ("matmul", "Matmul", "Matrix multiply, 1200 x 1200 x 10", False),
]


def best(cmd, runs=3):
    times, out = [], None
    for _ in range(runs):
        t = time.perf_counter()
        r = subprocess.run(cmd, capture_output=True, text=True)
        times.append(time.perf_counter() - t)
        if r.returncode != 0:
            return None, "ERR " + (r.stderr or r.stdout).strip()[:150]
        out = r.stdout.strip()
    return min(times), out


def same(a, b):
    try:
        x, y = float(a), float(b)
        return abs(x - y) <= 1e-9 * max(1.0, abs(x), abs(y))
    except (TypeError, ValueError):
        return a == b


def cell(t):
    return "n/a" if t is None else (f"{t:.2f} s" if t >= 0.1 else f"{t:.3f} s")


rows = []
for name, cls, label, plain in TESTS:
    g, og = best([GW, os.path.join(HERE, name + ".gw")])
    p, op = best([PY, os.path.join(HERE, name + ".py")])
    c, oc = best([os.path.join(HERE, "cpp", name + ".exe")])
    j, oj = best(JAVA + [cls])
    x, ox = best(JAVA[:1] + ["-Xint"] + JAVA[1:] + [cls]) if plain else (None, og)
    bad = [who for who, o in (("Python", op), ("C++", oc), ("Java", oj), ("Java -Xint", ox)) if not same(og, o)]
    rows.append((label, g, p, c, j, x, bad))
    print(f"{label:<38} {cell(g):>8} {cell(p):>8} {cell(c):>8} {cell(j):>8} {cell(x):>8}  {'ok' if not bad else 'DIFF ' + ','.join(bad)}", flush=True)

print()
print("| Test | Globw | Python / NumPy | C++ | Java (JIT) | Java bytecode only |")
print("|---|---:|---:|---:|---:|---:|")
for label, g, p, c, j, x, bad in rows:
    print(f"| {label} | {cell(g)} | {cell(p)} | {cell(c)} | {cell(j)} | {cell(x)} |")
