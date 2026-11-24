from array import array
from math import sqrt, sumprod
n = 50_000_000
a = array('d', map(float, range(n)))
s = 0.0
for _ in range(20):
    t = sum(a)
    s += t + sqrt(sumprod(a, a)) + t / n
print(s)
