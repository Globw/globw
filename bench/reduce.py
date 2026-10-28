import numpy as np
n = 50_000_000
a = np.arange(n, dtype=np.float64)
s = 0.0
for _ in range(20):
    s += a.sum() + np.linalg.norm(a) + a.mean()
print(s)
