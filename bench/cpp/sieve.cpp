// Sieve of Eratosthenes up to n (default 10,000,000), like sieve.gw: prints the number of primes.
#include <cstdio>
#include <cstdlib>
#include <vector>
int main(int argc, char** argv) {
    long long n = argc > 1 ? atoll(argv[1]) : 10000000LL;
    std::vector<bool> prime(n + 1, true);
    prime[0] = prime[1] = false;
    for (long long i = 2; i <= n; i++)
        if (prime[i])
            for (long long j = i * i; j <= n; j += i) prime[j] = false;
    long long primes = 0;
    for (long long i = 1; i <= n; i++) primes += prime[i];
    printf("%lld\n", primes);
}
