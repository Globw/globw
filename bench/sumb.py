def main():
    s = 0
    for i in range(1, 200_000_001):
        s += i % 7
    print(s)
main()
