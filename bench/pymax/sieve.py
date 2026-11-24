n = 10_000_000
prime = bytearray([1]) * (n + 1)
prime[0] = prime[1] = 0
i = 2
while i * i <= n:
    if prime[i]:
        prime[i*i::i] = bytes(len(range(i*i, n + 1, i)))
    i += 1
print(prime.count(1))
