// A tight counting loop, like sumb.gw: s += i mod 7 for i in 1..n (default 200,000,000).
#include <cstdio>
#include <cstdlib>
int main(int argc, char** argv) {
    long long n = argc > 1 ? atoll(argv[1]) : 200000000LL, s = 0;
    for (long long i = 1; i <= n; i++) s += i % 7;
    printf("%lld\n", s);
}
