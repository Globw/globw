import numpy as np
n = 1200
a = np.ones((n, n)); b = np.ones((n, n))
s = 0.0
for _ in range(10):
    c = a @ b
    s += c.sum()
print(s)
