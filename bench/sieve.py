def main():
    n = 10_000_000
    prime = [False] * (n + 1)
    for i in range(2, n + 1):
        prime[i] = True
    for i in range(2, n + 1):
        if prime[i]:
            j = i * i
            while j <= n:
                prime[j] = False
                j += i
    count = 0
    for i in range(1, n + 1):
        if prime[i]:
            count += 1
    print(count)


main()
