from array import array
from math import exp, log, sin
n = 20_000_000
a = array('d', map(float, range(1, n + 1)))
s = 0.0
for _ in range(10):
    s += sum([exp(x * 1e-7) + log(x) + sin(x) for x in a])
print(round(s))
