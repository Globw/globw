from math import sumprod
n = 1200
a = [[1.0] * n for _ in range(n)]
b = [[1.0] * n for _ in range(n)]
s = 0.0
for _ in range(10):
    bt = list(zip(*b))
    c = [[sumprod(row, col) for col in bt] for row in a]
    s += sum(map(sum, c))
print(s)
