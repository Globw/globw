// Like reduce.gw: sum + norm + mean of 50 M values, 20 times.
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <vector>
int main(int argc, char** argv) {
    size_t n = argc > 1 ? strtoull(argv[1], 0, 10) : 50000000;
    std::vector<double> a(n);
    for (size_t i = 0; i < n; i++) a[i] = (double)i;
    double s = 0;
    for (int k = 0; k < 20; k++) {
        double sum = 0, sq = 0;
        for (size_t i = 0; i < n; i++) sum += a[i];
        for (size_t i = 0; i < n; i++) sq += a[i] * a[i];
        s += sum + std::sqrt(sq) + sum / n;
    }
    printf("%.17g\n", s);
}
