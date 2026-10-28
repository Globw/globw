import numpy as np
n = 20_000_000
a = np.arange(n, dtype=np.float64)
b = np.arange(n, dtype=np.float64)
s = 0.0
for _ in range(10):
    c = np.sqrt(a*a + b*b)
    s += c.sum()
print(s)
