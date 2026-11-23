// Naive recursive Fibonacci, like fibb.gw (n from the command line, default 35).
#include <cstdio>
#include <cstdlib>
static long long fib(long long n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }
int main(int argc, char** argv) {
    long long n = argc > 1 ? atoll(argv[1]) : 35;
    printf("%lld\n", fib(n));
}
