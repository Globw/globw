// Like trans.gw: r = exp(a * 1e-7) + log(a) + sin(a) over 20 M values (a = 1..n), summed, 10 times.
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <vector>
int main(int argc, char** argv) {
    size_t n = argc > 1 ? strtoull(argv[1], 0, 10) : 20000000;
    std::vector<double> a(n), r(n);
    for (size_t i = 0; i < n; i++) a[i] = (double)i + 1.0;
    double s = 0;
    for (int k = 0; k < 10; k++) {
        for (size_t i = 0; i < n; i++) r[i] = std::exp(a[i] * 0.0000001) + std::log(a[i]) + std::sin(a[i]);
        double t = 0; for (size_t i = 0; i < n; i++) t += r[i]; s += t;
    }
    printf("%.0f\n", std::round(s));
}
