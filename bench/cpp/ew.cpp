// Like ew.gw: c = sqrt(a*a + b*b) over 20 M values, summed, 10 times. Plain loops, one core.
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <vector>
int main(int argc, char** argv) {
    size_t n = argc > 1 ? strtoull(argv[1], 0, 10) : 20000000;
    std::vector<double> a(n), b(n), c(n);
    for (size_t i = 0; i < n; i++) a[i] = b[i] = (double)i;
    double s = 0;
    for (int k = 0; k < 10; k++) {
        for (size_t i = 0; i < n; i++) c[i] = std::sqrt(a[i] * a[i] + b[i] * b[i]);
        double t = 0; for (size_t i = 0; i < n; i++) t += c[i]; s += t;
    }
    printf("%.17g\n", s);
}
