from array import array
from math import sqrt
n = 20_000_000
a = array('d', map(float, range(n)))
b = a
s = 0.0
for _ in range(10):
    c = array('d', map(sqrt, map(float.__add__, map(float.__mul__, a, a), map(float.__mul__, b, b))))
    s += sum(c)
print(s)
