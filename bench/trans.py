import numpy as np
n = 20_000_000
a = np.arange(n, dtype=np.float64) + 1.0
s = 0.0
for _ in range(10):
    r = np.exp(a*1e-7) + np.log(a) + np.sin(a)
    s += r.sum()
print(round(s))
